//! Batch video generation and cancellation commands.
//!
//! Provides:
//! - `generate_all_sections`: Processes multiple sections concurrently in parallel.
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
use super::section_video::generate_section_video;

/// Shared state for tracking active section generation cancellation tokens.
/// Keyed by section_id.
pub struct SectionCancelTokens(pub Mutex<HashMap<String, CancellationToken>>);

impl Default for SectionCancelTokens {
    fn default() -> Self {
        Self(Mutex::new(HashMap::new()))
    }
}

/// Generate videos for all configured sections in parallel.
///
/// - Accepts a list of (section_id, SectionStyleConfig) pairs
/// - Processes all sections concurrently using tokio::spawn
/// - Returns BatchGenerationResult with completed/failed lists
/// - Emits progress events for each section
#[tauri::command]
pub async fn generate_all_sections(
    app: tauri::AppHandle,
    db: tauri::State<'_, Mutex<Database>>,
    cancel_tokens: tauri::State<'_, SectionCancelTokens>,
    project_id: String,
    section_configs: Vec<(String, SectionStyleConfig)>,
) -> Result<BatchGenerationResult, AppError> {
    info!(
        "[Batch Video] Starting parallel batch generation: project={}, configs={}",
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

    // Spawn all section generation tasks in parallel
    let mut tasks = Vec::new();

    for section in &sections {
        // Skip sections without config
        let style_config = match config_map.get(&section.id) {
            Some(config) => config.clone(),
            None => {
                info!(
                    "[Batch Video] Skipping unconfigured section: {}",
                    section.id
                );
                continue;
            }
        };

        // Register a cancellation token for this section
        let cancel_token = CancellationToken::new();
        {
            let mut tokens = cancel_tokens
                .0
                .lock()
                .map_err(|e| AppError::FileSystem(format!("Failed to lock cancel tokens: {}", e)))?;
            tokens.insert(section.id.clone(), cancel_token.clone());
        }

        // Emit batch progress event
        let _ = app.emit(
            "section-video-progress",
            SectionProgress {
                section_id: section.id.clone(),
                percent: 0.0,
                stage: "starting".to_string(),
            },
        );

        // Check if cancelled before starting
        if cancel_token.is_cancelled() {
            info!(
                "[Batch Video] Section {} cancelled before start",
                section.id
            );
            if let Ok(mut tokens) = cancel_tokens.0.lock() {
                tokens.remove(&section.id);
            }
            continue;
        }

        // Spawn async task for this section
        let app_clone = app.clone();
        let project_id_clone = project_id.clone();
        let section_id_clone = section.id.clone();

        let task = tokio::spawn(async move {
            let db_state: tauri::State<'_, Mutex<Database>> = app_clone.state();
            let result = generate_section_video(
                app_clone.clone(),
                db_state,
                project_id_clone,
                section_id_clone.clone(),
                style_config,
            ).await;

            (section_id_clone, result)
        });

        tasks.push(task);
    }

    // Wait for all tasks to complete
    let mut completed: Vec<String> = Vec::new();
    let mut failed: Vec<(String, String)> = Vec::new();

    for task in tasks {
        match task.await {
            Ok((section_id, result)) => {
                // Remove the cancellation token after completion
                if let Ok(mut tokens) = cancel_tokens.0.lock() {
                    tokens.remove(&section_id);
                }

                match result {
                    Ok(_video_result) => {
                        info!(
                            "[Batch Video] Section {} completed successfully",
                            section_id
                        );
                        completed.push(section_id);
                    }
                    Err(e) => {
                        let error_msg = e.to_string();
                        info!(
                            "[Batch Video] Section {} failed: {}",
                            section_id, error_msg
                        );
                        failed.push((section_id, error_msg));
                    }
                }
            }
            Err(e) => {
                let error_msg = format!("Task join error: {}", e);
                info!("[Batch Video] Task failed: {}", error_msg);
                failed.push(("unknown".to_string(), error_msg));
            }
        }
    }

    let result = BatchGenerationResult { completed, failed };

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
    info!(
        "[Cancel] Cancelling section generation: {}",
        section_id
    );

    let tokens = cancel_tokens
        .0
        .lock()
        .map_err(|e| AppError::FileSystem(format!("Failed to lock cancel tokens: {}", e)))?;

    if let Some(token) = tokens.get(&section_id) {
        token.cancel();
        info!("[Cancel] Cancellation token triggered for section: {}", section_id);
    } else {
        info!(
            "[Cancel] No active generation found for section: {}",
            section_id
        );
    }

    Ok(())
}
