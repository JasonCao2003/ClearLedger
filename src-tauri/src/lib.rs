mod application;
mod commands;
mod domain;
mod infrastructure;

use tauri::Manager;

pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::new("info"))
        .with_target(false)
        .init();
    tauri::Builder::default()
        .setup(|app| {
            let directory = app.path().app_data_dir()?;
            let database = infrastructure::db::Database::open(&directory)?;
            app.manage(application::AppState { database });
            tracing::info!("Local database initialized");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_app_status,
            commands::save_transaction,
            commands::void_transaction,
            commands::list_transactions,
            commands::get_overview
        ])
        .run(tauri::generate_context!())
        .expect("Unable to start ledger desktop");
}
