CREATE TABLE schema_migrations (
    version INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    applied_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE TABLE books (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    currency TEXT NOT NULL CHECK (currency = 'CNY'),
    timezone TEXT NOT NULL DEFAULT 'Asia/Shanghai',
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE TABLE app_settings (
    key TEXT PRIMARY KEY,
    value_json TEXT NOT NULL
);

INSERT INTO books (id, name, currency) VALUES ('default', '我的个人账本', 'CNY');
INSERT INTO schema_migrations (version, name) VALUES (1, 'initial framework');
