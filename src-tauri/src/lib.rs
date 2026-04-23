mod commands;
pub mod core;

use std::sync::Mutex;

use tauri::Manager;

use crate::core::cancel_token::CancellationToken;
use crate::core::db::Database;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            let app_data_dir = match app.path().app_data_dir() {
                Ok(dir) => dir,
                Err(e) => {
                    log::error!("failed to resolve app data dir: {}", e);
                    return Err(format!("failed to resolve app data dir: {}", e).into());
                }
            };

            if let Err(e) = std::fs::create_dir_all(&app_data_dir) {
                log::error!("failed to create app data directory: {}", e);
                // Continue anyway — db might still open
            }

            let db_path = app_data_dir.join("voxflow.db");
            let db = match Database::open(&db_path) {
                Ok(db) => db,
                Err(e) => {
                    log::error!("failed to open database at {:?}: {}", db_path, e);
                    return Err(format!("failed to open database: {}", e).into());
                }
            };
            if let Err(e) = db.migrate() {
                log::error!("failed to run database migrations: {}", e);
                return Err(format!("failed to run database migrations: {}", e).into());
            }

            app.manage(Mutex::new(db));
            app.manage(commands::audio::AudioPlayer::new());
            app.manage(CancellationToken::default());

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::create_project,
            commands::list_projects,
            commands::list_projects_with_stats,
            commands::load_project,
            commands::delete_project,
            commands::save_outline,
            commands::create_character,
            commands::update_character,
            commands::delete_character,
            commands::list_characters,
            commands::list_all_project_characters,
            commands::import_characters,
            commands::generate_script,
            commands::analyze_outline,
            commands::run_agent_pipeline,
            commands::run_analysis_step,
            commands::run_generation_step,
            commands::run_revision_step,
            commands::story_recall,
            commands::build_story_kb,
            commands::cancel_llm,
            commands::save_script,
            commands::load_script,
            commands::generate_tts,
            commands::generate_all_tts,
            commands::cancel_tts,
            commands::clear_audio_fragments,
            commands::clear_tts_fragments,
            commands::export_audio_mix,
            commands::import_bgm,
            commands::play_audio,
            commands::stop_audio,
            commands::set_audio_volume,
            commands::get_audio_position,
            commands::get_audio_duration_ms,
            commands::seek_audio,
            commands::import_audio,
            commands::save_settings,
            commands::load_settings,
            commands::save_api_key,
            commands::load_api_key,
            commands::read_audio_file,
            commands::create_voice,
            commands::preview_voice,
            commands::check_for_updates,
            commands::install_update,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
