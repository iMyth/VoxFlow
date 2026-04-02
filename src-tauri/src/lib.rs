mod commands;
mod core;

use std::sync::Mutex;

use tauri::Manager;

use crate::core::db::Database;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .setup(|app| {
            let app_data_dir = app
                .path()
                .app_data_dir()
                .expect("failed to resolve app data dir");

            std::fs::create_dir_all(&app_data_dir)
                .expect("failed to create app data directory");

            let db_path = app_data_dir.join("voxflow.db");
            let db = Database::open(&db_path)
                .expect("failed to open database");
            db.migrate().expect("failed to run database migrations");

            app.manage(Mutex::new(db));

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::create_project,
            commands::list_projects,
            commands::load_project,
            commands::delete_project,
            commands::create_character,
            commands::update_character,
            commands::delete_character,
            commands::list_characters,
            commands::generate_script,
            commands::save_script,
            commands::load_script,
            commands::generate_tts,
            commands::export_audio_mix,
            commands::import_bgm,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
