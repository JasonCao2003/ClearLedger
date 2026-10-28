// Tauri requires Result for async commands borrowing State. The outer Result
// stays Ok; ApiResult preserves the existing success/error response envelope.
use crate::domain::ledger::{Entry, EntryPage, EntryQuery, Overview, SaveEntry, VoidEntry};
use crate::{
    application::{self, AppState, AppStatus},
    domain::ApiResult,
};
use tauri::State;

#[tauri::command]
pub async fn save_transaction(
    state: State<'_, AppState>,
    input: SaveEntry,
) -> Result<ApiResult<Entry>, ()> {
    let db = state.database.clone();
    Ok(
        match tauri::async_runtime::spawn_blocking(move || db.save_entry(input)).await {
            Ok(r) => r.into(),
            Err(_) => Err(crate::domain::AppError::WorkerUnavailable).into(),
        },
    )
}
#[tauri::command]
pub async fn void_transaction(
    state: State<'_, AppState>,
    input: VoidEntry,
) -> Result<ApiResult<Entry>, ()> {
    let db = state.database.clone();
    Ok(
        match tauri::async_runtime::spawn_blocking(move || db.void_entry(input)).await {
            Ok(r) => r.into(),
            Err(_) => Err(crate::domain::AppError::WorkerUnavailable).into(),
        },
    )
}
#[tauri::command]
pub async fn list_transactions(
    state: State<'_, AppState>,
    query: EntryQuery,
) -> Result<ApiResult<EntryPage>, ()> {
    let db = state.database.clone();
    Ok(
        match tauri::async_runtime::spawn_blocking(move || db.list_entries(query)).await {
            Ok(r) => r.into(),
            Err(_) => Err(crate::domain::AppError::WorkerUnavailable).into(),
        },
    )
}
#[tauri::command]
pub async fn get_overview(
    state: State<'_, AppState>,
    month: String,
) -> Result<ApiResult<Overview>, ()> {
    let db = state.database.clone();
    Ok(
        match tauri::async_runtime::spawn_blocking(move || db.overview(month)).await {
            Ok(r) => r.into(),
            Err(_) => Err(crate::domain::AppError::WorkerUnavailable).into(),
        },
    )
}

#[tauri::command]
pub async fn get_app_status(state: State<'_, AppState>) -> Result<ApiResult<AppStatus>, ()> {
    let database = state.database.clone();
    Ok(
        match tauri::async_runtime::spawn_blocking(move || application::get_app_status(&database))
            .await
        {
            Ok(result) => result.into(),
            Err(_) => Err(crate::domain::AppError::WorkerUnavailable).into(),
        },
    )
}
