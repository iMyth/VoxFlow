//! Batch video generation and cancellation commands.
//!
//! Provides:
//! - `generate_all_sections`: Two-phase processing - LLM generation in parallel,
//!   rendering sequential to avoid overloading Chrome instances.
//! - `cancel_section_generation`: Cancels an active section generation using a shared
//!   CancellationToken map.

use std::collections::HashMap;
use std::sync::Mutex;

use log::info;
use tauri::{Emitter, Manager};
use tokio_util::sync::CancellationToken;

use crate::core::db::Database;
use crate::core::error::AppError;

use super::section_types::{BatchGenerationResult, SectionProgress, SectionStyleConfig};
use super::section_video::{generate_section_html, render_section_video};

/// Shared state for tracking active section generation cancellation tokens.
/// Keyed by section_id.
pub struct SectionCancelTokens(pub Mutex<HashMap<String, CancellationToken>>);

impl Default for SectionCancelTokens {
    fn default() -> Self {
        Self(Mutex::new(HashMap::new()))
    }
}

/// Generate videos for all configured sections using two-phase processing.
///
/// Phase 1 (Parallel): Generate HTML compositions for all sections simultaneously.
/// Phase 2 (Sequential): Render each HTML to video one at a time to avoid
///                       overloading system resources with multiple Chrome instances.
///
/// This approach maximizes LLM utilization while preventing memory/CPU exhaustion
/// from concurrent browser rendering.
#[tauri::command]
pub async fn generate_all_sections(
    app: tauri::AppHandle,
    db: tauri::State<'_, Mutex<Database>>,
    cancel_tokens: tauri::State<'_, SectionCancelTokens>,
    project_id: String,
    section_configs: Vec<(String, SectionStyleConfig)>,
) -> Result<BatchGenerationResult, AppError> {
    info!(
        "[Batch Video] Starting two-phase generation: project={}, configs={}",
        project_id,
        section_configs.len()
    );

    // Build a lookup map from section_id -> style_config
    let config_map: HashMap<String, SectionStyleConfig> = section_configs.into_iter().collect();

    // Load sections from DB to get proper section_order
    let sections = {
        let db_guard = db.lock().map_err(|e| AppError::Database(e.to_string()))?;
        db_guard.list_sections(&project_id)?
    };

    let total_configured = sections
        .iter()
        .filter(|s| config_map.contains_key(&s.id))
        .count();

    info!(
        "[Batch Video] Processing {} configured sections out of {} total",
        total_configured,
        sections.len()
    );

    // Track which sections we're processing
    let mut sections_to_process: Vec<(String, SectionStyleConfig)> = Vec::new();

    for section in &sections {
        let style_config = match config_map.get(&section.id) {
            Some(config) => config.clone(),
            None => continue,
        };

        // Register a cancellation token
        let cancel_token = CancellationToken::new();
        {
            let mut tokens = cancel_tokens.0.lock().map_err(|e| {
                AppError::FileSystem(format!("Failed to lock cancel tokens: {}", e))
            })?;
            tokens.insert(section.id.clone(), cancel_token.clone());
        }

        // Emit starting event
        let _ = app.emit(
            "section-video-progress",
            SectionProgress {
                section_id: section.id.clone(),
                percent: 0.0,
                stage: "starting".to_string(),
            },
        );

        sections_to_process.push((section.id.clone(), style_config));
    }

    // ===== PHASE 1: Parallel HTML Generation =====
    info!("[Batch Video] Phase 1: Generating HTML compositions in parallel...");

    let mut html_tasks = Vec::new();
    let mut failed: Vec<(String, String)> = Vec::new();

    for (section_id, style_config) in &sections_to_process {
        // Check cancellation BEFORE spawning task
        {
            let tokens = cancel_tokens.0.lock().map_err(|e| {
                AppError::FileSystem(format!("Failed to lock cancel tokens: {}", e))
            })?;
            if let Some(token) = tokens.get(section_id) {
                if token.is_cancelled() {
                    info!(
                        "[Batch Video] Section {} cancelled before HTML generation, skipping",
                        section_id
                    );
                    failed.push((section_id.clone(), "Cancelled".to_string()));
                    continue;
                }
            }
        }

        let app_clone = app.clone();
        let project_id_clone = project_id.clone();
        let section_id_clone = section_id.clone();
        let style_config_clone = style_config.clone();

        // Clone the cancellation token for this section
        let cancel_token_clone = {
            let tokens = cancel_tokens.0.lock().map_err(|e| {
                AppError::FileSystem(format!("Failed to lock cancel tokens: {}", e))
            })?;
            tokens.get(section_id).cloned()
        };

        let task = tokio::spawn(async move {
            let db_state: tauri::State<'_, Mutex<Database>> = app_clone.state();
            let result = generate_section_html(
                app_clone.clone(),
                db_state,
                project_id_clone,
                section_id_clone.clone(),
                style_config_clone,
                cancel_token_clone,
            )
            .await;
            (section_id_clone, result)
        });

        html_tasks.push(task);
    }

    // Wait for all HTML generation tasks
    let mut completed_html: Vec<String> = Vec::new();

    for task in html_tasks {
        match task.await {
            Ok((section_id, result)) => match result {
                Ok(_) => {
                    info!("[Batch Video] HTML generated for section {}", section_id);
                    completed_html.push(section_id);
                }
                Err(e) => {
                    let error_msg = e.to_string();
                    info!(
                        "[Batch Video] HTML generation failed for section {}: {}",
                        section_id, error_msg
                    );

                    // Emit failure event so frontend updates status
                    let _ = app.emit(
                        "section-video-failed",
                        serde_json::json!({
                            "section_id": section_id,
                            "error": error_msg,
                        }),
                    );

                    failed.push((section_id, error_msg));
                }
            },
            Err(e) => {
                let error_msg = format!("Task join error: {}", e);
                info!("[Batch Video] Task failed: {}", error_msg);

                // Emit failure event for unknown section
                let _ = app.emit(
                    "section-video-failed",
                    serde_json::json!({
                        "section_id": "unknown",
                        "error": error_msg,
                    }),
                );

                failed.push(("unknown".to_string(), error_msg));
            }
        }
    }

    info!(
        "[Batch Video] Phase 1 complete: {} HTML generated, {} failed",
        completed_html.len(),
        failed.len()
    );

    // ===== PHASE 2: Sequential Video Rendering =====
    info!("[Batch Video] Phase 2: Rendering videos sequentially...");

    let mut completed_videos: Vec<String> = Vec::new();

    for section_id in &completed_html {
        // Check if cancelled
        {
            let tokens = cancel_tokens.0.lock().map_err(|e| {
                AppError::FileSystem(format!("Failed to lock cancel tokens: {}", e))
            })?;
            if let Some(token) = tokens.get(section_id) {
                if token.is_cancelled() {
                    info!(
                        "[Batch Video] Section {} cancelled, skipping render",
                        section_id
                    );
                    failed.push((section_id.clone(), "Cancelled".to_string()));
                    continue;
                }
            }
        }

        // Emit render starting event
        let _ = app.emit(
            "section-video-progress",
            SectionProgress {
                section_id: section_id.clone(),
                percent: 50.0,
                stage: "rendering".to_string(),
            },
        );

        // Render this section
        let render_result =
            render_section_video(app.clone(), project_id.clone(), section_id.clone()).await;

        // Remove cancellation token
        if let Ok(mut tokens) = cancel_tokens.0.lock() {
            tokens.remove(section_id);
        }

        match render_result {
            Ok(video_result) => {
                info!("[Batch Video] Video rendered for section {}", section_id);

                // Emit completion event so frontend updates status
                let _ = app.emit(
                    "section-video-complete",
                    serde_json::json!({
                        "section_id": section_id,
                        "video_path": video_result.video_path,
                        "duration_ms": video_result.duration_ms,
                        "file_size_bytes": video_result.file_size_bytes,
                    }),
                );

                completed_videos.push(section_id.clone());
            }
            Err(e) => {
                let error_msg = e.to_string();
                info!(
                    "[Batch Video] Render failed for section {}: {}",
                    section_id, error_msg
                );

                // Emit failure event so frontend updates status
                let _ = app.emit(
                    "section-video-failed",
                    serde_json::json!({
                        "section_id": section_id,
                        "error": error_msg,
                    }),
                );

                failed.push((section_id.clone(), error_msg));
            }
        }
    }

    let result = BatchGenerationResult {
        completed: completed_videos,
        failed,
    };

    info!(
        "[Batch Video] Batch complete: {} completed, {} failed",
        result.completed.len(),
        result.failed.len()
    );

    Ok(result)
}

/// Cancel an active section video generation.
///
/// Looks up the section_id in the shared cancellation token map and triggers cancellation.
/// Returns Ok(()) whether or not the section was actively generating.
#[tauri::command]
pub async fn cancel_section_generation(
    cancel_tokens: tauri::State<'_, SectionCancelTokens>,
    section_id: String,
) -> Result<(), AppError> {
    info!("[Cancel] Cancelling section generation: {}", section_id);

    let tokens = cancel_tokens
        .0
        .lock()
        .map_err(|e| AppError::FileSystem(format!("Failed to lock cancel tokens: {}", e)))?;

    if let Some(token) = tokens.get(&section_id) {
        token.cancel();
        info!(
            "[Cancel] Cancellation token triggered for section: {}",
            section_id
        );
    } else {
        info!(
            "[Cancel] No active generation found for section: {}",
            section_id
        );
    }

    Ok(())
}
