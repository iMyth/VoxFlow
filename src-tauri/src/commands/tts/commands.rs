use std::sync::Mutex;

use tauri::{Emitter, Manager};

use crate::core::cancel_token::CancellationToken;
use crate::core::db::Database;
use crate::core::error::AppError;
use crate::core::models::{AudioFragment, TtsBatchProgress, VoiceConfig};
use log::{error, info, warn};

use super::core::{batch_tts_one, call_tts};
use super::utils::{
    build_audio_path, get_audio_duration, reencode_with_ffmpeg, resolve_model,
};

#[tauri::command]
pub async fn generate_tts(
    app: tauri::AppHandle,
    db: tauri::State<'_, Mutex<Database>>,
    project_id: String,
    line_id: String,
    text: String,
    voice_config: VoiceConfig,
    instructions: Option<String>,
    api_key: String,
) -> Result<AudioFragment, AppError> {
    info!(
        "[TTS] generate_tts: project={}, line={}, voice={}",
        project_id, line_id, voice_config.voice_name
    );

    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::FileSystem(e.to_string()))?;
    let audio_path = build_audio_path(&app_data_dir, &project_id, &line_id);
    if let Some(p) = audio_path.parent() {
        std::fs::create_dir_all(p).map_err(|e| AppError::FileSystem(format!("mkdir: {}", e)))?;
    }

    let has_instr = instructions.as_ref().map(|i| !i.trim().is_empty()).unwrap_or(false);
    let model = resolve_model(&voice_config, has_instr);

    let audio_bytes = call_tts(
        &text,
        &voice_config,
        instructions.as_deref(),
        &api_key,
        &model,
    )
    .await?;

    info!("[TTS] Got {} bytes for line={}", audio_bytes.len(), line_id);

    std::fs::write(&audio_path, &audio_bytes)
        .map_err(|e| AppError::FileSystem(format!("write: {}", e)))?;
    reencode_with_ffmpeg(&audio_path, &line_id).await;

    let duration_ms = get_audio_duration(&audio_path).await;
    let fragment = AudioFragment {
        id: uuid::Uuid::new_v4().to_string(),
        project_id: project_id.clone(),
        line_id: line_id.clone(),
        file_path: audio_path.to_string_lossy().to_string(),
        duration_ms,
        source: "tts".to_string(),
    };

    let db = db.lock().map_err(|e| AppError::Database(e.to_string()))?;
    db.upsert_audio_fragment(&fragment)?;
    info!("[TTS] generate_tts done: line={}", line_id);
    Ok(fragment)
}

#[tauri::command]
pub async fn generate_all_tts(
    app: tauri::AppHandle,
    db: tauri::State<'_, Mutex<Database>>,
    cancel_token: tauri::State<'_, CancellationToken>,
    project_id: String,
    api_key: String,
) -> Result<usize, AppError> {
    info!("[TTS] generate_all_tts: project={}", project_id);

    cancel_token.reset();

    let (script_lines, fragments, characters) = {
        let db = db.lock().map_err(|e| AppError::Database(e.to_string()))?;
        let lines = db.load_script(&project_id)?;
        let frags = db.list_audio_fragments(&project_id)?;
        let chars = db.list_characters(&project_id)?;
        info!(
            "[TTS] {} lines, {} existing, {} chars",
            lines.len(),
            frags.len(),
            chars.len()
        );
        (lines, frags, chars)
    };

    let existing: std::collections::HashSet<String> =
        fragments.iter().map(|f| f.line_id.clone()).collect();

    let char_map: std::collections::HashMap<String, VoiceConfig> = characters
        .iter()
        .map(|c| {
            (
                c.id.clone(),
                VoiceConfig {
                    voice_name: c.voice_name.clone(),
                    tts_model: c.tts_model.clone(),
                    speed: c.speed,
                    pitch: c.pitch,
                },
            )
        })
        .collect();

    let default_vc = VoiceConfig {
        voice_name: String::new(),
        tts_model: String::new(),
        speed: 1.0,
        pitch: 1.0,
    };

    struct LineInfo {
        id: String,
        text: String,
        instructions: String,
        vc: VoiceConfig,
    }

    let missing: Vec<LineInfo> = script_lines
        .iter()
        .filter(|l| !existing.contains(&l.id) && !l.text.trim().is_empty())
        .map(|l| {
            let vc = l
                .character_id
                .as_ref()
                .and_then(|cid| char_map.get(cid))
                .cloned()
                .unwrap_or_else(|| default_vc.clone());
            LineInfo {
                id: l.id.clone(),
                text: l.text.clone(),
                instructions: l.instructions.clone(),
                vc,
            }
        })
        .collect();

    if missing.is_empty() {
        info!("[TTS] Nothing to generate");
        return Ok(0);
    }

    let total = missing.len();
    info!("[TTS] {} lines to generate", total);

    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::FileSystem(e.to_string()))?;

    let mut success_count = 0usize;

    for line in &missing {
        if cancel_token.is_cancelled() {
            info!("[TTS] batch TTS cancelled at line={}", line.id);
            let _ = app.emit("tts-cancelled", &());
            return Ok(success_count);
        }

        let has_instr = !line.instructions.is_empty();
        let model = resolve_model(&line.vc, has_instr);
        let instr: Option<&str> = if has_instr {
            Some(&line.instructions)
        } else {
            None
        };

        info!(
            "[TTS][batch] line={}, voice={}, model={}",
            line.id, line.vc.voice_name, model
        );

        let task_result = tokio::time::timeout(
            std::time::Duration::from_secs(600),
            batch_tts_one(&line.text, &line.vc, instr, &model, &api_key),
        )
        .await;

        let audio = match task_result {
            Ok(Ok(bytes)) => bytes,
            Ok(Err(e)) => {
                error!("[TTS][batch] line {} failed: {}", line.id, e);
                let _ = app.emit(
                    "tts-batch-progress",
                    TtsBatchProgress {
                        current: missing.iter().position(|l| l.id == line.id).unwrap_or(0) + 1,
                        total,
                        line_id: line.id.clone(),
                        success: false,
                        error: Some(e.to_string()),
                    },
                );
                continue;
            }
            Err(_) => {
                error!("[TTS][batch] line {} timed out", line.id);
                let _ = app.emit(
                    "tts-batch-progress",
                    TtsBatchProgress {
                        current: missing.iter().position(|l| l.id == line.id).unwrap_or(0) + 1,
                        total,
                        line_id: line.id.clone(),
                        success: false,
                        error: Some("Timeout".into()),
                    },
                );
                continue;
            }
        };

        let audio_path = build_audio_path(&app_data_dir, &project_id, &line.id);
        if let Some(p) = audio_path.parent() {
            let _ = std::fs::create_dir_all(p);
        }

        if let Err(e) = std::fs::write(&audio_path, &audio) {
            error!("[TTS][batch] write failed {}: {}", line.id, e);
            let _ = app.emit(
                "tts-batch-progress",
                TtsBatchProgress {
                    current: missing.iter().position(|l| l.id == line.id).unwrap_or(0) + 1,
                    total,
                    line_id: line.id.clone(),
                    success: false,
                    error: Some(format!("Write: {}", e)),
                },
            );
            continue;
        }

        reencode_with_ffmpeg(&audio_path, &line.id).await;
        let duration_ms = get_audio_duration(&audio_path).await;

        let fragment = AudioFragment {
            id: uuid::Uuid::new_v4().to_string(),
            project_id: project_id.clone(),
            line_id: line.id.clone(),
            file_path: audio_path.to_string_lossy().to_string(),
            duration_ms,
            source: "tts".to_string(),
        };

        let db_result = {
            let db = db.lock().map_err(|e| AppError::Database(e.to_string()))?;
            db.upsert_audio_fragment(&fragment)
        };

        let completed = missing.iter().position(|l| l.id == line.id).unwrap_or(0) + 1;
        match db_result {
            Ok(()) => {
                success_count += 1;
                info!("[TTS][batch] line {} ok ({} bytes)", line.id, audio.len());
                let _ = app.emit(
                    "tts-batch-progress",
                    TtsBatchProgress {
                        current: completed,
                        total,
                        line_id: line.id.clone(),
                        success: true,
                        error: None,
                    },
                );
            }
            Err(e) => {
                error!("[TTS][batch] DB error {}: {}", line.id, e);
                let _ = app.emit(
                    "tts-batch-progress",
                    TtsBatchProgress {
                        current: completed,
                        total,
                        line_id: line.id.clone(),
                        success: false,
                        error: Some(format!("DB: {}", e)),
                    },
                );
            }
        }
    }

    info!("[TTS] generate_all_tts done: {}/{}", success_count, total);
    Ok(success_count)
}

#[tauri::command]
pub async fn clear_audio_fragments(
    _app: tauri::AppHandle,
    db: tauri::State<'_, Mutex<Database>>,
    project_id: String,
) -> Result<(), AppError> {
    info!("[TTS] clear_audio_fragments: project={}", project_id);
    let paths = {
        let db = db.lock().map_err(|e| AppError::Database(e.to_string()))?;
        db.clear_audio_fragments(&project_id)?
    };
    // Delete audio files from disk
    for path in &paths {
        if let Err(e) = std::fs::remove_file(path) {
            warn!("[TTS] Failed to delete audio file {}: {}", path, e);
        }
    }
    info!("[TTS] Cleared {} audio fragments", paths.len());
    // Reload project in frontend by re-fetching
    Ok(())
}

#[tauri::command]
pub async fn clear_tts_fragments(
    _app: tauri::AppHandle,
    db: tauri::State<'_, Mutex<Database>>,
    project_id: String,
) -> Result<(), AppError> {
    info!("[TTS] clear_tts_fragments: project={}", project_id);
    let paths = {
        let db = db.lock().map_err(|e| AppError::Database(e.to_string()))?;
        db.clear_tts_fragments(&project_id)?
    };
    // Delete audio files from disk
    for path in &paths {
        if let Err(e) = std::fs::remove_file(path) {
            warn!("[TTS] Failed to delete audio file {}: {}", path, e);
        }
    }
    info!("[TTS] Cleared {} TTS audio fragments", paths.len());
    Ok(())
}

#[tauri::command]
pub fn cancel_tts(cancel_token: tauri::State<'_, CancellationToken>) {
    cancel_token.cancel();
}
