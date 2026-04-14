use serde::Serialize;
use tauri::State;
use tauri_plugin_updater::Updater;

use crate::core::error::AppError;

#[derive(Debug, Serialize)]
pub struct UpdateInfo {
    pub available: bool,
    pub version: String,
    pub body: Option<String>,
}

#[tauri::command]
pub async fn check_for_updates(updater: State<'_, Updater>) -> Result<UpdateInfo, AppError> {
    let update = updater
        .check()
        .await
        .map_err(|e| AppError::FileSystem(format!("Failed to check for updates: {}", e)))?;

    match update {
        Some(update) => Ok(UpdateInfo {
            available: true,
            version: update.version,
            body: update.body.clone(),
        }),
        None => Ok(UpdateInfo {
            available: false,
            version: String::new(),
            body: None,
        }),
    }
}

#[tauri::command]
pub async fn install_update(
    updater: State<'_, Updater>,
) -> Result<(), AppError> {
    let update = updater
        .check()
        .await
        .map_err(|e| AppError::FileSystem(format!("Failed to check for updates: {}", e)))?
        .ok_or_else(|| AppError::FileSystem("No update available".to_string()))?;

    let mut downloaded = 0;
    update
        .download_and_install(
            |chunk_len, content_len| {
                downloaded += chunk_len as u64;
                log::info!("downloaded {downloaded} from {content_len:?}");
            },
            || {
                log::info!("download finished");
            },
        )
        .await
        .map_err(|e| AppError::FileSystem(format!("Failed to install update: {}", e)))?;

    Ok(())
}
