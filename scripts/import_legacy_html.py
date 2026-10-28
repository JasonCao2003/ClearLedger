"""Import the static legacy ledger without executing scripts embedded in HTML.

Schema-v2 compatible extension tables keep immutable source/evidence and legacy
semantics. transactions.origin remains the existing v2 compatibility value;
legacy_transaction_map is the authoritative provenance for imported entries.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import sqlite3
import uuid
import zlib
from collections import Counter, defaultdict
from contextlib import closing
from datetime import date, datetime, timezone
from html.parser import HTMLParser
from pathlib import Path

NAMESPACE = uuid.UUID('b05ec2c0-1c0f-4b77-a1ee-d3b0ce84603a')
PRIORITY = {'京东': 0, '美团': 0, '支付宝': 1, '微信': 1, '工商银行': 2}
NATURES = {
    '消费候选': 'expense', '收入': 'income', '退款': 'refund',
    '内部转账': 'transfer', '还款本金': 'repayment',
    '消费待确认': 'expense', '收入待确认': 'income',
    '往来待确认': 'loan', '还款待拆分': 'repayment',
    '退款状态待核实': 'unknown', '资产转移待确认': 'unknown',
    '不计收支': 'unknown',
}
EXTENSION_SQL = '''
CREATE TABLE IF NOT EXISTS legacy_imports (
    id TEXT PRIMARY KEY, source_sha256 TEXT NOT NULL UNIQUE,
    format_version INTEGER NOT NULL CHECK(format_version=1),
    source_name TEXT NOT NULL, html_zlib BLOB NOT NULL,
    summary_json TEXT NOT NULL, issues_json TEXT NOT NULL,
    report_json TEXT NOT NULL,
    imported_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);
CREATE TABLE IF NOT EXISTS legacy_source_files (
    import_id TEXT NOT NULL REFERENCES legacy_imports(id),
    file_name TEXT NOT NULL, original_path TEXT,
    sha256 TEXT, metadata_json TEXT NOT NULL,
    PRIMARY KEY(import_id,file_name)
);
CREATE TABLE IF NOT EXISTS legacy_records (
    legacy_id TEXT PRIMARY KEY,
    import_id TEXT NOT NULL REFERENCES legacy_imports(id),
    source TEXT NOT NULL, file_name TEXT NOT NULL, locator TEXT NOT NULL,
    occurred_at TEXT NOT NULL, amount_minor INTEGER NOT NULL CHECK(amount_minor>=0),
    raw_json TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS legacy_groups (
    id TEXT PRIMARY KEY, import_id TEXT NOT NULL REFERENCES legacy_imports(id),
    legacy_root TEXT NOT NULL, representative_id TEXT NOT NULL REFERENCES legacy_records(legacy_id),
    legacy_nature TEXT NOT NULL, amount_minor INTEGER NOT NULL CHECK(amount_minor>=0),
    business_date TEXT NOT NULL, category TEXT NOT NULL,
    preliminary_eligible INTEGER NOT NULL CHECK(preliminary_eligible IN (0,1)),
    human_confirmed INTEGER NOT NULL CHECK(human_confirmed=0),
    pending_reason TEXT NOT NULL, group_json TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS legacy_group_members (
    group_id TEXT NOT NULL REFERENCES legacy_groups(id),
    legacy_record_id TEXT NOT NULL UNIQUE REFERENCES legacy_records(legacy_id),
    PRIMARY KEY(group_id,legacy_record_id)
);
CREATE TABLE IF NOT EXISTS legacy_transaction_map (
    group_id TEXT PRIMARY KEY REFERENCES legacy_groups(id),
    transaction_id TEXT NOT NULL UNIQUE REFERENCES transactions(id),
    origin TEXT NOT NULL CHECK(origin='legacy_html'),
    initial_revision INTEGER NOT NULL CHECK(initial_revision=1)
);
CREATE TABLE IF NOT EXISTS legacy_candidates (
    import_id TEXT NOT NULL REFERENCES legacy_imports(id), ordinal INTEGER NOT NULL,
    a TEXT NOT NULL REFERENCES legacy_records(legacy_id),
    b TEXT NOT NULL REFERENCES legacy_records(legacy_id),
    suggested INTEGER NOT NULL CHECK(suggested IN (0,1)), evidence_json TEXT NOT NULL,
    PRIMARY KEY(import_id,ordinal)
);
CREATE INDEX IF NOT EXISTS idx_legacy_groups_import ON legacy_groups(import_id);
CREATE INDEX IF NOT EXISTS idx_legacy_records_file ON legacy_records(import_id,file_name);
'''


def dumps(value):
    return json.dumps(value, ensure_ascii=False, separators=(',', ':'), sort_keys=True)


class PayloadParser(HTMLParser):
    def __init__(self):
        super().__init__(convert_charrefs=False)
        self.active = False
        self.parts = []
        self.matches = 0

    def handle_starttag(self, tag, attrs):
        if tag == 'script':
            attributes = dict(attrs)
            self.active = attributes.get('id') == 'ledger-data'
            if self.active:
                if attributes.get('type') != 'application/json':
                    raise ValueError('ledger-data 必须是 JSON 数据块')
                self.matches += 1

    def handle_endtag(self, tag):
        if tag == 'script':
            self.active = False

    def handle_data(self, data):
        if self.active:
            self.parts.append(data)


def read_html(path):
    content = Path(path).read_bytes()
    parser = PayloadParser()
    parser.feed(content.decode('utf-8-sig'))
    if parser.matches != 1:
        raise ValueError('HTML 必须包含且仅包含一个 ledger-data 数据块')
    payload = json.loads(''.join(parser.parts))
    validate_payload(payload)
    return content, payload


def validate_payload(payload):
    records, candidates = payload['records'], payload['candidates']
    if not isinstance(records, list) or not isinstance(candidates, list) or not records:
        raise ValueError('账单数据格式无效或为空')
    ids = set()
    for record in records:
        identifier = record['id']
        if not isinstance(identifier, str) or not identifier or identifier in ids:
            raise ValueError('原始记录 ID 无效或重复')
        ids.add(identifier)
        if type(record['amount']) is not int or not 0 <= record['amount'] <= 9223372036854775807:
            raise ValueError(f'原始金额无效：{identifier}')
        parsed = datetime.fromisoformat(record['time'])
        if parsed.date().isoformat() != record['time'][:10]:
            raise ValueError(f'原始日期无效：{identifier}')
        if record['source'] not in PRIORITY or record['nature'] not in NATURES:
            raise ValueError(f'未知渠道或性质：{identifier}')
        for field in ['file', 'merchant', 'title', 'category', 'direction']:
            if not isinstance(record[field], str):
                raise ValueError(f'原始文本字段无效：{identifier}')
    for candidate in candidates:
        if candidate['a'] not in ids or candidate['b'] not in ids:
            raise ValueError('关联候选引用缺失的原始记录')
        if not isinstance(candidate.get('suggested'), bool):
            raise ValueError('关联候选缺少布尔建议标记')


def build_groups(payload):
    """Mirror rebuild() for the HTML-embedded initial state (no localStorage)."""
    records = payload['records']
    parent = {record['id']: record['id'] for record in records}

    def root(identifier):
        while parent[identifier] != identifier:
            parent[identifier] = parent[parent[identifier]]
            identifier = parent[identifier]
        return identifier

    for candidate in payload['candidates']:
        if candidate['suggested']:
            a, b = root(candidate['a']), root(candidate['b'])
            if a != b:
                parent[b] = a
    buckets = defaultdict(list)
    for record in records:
        buckets[root(record['id'])].append(record)
    disputed = set()
    for candidate in payload['candidates']:
        a, b = root(candidate['a']), root(candidate['b'])
        if a != b:
            disputed.update((a, b))
    groups = []
    for identifier, members in buckets.items():
        members.sort(key=lambda record: (PRIORITY[record['source']], record['time']))
        representative = members[0]
        varying = len({record['amount'] for record in members}) > 1
        uncertain = representative['nature'] not in ['消费候选', '收入', '内部转账', '还款本金', '不计收支']
        pending = identifier in disputed or uncertain or varying
        reason = ('仍有跨组重复候选' if identifier in disputed else
                  '合并凭证金额不同，需确认交易总额' if varying else
                  representative.get('reason', '') if uncertain else '')
        member_ids = sorted(record['id'] for record in members)
        groups.append({
            'id': str(uuid.uuid5(NAMESPACE, 'group:' + '|'.join(member_ids))),
            'legacy_root': identifier,
            'representative_id': representative['id'],
            'member_ids': member_ids,
            'nature': representative['nature'], 'amount': representative['amount'],
            'date': representative['time'][:10], 'category': representative['category'],
            'merchant': representative['merchant'] or representative['source'],
            'title': representative['title'], 'pending': pending, 'reason': reason,
        })
    return groups


def summarize(payload, groups):
    monthly = {}
    totals = Counter()
    for group in groups:
        month = group['date'][:7]
        bucket = monthly.setdefault(month, Counter())
        bucket['groups'] += 1
        if group['pending']:
            bucket['legacy_pending_groups'] += 1
        else:
            key = {'消费候选': 'expense_minor', '收入': 'income_minor', '退款': 'refund_minor'}.get(group['nature'])
            if key:
                bucket[key] += group['amount']
                totals[key] += group['amount']
    def amounts(counter):
        return {name: str(counter[name]) for name in ['expense_minor', 'income_minor', 'refund_minor']}
    return {
        'raw_records': len(payload['records']), 'groups': len(groups),
        'posting_transactions': sum(group['amount'] > 0 for group in groups),
        'zero_amount_groups_preserved_without_posting': sum(group['amount'] == 0 for group in groups),
        'raw_zero_amount_records': sum(record['amount'] == 0 for record in payload['records']),
        'candidate_pairs': len(payload['candidates']),
        'suggested_pairs': sum(candidate['suggested'] for candidate in payload['candidates']),
        'legacy_pending_groups': sum(group['pending'] for group in groups),
        'legacy_preliminary_eligible_groups': sum(not group['pending'] for group in groups),
        'human_confirmed_transactions': 0,
        'sources': dict(Counter(record['source'] for record in payload['records'])),
        'date_range': [min(group['date'] for group in groups), max(group['date'] for group in groups)],
        'legacy_preliminary_totals': amounts(totals),
        'monthly': {month: {**dict(values), **amounts(values)} for month, values in sorted(monthly.items())},
        'policy': '全部导入交易保留待核实状态；旧账本初步可计入标记单独保留，不冒充人工确认。',
    }


def extension_exists(connection):
    return connection.execute("SELECT 1 FROM sqlite_master WHERE type='table' AND name='legacy_imports'").fetchone() is not None


def apply_import(html_path, database_path, report_path=None):
    html_path, database_path = Path(html_path).resolve(), Path(database_path).resolve()
    if not database_path.is_file():
        raise ValueError('数据库不存在，请先启动桌面应用创建数据库')
    content, payload = read_html(html_path)
    groups = build_groups(payload)
    sha256 = hashlib.sha256(content).hexdigest()
    import_id = str(uuid.uuid5(NAMESPACE, 'document:' + sha256))
    report = summarize(payload, groups)
    report.update(source_sha256=sha256, database=str(database_path), import_id=import_id)
    with closing(sqlite3.connect(database_path, timeout=10)) as connection:
        connection.execute('PRAGMA foreign_keys=ON')
        connection.execute('PRAGMA synchronous=FULL')
        version = connection.execute('PRAGMA user_version').fetchone()[0]
        if version not in (2, 3):
            raise ValueError(f'此迁移器仅兼容 schema v2/v3，当前为 {version}')
        if connection.execute('PRAGMA quick_check').fetchone()[0] != 'ok':
            raise ValueError('数据库完整性检查失败')
        if extension_exists(connection):
            saved = connection.execute('SELECT report_json FROM legacy_imports WHERE source_sha256=?', (sha256,)).fetchone()
            if saved:
                result = json.loads(saved[0])
                result['already_imported'] = True
                if report_path:
                    Path(report_path).write_text(json.dumps(result, ensure_ascii=False, indent=2), encoding='utf-8')
                return result
        backup_dir = database_path.parent / 'backups'
        backup_dir.mkdir(exist_ok=True)
        backup_path = backup_dir / f'before-legacy-import-{datetime.now(timezone.utc):%Y%m%dT%H%M%SZ}-{uuid.uuid4().hex[:8]}.sqlite3'
        with closing(sqlite3.connect(backup_path)) as backup:
            connection.backup(backup)
            if backup.execute('PRAGMA integrity_check').fetchone()[0] != 'ok':
                raise ValueError('导入前快照校验失败')
        report['backup_path'] = str(backup_path)
        # BEGIN included in executescript: DDL and all inserts roll back together.
        try:
            connection.executescript('BEGIN IMMEDIATE;\n' + EXTENSION_SQL)
            # Check again after acquiring the write lock.
            if connection.execute('SELECT 1 FROM legacy_imports WHERE source_sha256=?', (sha256,)).fetchone():
                raise ValueError('另一个任务已导入此文件，请重试以读取结果')
            existing_count = connection.execute('SELECT count(*) FROM transactions').fetchone()[0]
            report['existing_transactions_preserved'] = existing_count
            report['provenance'] = 'legacy_transaction_map.origin=legacy_html；transactions.origin 保留 schema v2 固定兼容值 manual，不代表人工确认。'
            connection.execute('INSERT INTO legacy_imports(id,source_sha256,format_version,source_name,html_zlib,summary_json,issues_json,report_json) VALUES(?,?,1,?,?,?,?,?)',
                               (import_id, sha256, html_path.name, zlib.compress(content), dumps(payload['summary']), dumps(payload['issues']), dumps(report)))
            for metadata in payload['summary']['files']:
                source = (html_path.parent / metadata['file']).resolve()
                organized_source = (html_path.parent / 'data' / 'raw' / metadata['file']).resolve()
                if not source.is_file() and organized_source.is_relative_to(html_path.parent) and organized_source.is_file():
                    source = organized_source
                # Metadata in HTML may not request arbitrary local file access.
                in_workspace = source.is_relative_to(html_path.parent)
                present = in_workspace and source.is_file()
                file_sha = hashlib.sha256(source.read_bytes()).hexdigest() if present else None
                connection.execute('INSERT INTO legacy_source_files VALUES(?,?,?,?,?)',
                                   (import_id, metadata['file'], str(source) if present else None, file_sha, dumps(metadata)))
            for record in payload['records']:
                connection.execute('INSERT INTO legacy_records VALUES(?,?,?,?,?,?,?,?)',
                                   (record['id'], import_id, record['source'], record['file'], str(record['line']), record['time'], record['amount'], dumps(record)))
            for ordinal, candidate in enumerate(payload['candidates']):
                connection.execute('INSERT INTO legacy_candidates VALUES(?,?,?,?,?,?)',
                                   (import_id, ordinal, candidate['a'], candidate['b'], int(candidate['suggested']), dumps(candidate)))
            for group in groups:
                connection.execute('INSERT INTO legacy_groups VALUES(?,?,?,?,?,?,?,?,?,0,?,?)',
                                   (group['id'], import_id, group['legacy_root'], group['representative_id'], group['nature'], group['amount'], group['date'], group['category'], int(not group['pending']), group['reason'], dumps(group)))
                connection.executemany('INSERT INTO legacy_group_members VALUES(?,?)', [(group['id'], identifier) for identifier in group['member_ids']])
                if group['amount'] == 0:
                    continue
                transaction_id = str(uuid.uuid5(NAMESPACE, 'transaction:' + group['id']))
                nature = NATURES[group['nature']]
                note = '\n'.join(part for part in [group['title'], f"旧账本导入 · 原性质：{group['nature']}", group['reason']] if part)
                # v2 CHECK fixes origin to manual. The explicit provenance mapping
                # below records legacy_html without changing the compiled schema.
                connection.execute("INSERT INTO transactions(id,book_id,business_date,nature,amount_minor,merchant,category,note,review_status) VALUES(?,'default',?,?,?,?,?,?,'pending')",
                                   (transaction_id, group['date'], nature, group['amount'], group['merchant'], group['category'], note))
                connection.execute("INSERT INTO legacy_transaction_map VALUES(?,?,'legacy_html',1)", (group['id'], transaction_id))
                after = {'id': transaction_id, 'businessDate': group['date'], 'nature': nature,
                         'amountMinor': str(group['amount']), 'merchant': group['merchant'],
                         'category': group['category'], 'note': note, 'reviewStatus': 'pending',
                         'lifecycle': 'active', 'revision': 1}
                connection.execute("INSERT INTO audit_events(id,operation_id,entity_id,action,after_json,reason) VALUES(?,?,?,'legacy_import',?,?)",
                                   (str(uuid.uuid4()), import_id, transaction_id, dumps(after), '迁移 HTML 内嵌数据；未读取浏览器修正'))
            counts = {
                'records': connection.execute('SELECT count(*) FROM legacy_records WHERE import_id=?', (import_id,)).fetchone()[0],
                'members': connection.execute('SELECT count(*) FROM legacy_group_members m JOIN legacy_groups g ON g.id=m.group_id WHERE g.import_id=?', (import_id,)).fetchone()[0],
                'groups': connection.execute('SELECT count(*) FROM legacy_groups WHERE import_id=?', (import_id,)).fetchone()[0],
                'transactions': connection.execute('SELECT count(*) FROM legacy_transaction_map m JOIN legacy_groups g ON g.id=m.group_id WHERE g.import_id=?', (import_id,)).fetchone()[0],
                'total_transactions': connection.execute('SELECT count(*) FROM transactions').fetchone()[0],
            }
            if counts != {'records': report['raw_records'], 'members': report['raw_records'], 'groups': report['groups'], 'transactions': report['posting_transactions'], 'total_transactions': existing_count + report['posting_transactions']}:
                raise ValueError('导入记录守恒校验失败')
            if connection.execute('PRAGMA foreign_key_check').fetchall():
                raise ValueError('外键校验失败')
            for group in groups:
                saved = connection.execute('SELECT amount_minor,business_date,nature,review_status FROM transactions t JOIN legacy_transaction_map m ON m.transaction_id=t.id WHERE m.group_id=?', (group['id'],)).fetchone()
                if group['amount'] > 0 and saved != (group['amount'], group['date'], NATURES[group['nature']], 'pending'):
                    raise ValueError('逐组金额、日期或性质校验失败')
            report['verification'] = {'record_conservation': True, 'per_group_amounts': True, 'foreign_keys': True, 'schema_version_unchanged': version}
            connection.execute('UPDATE legacy_imports SET report_json=? WHERE id=?', (dumps(report), import_id))
            connection.commit()
        except Exception:
            connection.rollback()
            raise
        report['integrity_check'] = connection.execute('PRAGMA integrity_check').fetchone()[0]
        if report['integrity_check'] != 'ok':
            raise ValueError('导入后完整性检查失败；请保留备份并检查数据库')
        # Persist final verification so idempotent re-runs return a full report.
        with connection:
            connection.execute('UPDATE legacy_imports SET report_json=? WHERE id=?', (dumps(report), import_id))
    if report_path:
        Path(report_path).write_text(json.dumps(report, ensure_ascii=False, indent=2), encoding='utf-8')
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--html', type=Path, required=True)
    parser.add_argument('--db', type=Path, default=Path(os.environ.get('APPDATA', '.')) / 'com.jason.personalledger' / 'ledger.sqlite3')
    parser.add_argument('--apply', action='store_true', help='备份并实际写入；默认只预览')
    parser.add_argument('--report', type=Path)
    args = parser.parse_args()
    if args.apply:
        report = apply_import(args.html, args.db, args.report)
    else:
        _, payload = read_html(args.html)
        report = summarize(payload, build_groups(payload))
        report['preview_only'] = True
        if args.report:
            args.report.write_text(json.dumps(report, ensure_ascii=False, indent=2), encoding='utf-8')
    # Keep transaction details out of terminal output.
    print(json.dumps({key: value for key, value in report.items() if key != 'monthly'}, ensure_ascii=False, indent=2))


if __name__ == '__main__':
    main()
