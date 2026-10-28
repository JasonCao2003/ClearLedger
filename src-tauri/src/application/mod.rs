use crate::{domain::AppError, infrastructure::db::Database};
use serde::Serialize;

pub struct AppState {
    pub database: Database,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppStatus {
    app_version: &'static str,
    schema_version: i64,
    database_ready: bool,
    database_path: String,
    book_name: String,
    currency: String,
}

pub fn get_app_status(database: &Database) -> Result<AppStatus, AppError> {
    let info = database.status()?;
    Ok(AppStatus {
        app_version: env!("CARGO_PKG_VERSION"),
        schema_version: info.schema_version,
        database_ready: true,
        database_path: database.path().display().to_string(),
        book_name: info.book_name,
        currency: info.currency,
    })
}
