"""SQLite migration/constraint smoke checks; does not replace Rust integration tests."""
import sqlite3
import tempfile
import unittest
from contextlib import closing
from pathlib import Path

MIGRATIONS = Path(__file__).resolve().parents[1] / 'src-tauri' / 'migrations'


class SchemaTests(unittest.TestCase):
    def test_upgrade_preserves_existing_book_and_reopens(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'ledger.sqlite3'
            with closing(sqlite3.connect(path)) as connection:
                connection.executescript((MIGRATIONS / '0001_initial.sql').read_text('utf-8'))
                connection.execute("UPDATE books SET name='已有账本' WHERE id='default'")
                connection.executescript((MIGRATIONS / '0002_manual_ledger.sql').read_text('utf-8'))
            with closing(sqlite3.connect(path)) as connection:
                self.assertEqual(connection.execute('SELECT name FROM books').fetchone()[0], '已有账本')
                self.assertEqual(connection.execute('SELECT count(*) FROM schema_migrations').fetchone()[0], 2)
                self.assertEqual(connection.execute('PRAGMA integrity_check').fetchone()[0], 'ok')

    def test_constraints_and_rollback_protect_transactions(self):
        connection = sqlite3.connect(':memory:')
        self.addCleanup(connection.close)
        connection.execute('PRAGMA foreign_keys=ON')
        for file in sorted(MIGRATIONS.glob('*.sql')):
            connection.executescript(file.read_text('utf-8'))
        statement = "INSERT INTO transactions(id,book_id,business_date,nature,amount_minor,review_status) VALUES(?,?,?,?,?,?)"
        for values in [('zero','default','2026-10-28','expense',0,'confirmed'),
                       ('decimal','default','2026-10-28','expense',1.5,'confirmed'),
                       ('unknown','default','2026-10-28','unknown',100,'confirmed'),
                       ('foreign','missing','2026-10-28','expense',100,'confirmed')]:
            with self.assertRaises(sqlite3.IntegrityError):
                connection.execute(statement, values)
            connection.rollback()
        with self.assertRaises(sqlite3.IntegrityError):
            with connection:
                connection.execute(statement, ('valid','default','2026-10-28','expense',100,'confirmed'))
                connection.execute("INSERT INTO audit_events(id,operation_id,entity_id,action,after_json) VALUES('audit','op','missing','create','{}')")
        self.assertEqual(connection.execute('SELECT count(*) FROM transactions').fetchone()[0], 0)
        self.assertEqual(connection.execute('PRAGMA foreign_key_check').fetchall(), [])


if __name__ == '__main__':
    unittest.main()
