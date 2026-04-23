use std::sync::Mutex;

use tauri::Manager;

use crate::core::config::ConfigManager;
use crate::core::db::Database;
use crate::core::error::AppError;
use crate::core::models::UserSettings;

#[tauri::command]
pub fn save_settings(
    db: tauri::State<'_, Mutex<Database>>,
    settings: UserSettings,
) -> Result<(), AppError> {
    let db = db.lock().map_err(|e| AppError::Database(e.to_string()))?;
    db.save_settings(&settings)
}

#[tauri::command]
pub fn load_settings(
    db: tauri::State<'_, Mutex<Database>>,
) -> Result<UserSettings, AppError> {
    let db = db.lock().map_err(|e| AppError::Database(e.to_string()))?;
    db.load_settings()
}

#[tauri::command]
pub fn save_api_key(
    app: tauri::AppHandle,
    service: String,
    key: String,
) -> Result<(), AppError> {
    let config = ConfigManager::new(app);
    config.save_api_key(&service, &key)
}

#[tauri::command]
pub fn load_api_key(
    app: tauri::AppHandle,
    service: String,
) -> Result<Option<String>, AppError> {
    let config = ConfigManager::new(app);
    config.load_api_key(&service)
}

/// Read an audio file and return its contents as base64-encoded string.
#[tauri::command]
pub fn read_audio_file(app: tauri::AppHandle, file_path: String) -> Result<String, AppError> {
    use base64::Engine;

    // Path validation: only allow reading files within the app data directory
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e: tauri::Error| AppError::FileSystem(e.to_string()))?;
    let canonical_app_data = app_data_dir
        .canonicalize()
        .map_err(|e| AppError::FileSystem(format!("Cannot canonicalize app data dir: {}", e)))?;

    let requested = std::path::Path::new(&file_path);
    let canonical_path = requested
        .canonicalize()
        .map_err(|e| AppError::FileSystem(format!("Cannot resolve path {}: {}", file_path, e)))?;

    if !canonical_path.starts_with(&canonical_app_data) {
        return Err(AppError::FileSystem(format!(
            "Access denied: path {} is outside the app data directory",
            file_path
        )));
    }

    let bytes = std::fs::read(&canonical_path).map_err(|e| {
        AppError::FileSystem(format!("Failed to read audio file {}: {}", file_path, e))
    })?;
    Ok(base64::engine::general_purpose::STANDARD.encode(&bytes))
}

/// Read a text file from an arbitrary path and return its contents as a string.
/// Used for importing script text files chosen by the user.
#[tauri::command]
pub fn read_text_file(file_path: String) -> Result<String, AppError> {
    std::fs::read_to_string(&file_path).map_err(|e| {
        AppError::FileSystem(format!("Failed to read text file {}: {}", file_path, e))
    })
}
