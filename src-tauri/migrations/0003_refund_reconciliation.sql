CREATE TABLE IF NOT EXISTS refund_links (
    refund_id TEXT PRIMARY KEY REFERENCES transactions(id),
    expense_id TEXT REFERENCES transactions(id),
    state TEXT NOT NULL CHECK(state IN ('linked','review','unmatched','conflict','rejected','stale')),
    refund_revision INTEGER NOT NULL,
    expense_revision INTEGER,
    result_json TEXT NOT NULL,
    CHECK(refund_id <> expense_id)
);
CREATE INDEX IF NOT EXISTS idx_refund_links_expense ON refund_links(expense_id);
CREATE TABLE IF NOT EXISTS refund_semantics (
    expense_id TEXT PRIMARY KEY REFERENCES transactions(id),
    expense_revision INTEGER NOT NULL,
    mode TEXT NOT NULL CHECK(mode IN ('gross','net','unknown','conflict','stale')),
    gross_minor INTEGER CHECK(gross_minor > 0),
    refund_minor INTEGER NOT NULL CHECK(refund_minor >= 0),
    result_json TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS refund_audit_events (
    id INTEGER PRIMARY KEY,
    action TEXT NOT NULL,
    entity_id TEXT NOT NULL,
    before_json TEXT,
    after_json TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);
INSERT OR IGNORE INTO schema_migrations(version,name) VALUES(3,'refund links and amount semantics');
