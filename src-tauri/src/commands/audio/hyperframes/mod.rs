//! Hyperframes video export module.
//!
//! Exports VoxFlow projects as Hyperframes HTML compositions
//! for rendering audiobook videos with synchronized text/animations.

pub mod agent;
pub mod batch_video;
pub mod ffmpeg_utils;
pub mod render;
pub mod section_audio;
pub mod section_types;
pub mod section_video;
pub mod timeline;
pub mod validation;
pub mod video_merger;

use std::sync::Mutex;

use log::info;
use serde_json::json;
use tauri::Emitter;

use crate::core::config::ConfigManager;
use crate::core::db::Database;
use crate::core::error::AppError;

use self::agent::{generate_with_agent, AgentConfig};
use self::timeline::compute_timeline;

/// Progress event payload emitted to the frontend via `hyperframes-progress`.
#[derive(Debug, Clone, serde::Serialize)]
struct HyperframesProgress {
    percent: f32,
    stage: String,
}

/// Main entry point for Hyperframes video export.
///
/// Loads project data, computes the timeline, generates HTML via agent,
/// and writes the output files to the specified directory.
#[tauri::command]
pub async fn export_hyperframes(
    app: tauri::AppHandle,
    db: tauri::State<'_, Mutex<Database>>,
    project_id: String,
    output_dir: String,
    include_audio: bool,
    audio_path: Option<String>,
    user_prompt: Option<String>,
) -> Result<String, AppError> {
    info!(
        "[Hyperframes] export_hyperframes: project={}, include_audio={}, audio_path={:?}, user_prompt={:?}",
        project_id, include_audio, audio_path, user_prompt.as_deref().map(|s| {
            let end = s.char_indices().take_while(|(i, _)| *i < 50).last().map(|(i, c)| i + c.len_utf8()).unwrap_or(0);
            &s[..end]
        })
    );

    // --- Emit initial progress ---
    let _ = app.emit(
        "hyperframes-progress",
        HyperframesProgress {
            percent: 0.0,
            stage: String::new(),
        },
    );

    // --- Load project data from database ---
    let (script_lines, fragments) = {
        let db = db.lock().map_err(|e| AppError::Database(e.to_string()))?;
        let lines = db.load_script_lines(&project_id)?;
        let frags = db.list_audio_fragments(&project_id)?;
        (lines, frags)
    };

    // --- Check if there are any audio fragments ---
    if fragments.is_empty() {
        return Err(AppError::FileSystem(
            "Please generate audio first".to_string(),
        ));
    }

    let _ = app.emit(
        "hyperframes-progress",
        HyperframesProgress {
            percent: 10.0,
            stage: String::new(),
        },
    );

    // --- Compute timeline ---
    let timeline_entries = compute_timeline(&script_lines, &fragments);

    if timeline_entries.is_empty() {
        return Err(AppError::FileSystem(
            "No timeline data available (all audio fragments missing duration)".to_string(),
        ));
    }

    let _ = app.emit(
        "hyperframes-progress",
        HyperframesProgress {
            percent: 20.0,
            stage: String::new(),
        },
    );

    // --- Generate HTML via agent ---
    let (api_endpoint, model) = {
        let db = db.lock().map_err(|e| AppError::Database(e.to_string()))?;
        let settings = db.load_settings()?;
        (settings.llm_endpoint, settings.llm_model)
    };

    let config_manager = ConfigManager::new(app.clone());
    let api_key = config_manager
        .load_api_key("llm")
        .map_err(|e| AppError::Config(format!("Failed to load API key: {}", e)))?
        .ok_or_else(|| AppError::Config("LLM API key not configured".to_string()))?;

    let agent_config = AgentConfig {
        api_endpoint,
        api_key,
        model,
    };

    let app_clone = app.clone();
    let progress_counter = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
    let on_progress: Box<dyn Fn(&str) + Send + Sync> = Box::new(move |stage: &str| {
        let count = progress_counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let internal_percent = (1.0 - (-0.15 * count as f64).exp()) * 100.0;
        let mapped_percent = 20.0 + (internal_percent / 100.0) * 60.0;
        let _ = app_clone.emit(
            "hyperframes-progress",
            HyperframesProgress {
                percent: mapped_percent.min(78.0) as f32,
                stage: stage.to_string(),
            },
        );
    });

    let html = generate_with_agent(
        &timeline_entries,
        &agent_config,
        Some(on_progress),
        user_prompt.as_deref(),
        None, // No actual audio duration available for full-project export
    )
    .await
    .map_err(AppError::LlmService)?;

    let _ = app.emit(
        "hyperframes-progress",
        HyperframesProgress {
            percent: 80.0,
            stage: String::new(),
        },
    );

    // --- Write output files ---
    let output_path = std::path::Path::new(&output_dir);

    // Create output directory and assets subdirectory
    std::fs::create_dir_all(output_path.join("assets"))
        .map_err(|e| AppError::FileSystem(format!("Failed to create output directory: {}", e)))?;

    // Write index.html
    std::fs::write(output_path.join("index.html"), &html)
        .map_err(|e| AppError::FileSystem(format!("Failed to write index.html: {}", e)))?;

    // Write meta.json
    let total_duration = timeline_entries
        .iter()
        .map(|e| e.start_time + e.duration)
        .fold(0.0_f64, f64::max);
    let meta = json!({
        "id": &project_id,
        "title": &project_id,
        "width": 1920,
        "height": 1080,
        "fps": 30,
        "duration": total_duration
    });
    let meta_str = serde_json::to_string_pretty(&meta).unwrap_or_default();
    std::fs::write(output_path.join("meta.json"), meta_str)
        .map_err(|e| AppError::FileSystem(format!("Failed to write meta.json: {}", e)))?;

    // Copy audio file if requested
    if include_audio {
        if let Some(ref src_audio_path) = audio_path {
            let src = std::path::Path::new(src_audio_path);
            if src.exists() {
                let dest = output_path.join("assets").join("audio.mp3");
                std::fs::copy(src, &dest).map_err(|e| {
                    AppError::FileSystem(format!("Failed to copy audio file: {}", e))
                })?;
                info!("[Hyperframes] Copied audio: {:?} -> {:?}", src, dest);
            } else {
                info!("[Hyperframes] Audio file not found: {:?}", src);
            }
        } else {
            info!("[Hyperframes] include_audio=true but no audio_path provided");
        }
    }

    let _ = app.emit(
        "hyperframes-progress",
        HyperframesProgress {
            percent: 100.0,
            stage: String::new(),
        },
    );

    info!("[Hyperframes] Export complete: {}", output_dir);
    Ok(output_dir)
}
