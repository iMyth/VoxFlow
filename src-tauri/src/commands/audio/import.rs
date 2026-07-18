use std::sync::Mutex;

use tauri::Manager;

use crate::core::db::Database;
use crate::core::error::AppError;
use crate::core::models::AudioFragment;

#[tauri::command]
pub fn import_bgm(
    db: tauri::State<'_, Mutex<Database>>,
    app: tauri::AppHandle,
    project_id: String,
    source_path: String,
    name: String,
) -> Result<(), AppError> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::FileSystem(e.to_string()))?;

    let bgm_dir = app_data_dir.join("projects").join(&project_id).join("bgm");

    // Ensure bgm directory exists
    std::fs::create_dir_all(&bgm_dir)
        .map_err(|e| AppError::FileSystem(format!("Failed to create BGM directory: {}", e)))?;

    // Determine destination filename
    let source = std::path::Path::new(&source_path);
    let extension = source.extension().and_then(|e| e.to_str()).unwrap_or("mp3");
    let dest_filename = format!("{}.{}", name, extension);
    let dest_path = bgm_dir.join(&dest_filename);

    // Path validation: ensure source file is within app data directory
    let canonical_app_data = app_data_dir
        .canonicalize()
        .or_else(|_| {
            // App data dir may not exist yet, use the non-canonicalized path
            std::result::Result::<_, AppError>::Ok(app_data_dir.clone())
        })
        .map_err(|e| AppError::FileSystem(format!("Cannot resolve app data dir: {}", e)))?;

    // Canonicalize source path for validation
    let canonical_source = source.canonicalize().map_err(|e| {
        AppError::FileSystem(format!("Cannot resolve source path {}: {}", source_path, e))
    })?;

    if !canonical_source.starts_with(&canonical_app_data) {
        return Err(AppError::FileSystem(format!(
            "Access denied: source path {} is outside the app data directory",
            source_path
        )));
    }

    // Copy file
    std::fs::copy(&canonical_source, &dest_path)
        .map_err(|e| AppError::FileSystem(format!("Failed to copy BGM file: {}", e)))?;

    // Record in database
    let id = uuid::Uuid::new_v4().to_string();
    let file_path = dest_path.to_string_lossy().to_string();

    let db = db.lock().map_err(|e| AppError::Database(e.to_string()))?;
    db.insert_bgm(&id, &project_id, &file_path, &name)?;

    Ok(())
}

/// Import a user-recorded audio blob as an AudioFragment with source="recording".
/// The frontend sends base64-encoded webm data, which is saved to the project's recordings directory.
#[tauri::command]
pub async fn import_audio(
    app: tauri::AppHandle,
    db: tauri::State<'_, Mutex<Database>>,
    project_id: String,
    line_id: String,
    audio_data_base64: String,
) -> Result<AudioFragment, AppError> {
    use base64::{engine::general_purpose::STANDARD, Engine as _};

    log::info!(
        "[Recording] import_audio: project={}, line={}",
        project_id,
        line_id
    );

    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::FileSystem(e.to_string()))?;

    let recording_dir = app_data_dir
        .join("projects")
        .join(&project_id)
        .join("recordings");

    std::fs::create_dir_all(&recording_dir)
        .map_err(|e| AppError::FileSystem(format!("mkdir recordings: {}", e)))?;

    let audio_bytes = STANDARD
        .decode(&audio_data_base64)
        .map_err(|e| AppError::FileSystem(format!("base64 decode: {}", e)))?;

    log::info!("[Recording] decoded {} bytes", audio_bytes.len());

    // Save as .webm (native MediaRecorder format)
    let file_path = recording_dir
        .join(format!("{}.webm", line_id))
        .to_string_lossy()
        .to_string();

    std::fs::write(&file_path, &audio_bytes)
        .map_err(|e| AppError::FileSystem(format!("write recording: {}", e)))?;

    // Try to get duration via FFmpeg (ffprobe handles webm fine)
    let duration_ms =
        crate::commands::tts::get_audio_duration(std::path::Path::new(&file_path)).await;

    log::info!("[Recording] duration_ms={:?}", duration_ms);

    let fragment = AudioFragment {
        id: uuid::Uuid::new_v4().to_string(),
        project_id: project_id.clone(),
        line_id: line_id.clone(),
        file_path,
        duration_ms,
        source: "recording".to_string(),
    };

    let db = db.lock().map_err(|e| AppError::Database(e.to_string()))?;
    db.upsert_audio_fragment(&fragment)?;

    log::info!("[Recording] import_audio done: line={}", line_id);
    Ok(fragment)
}
