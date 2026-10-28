CREATE TABLE transactions (
    id TEXT PRIMARY KEY,
    book_id TEXT NOT NULL REFERENCES books(id),
    business_date TEXT NOT NULL,
    nature TEXT NOT NULL CHECK(nature IN ('expense','income','refund','transfer','repayment','loan','unknown')),
    amount_minor INTEGER NOT NULL CHECK(typeof(amount_minor) = 'integer' AND amount_minor > 0),
    currency TEXT NOT NULL DEFAULT 'CNY' CHECK(currency = 'CNY'),
    merchant TEXT NOT NULL DEFAULT '',
    category TEXT NOT NULL DEFAULT '',
    note TEXT NOT NULL DEFAULT '',
    review_status TEXT NOT NULL CHECK(review_status IN ('pending','confirmed')),
    lifecycle TEXT NOT NULL DEFAULT 'active' CHECK(lifecycle IN ('active','voided')),
    origin TEXT NOT NULL DEFAULT 'manual' CHECK(origin = 'manual'),
    revision INTEGER NOT NULL DEFAULT 1 CHECK(revision > 0),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    CHECK(nature <> 'unknown' OR review_status = 'pending')
);
CREATE INDEX idx_transactions_date ON transactions(book_id,lifecycle,business_date DESC,id DESC);
CREATE TABLE audit_events (
    id TEXT PRIMARY KEY,
    operation_id TEXT NOT NULL,
    entity_id TEXT NOT NULL REFERENCES transactions(id),
    action TEXT NOT NULL,
    before_json TEXT,
    after_json TEXT NOT NULL,
    reason TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);
CREATE TABLE command_requests (
    request_id TEXT PRIMARY KEY,
    payload_json TEXT NOT NULL,
    response_json TEXT NOT NULL
);
INSERT INTO schema_migrations(version,name) VALUES (2,'manual ledger and audit');
