mod desktop;

use tauri::Manager;

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let directory = app.path().app_local_data_dir()?.join("desktop");
            app.manage(desktop::Database::open(&directory)?);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            desktop::load_ledger,
            desktop::import_ledger,
            desktop::save_settings,
        ])
        .run(tauri::generate_context!())
        .expect("Unable to start ClearLedger");
}
