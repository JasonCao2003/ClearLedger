use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{fs, path::Path, sync::{Arc, Mutex}};
use tauri::State;

pub struct Database(Arc<Mutex<Connection>>);

#[derive(Debug, thiserror::Error)]
pub enum LedgerError {
    #[error("无法创建本机账本目录")]
    Directory(#[from] std::io::Error),
    #[error("无法读写本机账本")]
    Sql(#[from] rusqlite::Error),
    #[error("账本数据格式不正确")]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Invalid(&'static str),
}

impl Database {
    pub fn open(directory: &Path) -> Result<Self, LedgerError> {
        fs::create_dir_all(directory)?;
        let connection = Connection::open(directory.join("ledger.sqlite3"))?;
        connection.busy_timeout(std::time::Duration::from_secs(5))?;
        connection.execute_batch(
            "BEGIN;
             CREATE TABLE IF NOT EXISTS imported_rows (
                 id TEXT PRIMARY KEY, amount INTEGER NOT NULL CHECK (amount >= 0),
                 payload TEXT NOT NULL, modified INTEGER NOT NULL
             );
             CREATE TABLE IF NOT EXISTS import_reports (
                 digest TEXT PRIMARY KEY, payload TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS desktop_settings (
                 id INTEGER PRIMARY KEY CHECK (id = 1), payload TEXT NOT NULL
             );
             COMMIT;",
        )?;
        Ok(Self(Arc::new(Mutex::new(connection))))
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Row {
    id: String,
    source: String,
    time: String,
    amount: i64,
    direction: String,
    kind: String,
    state: String,
    title: String,
    merchant: String,
    method: String,
    tx: String,
    order: String,
    account: String,
    note: String,
}

fn hex(value: &str, length: usize) -> bool {
    value.len() == length && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

impl Row {
    fn validate(&self) -> Result<(), LedgerError> {
        let timestamp = self.time.as_bytes();
        let valid_time = timestamp.len() == 19
            && timestamp.iter().enumerate().all(|(i, byte)| match i {
                4 | 7 => *byte == b'-',
                10 => *byte == b' ',
                13 | 16 => *byte == b':',
                _ => byte.is_ascii_digit(),
            });
        if !hex(&self.id, 24)
            || !valid_time
            || self.amount < 0
            || self.amount > 9_999_999_999
            || !matches!(self.source.as_str(), "支付宝" | "微信" | "美团" | "京东" | "工商银行")
            || [&self.direction, &self.kind, &self.state, &self.title, &self.merchant,
                &self.method, &self.tx, &self.order, &self.account, &self.note]
                .iter().any(|value| value.len() > 16_384)
        {
            return Err(LedgerError::Invalid("导入内容包含无效交易"));
        }
        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Report {
    source: String,
    file: String,
    start: String,
    end: String,
    count: usize,
    duplicates: usize,
    skipped: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImportBatch {
    digest: String,
    modified: i64,
    rows: Vec<Row>,
    reports: Vec<Report>,
    settings: Option<Value>,
}

#[derive(Serialize)]
pub struct Ledger {
    rows: Vec<Row>,
    reports: Vec<Report>,
    settings: Value,
}

#[derive(Serialize)]
#[serde(untagged)]
pub enum Reply<T> {
    Ok { ok: bool, data: T },
    Error { ok: bool, error: Value },
}

fn reply<T>(result: Result<T, LedgerError>) -> Reply<T> {
    match result {
        Ok(data) => Reply::Ok { ok: true, data },
        Err(error) => Reply::Error {
            ok: false,
            error: json!({
                "code": "LEDGER_ERROR",
                "message": error.to_string(),
                "traceId": uuid::Uuid::new_v4().to_string()
            }),
        },
    }
}

fn read_payloads<T: serde::de::DeserializeOwned>(
    connection: &Connection,
    query: &str,
) -> Result<Vec<T>, LedgerError> {
    let mut statement = connection.prepare(query)?;
    let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
    rows.map(|row| Ok(serde_json::from_str(&row?)?)).collect()
}

fn load(connection: &Connection) -> Result<Ledger, LedgerError> {
    let settings = connection.query_row(
        "SELECT payload FROM desktop_settings WHERE id = 1",
        [],
        |row| row.get::<_, String>(0),
    ).optional()?;
    Ok(Ledger {
        rows: read_payloads(connection, "SELECT payload FROM imported_rows ORDER BY rowid")?,
        reports: read_payloads(connection, "SELECT payload FROM import_reports ORDER BY rowid")?,
        settings: settings.map_or(Ok(json!({})), |payload| serde_json::from_str(&payload))?,
    })
}

fn import(connection: &mut Connection, mut batch: ImportBatch) -> Result<bool, LedgerError> {
    if !hex(&batch.digest, 64) || batch.modified < 0 || batch.rows.len() > 250_000
        || (batch.reports.is_empty() && (!batch.rows.is_empty() || batch.settings.is_none()))
        || batch.reports.len() > 500
        || batch.reports.iter().any(|r| r.file.len() > 1024 || r.source.len() > 64
            || r.start.len() > 10 || r.end.len() > 10
            || r.error.as_ref().is_some_and(|error| error.len() > 1024))
    {
        return Err(LedgerError::Invalid("导入批次格式不正确"));
    }

    let transaction = connection.transaction()?;
    let marker = format!("{}:0", batch.digest);
    let imported = transaction.query_row(
        "SELECT 1 FROM import_reports WHERE digest = ?1", [&marker], |_| Ok(()),
    ).optional()?.is_some();
    if imported {
        if let Some(settings) = batch.settings {
            write_settings(&transaction, settings)?;
        }
        transaction.commit()?;
        return Ok(false);
    }

    let mut duplicates = 0;
    {
        let mut existing = transaction.prepare_cached("SELECT 1 FROM imported_rows WHERE id = ?1")?;
        let mut write = transaction.prepare_cached(
            "INSERT INTO imported_rows (id, amount, payload, modified) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(id) DO UPDATE SET amount=excluded.amount, payload=excluded.payload,
                 modified=excluded.modified WHERE excluded.modified >= imported_rows.modified",
        )?;
        for row in &batch.rows {
            row.validate()?;
            if existing.query_row([&row.id], |_| Ok(())).optional()?.is_some() {
                duplicates += 1;
            }
            write.execute(params![row.id, row.amount, serde_json::to_string(row)?, batch.modified])?;
        }
    }
    if batch.reports.len() == 1 {
        batch.reports[0].duplicates += duplicates;
    }
    for (i, report) in batch.reports.into_iter().enumerate() {
        transaction.execute(
            "INSERT INTO import_reports (digest, payload) VALUES (?1, ?2)",
            params![format!("{}:{i}", batch.digest), serde_json::to_string(&report)?],
        )?;
    }
    if let Some(settings) = batch.settings {
        write_settings(&transaction, settings)?;
    }
    transaction.commit()?;
    Ok(true)
}

async fn with_database<T: Send + 'static>(
    state: State<'_, Database>,
    operation: impl FnOnce(&mut Connection) -> Result<T, LedgerError> + Send + 'static,
) -> Result<Reply<T>, ()> {
    // SQLite access is serialized, and large imports must not block the window.
    let app = state.inner().0.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let mut connection = app.lock().map_err(|_| LedgerError::Invalid("账本暂时无法访问"))?;
        operation(&mut connection)
    }).await.unwrap_or(Err(LedgerError::Invalid("账本操作未完成")));
    Ok(reply(result))
}

#[tauri::command]
pub async fn load_ledger(state: State<'_, Database>) -> Result<Reply<Ledger>, ()> {
    with_database(state, |connection| load(connection)).await
}

#[tauri::command]
pub async fn import_ledger(state: State<'_, Database>, batch: ImportBatch) -> Result<Reply<bool>, ()> {
    with_database(state, move |connection| import(connection, batch)).await
}

#[tauri::command]
pub async fn save_settings(state: State<'_, Database>, settings: Value) -> Result<Reply<()>, ()> {
    with_database(state, move |connection| write_settings(connection, settings)).await
}

fn write_settings(connection: &Connection, settings: Value) -> Result<(), LedgerError> {
    let payload = serde_json::to_string(&settings)?;
    if !settings.is_object() || payload.len() > 32_000_000 {
        return Err(LedgerError::Invalid("核对设置格式不正确"));
    }
    connection.execute(
        "INSERT INTO desktop_settings (id, payload) VALUES (1, ?1)
         ON CONFLICT(id) DO UPDATE SET payload=excluded.payload",
        [payload],
    )?;
    Ok(())
}
