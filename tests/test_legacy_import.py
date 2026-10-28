import copy
import hashlib
import json
import os
import sqlite3
import subprocess
import sys
import tempfile
import unittest
import zlib
from contextlib import closing
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'scripts'))
from import_legacy_html import apply_import, build_groups, dumps, read_html, validate_payload


def record(identifier, source='支付宝', amount=10000, nature='消费候选'):
    return dict(id=identifier, source=source, file='sample.csv', line=identifier,
                time='2026-04-30 12:00:00', amount=amount, nature=nature,
                merchant='测试商户', title='测试商品', category='日用购物',
                direction='支出', raw={'保留字段': '原值'}, reason='规则建议')


def fixture():
    records = [record('a'), record('b', '工商银行'), record('c'), record('d', amount=0), record('e', nature='收入待确认')]
    return {'records': records, 'candidates': [dict(a='a', b='b', suggested=True), dict(a='a', b='c', suggested=False)],
            'summary': {'files': [dict(file='sample.csv', source='支付宝', count=5)]}, 'issues': []}


class LegacyImportTests(unittest.TestCase):
    def test_finds_sources_after_workspace_reorganization(self):
        with tempfile.TemporaryDirectory() as directory:
            directory = Path(directory)
            db = directory / 'test.sqlite3'
            with closing(sqlite3.connect(db)) as connection:
                for migration in sorted((ROOT / 'src-tauri/migrations').glob('*.sql')):
                    connection.executescript(migration.read_text('utf-8'))
                connection.execute('PRAGMA user_version=2')
            raw = directory / 'data' / 'raw'
            raw.mkdir(parents=True)
            source = raw / 'sample.csv'
            source.write_text('synthetic,data\n1,2\n', encoding='utf-8')
            html = directory / 'ledger.html'
            html.write_text('<script id="ledger-data" type="application/json">'+dumps(fixture())+'</script>', encoding='utf-8')
            apply_import(html, db)
            with closing(sqlite3.connect(db)) as connection:
                saved = connection.execute('SELECT original_path,sha256 FROM legacy_source_files').fetchone()
                self.assertEqual(saved, (str(source.resolve()), hashlib.sha256(source.read_bytes()).hexdigest()))

    def test_preserves_evidence_zeros_existing_data_and_retries(self):
        with tempfile.TemporaryDirectory() as directory:
            directory = Path(directory)
            database = directory / 'ledger.sqlite3'
            with closing(sqlite3.connect(database)) as connection:
                for migration in sorted((ROOT / 'src-tauri/migrations').glob('*.sql')):
                    connection.executescript(migration.read_text('utf-8'))
                connection.execute('PRAGMA user_version=2')
                connection.execute("INSERT INTO transactions(id,book_id,business_date,nature,amount_minor,review_status) VALUES('existing','default','2026-04-01','income',900,'confirmed')")
                connection.commit()
            html = directory / 'ledger.html'
            data = fixture()
            content = '<script>throw new Error("MUST NOT EXECUTE")</script><script id="ledger-data" type="application/json">' + dumps(data) + '</script>'
            html.write_text(content, encoding='utf-8')
            result = apply_import(html, database)
            self.assertEqual(result['raw_records'], 5)
            self.assertEqual(result['groups'], 4)
            self.assertEqual(result['posting_transactions'], 3)
            self.assertEqual(result['zero_amount_groups_preserved_without_posting'], 1)
            self.assertTrue(Path(result['backup_path']).is_file())
            with closing(sqlite3.connect(database)) as connection:
                self.assertEqual(connection.execute('SELECT count(*) FROM transactions').fetchone()[0], 4)
                self.assertEqual(connection.execute("SELECT review_status FROM transactions WHERE id='existing'").fetchone()[0], 'confirmed')
                self.assertEqual(connection.execute("SELECT count(*) FROM transactions WHERE id<>'existing' AND review_status<>'pending'").fetchone()[0], 0)
                self.assertEqual(connection.execute('SELECT count(*) FROM legacy_group_members').fetchone()[0], 5)
                self.assertEqual(connection.execute("SELECT raw_json FROM legacy_records WHERE legacy_id='a'").fetchone()[0], dumps(data['records'][0]))
                self.assertEqual(zlib.decompress(connection.execute('SELECT html_zlib FROM legacy_imports').fetchone()[0]).decode('utf-8'), content)
                self.assertEqual(connection.execute('PRAGMA user_version').fetchone()[0], 2)
                imported_id = connection.execute('SELECT transaction_id FROM legacy_transaction_map LIMIT 1').fetchone()[0]
                connection.execute("UPDATE transactions SET note='人工修改',revision=2 WHERE id=?", (imported_id,))
                connection.commit()
            again = apply_import(html, database)
            self.assertTrue(again['already_imported'])
            with closing(sqlite3.connect(database)) as connection:
                self.assertEqual(connection.execute('SELECT count(*) FROM transactions').fetchone()[0], 4)
                self.assertEqual(connection.execute('SELECT note FROM transactions WHERE id=?', (imported_id,)).fetchone()[0], '人工修改')

    def test_collision_rolls_back_all_new_data(self):
        with tempfile.TemporaryDirectory() as directory:
            directory = Path(directory)
            db = directory / 'test.sqlite3'
            with closing(sqlite3.connect(db)) as connection:
                for migration in sorted((ROOT / 'src-tauri/migrations').glob('*.sql')):
                    connection.executescript(migration.read_text('utf-8'))
                connection.execute('PRAGMA user_version=2')
            html = directory / 'ledger.html'
            html.write_text('<script id="ledger-data" type="application/json">'+dumps(fixture())+'</script>', encoding='utf-8')
            apply_import(html, db)
            changed = fixture(); changed['records'][0]['amount'] = 20000
            html.write_text('<script id="ledger-data" type="application/json">'+dumps(changed)+'</script>', encoding='utf-8')
            with self.assertRaises(sqlite3.IntegrityError):
                apply_import(html, db)
            with closing(sqlite3.connect(db)) as connection:
                self.assertEqual(connection.execute('SELECT count(*) FROM legacy_imports').fetchone()[0], 1)
                self.assertEqual(connection.execute('SELECT count(*) FROM legacy_records').fetchone()[0], 5)
                self.assertEqual(connection.execute('PRAGMA foreign_key_check').fetchall(), [])

    def test_invalid_records_and_dangling_candidates_are_rejected(self):
        for mutation in ['negative', 'duplicate', 'dangling']:
            data = copy.deepcopy(fixture())
            if mutation == 'negative': data['records'][0]['amount'] = -1
            if mutation == 'duplicate': data['records'][1]['id'] = 'a'
            if mutation == 'dangling': data['candidates'][0]['b'] = 'missing'
            with self.assertRaises(ValueError): validate_payload(data)

    def assert_matches_reference(self, payload):
        expected = json.loads(subprocess.run(['node', str(ROOT / 'tests/legacy_reference.mjs')],
                                            input=dumps(payload), text=True, encoding='utf-8',
                                            capture_output=True, check=True).stdout)
        fields = ['legacy_root','member_ids','nature','amount','date','category','pending','reason']
        actual = [{key: group[key] for key in fields} for group in build_groups(payload)]
        self.assertEqual(actual, expected)

    def test_synthetic_fixture_matches_legacy_javascript(self):
        self.assert_matches_reference(fixture())

    @unittest.skipUnless(os.environ.get('CLEARLEDGER_LEGACY_HTML'), 'Optional local legacy HTML is not configured')
    def test_actual_html_matches_old_javascript_grouping_exactly(self):
        _, payload = read_html(Path(os.environ['CLEARLEDGER_LEGACY_HTML']))
        self.assert_matches_reference(payload)


if __name__ == '__main__':
    unittest.main()
