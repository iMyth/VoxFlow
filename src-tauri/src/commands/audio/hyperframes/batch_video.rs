//! Batch video generation and cancellation commands.
//!
//! Provides:
//! - `generate_all_sections`: Processes multiple sections sequentially in section_order,
//!   skipping unconfigured sections, continuing on failure.
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

/// Generate videos for all configured sections sequentially in section_order.
///
/// - Accepts a list of (section_id, SectionStyleConfig) pairs
/// - Processes sections in section_order (determined by DB ordering)
/// - Skips sections without config in the provided list
/// - Continues on failure (doesn't stop the batch)
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
        "[Batch Video] Starting batch generation: project={}, configs={}",
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

    let mut completed: Vec<String> = Vec::new();
    let mut failed: Vec<(String, String)> = Vec::new();

    let total_configured = sections
        .iter()
        .filter(|s| config_map.contains_key(&s.id))
        .count();

    info!(
        "[Batch Video] Processing {} configured sections out of {} total",
        total_configured,
        sections.len()
    );

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
            failed.push((
                section.id.clone(),
                "Cancelled before start".to_string(),
            ));
            continue;
        }

        // Call the existing generate_section_video command logic.
        // We get the db State from the app handle to pass to the function.
        let db_state: tauri::State<'_, Mutex<Database>> = app.state();
        let result = generate_section_video(
            app.clone(),
            db_state,
            project_id.clone(),
            section.id.clone(),
            style_config,
        )
        .await;

        // Remove the cancellation token after completion
        if let Ok(mut tokens) = cancel_tokens.0.lock() {
            tokens.remove(&section.id);
        }

        match result {
            Ok(_video_result) => {
                info!(
                    "[Batch Video] Section {} completed successfully",
                    section.id
                );
                completed.push(section.id.clone());
            }
            Err(e) => {
                let error_msg = e.to_string();
                info!(
                    "[Batch Video] Section {} failed: {}",
                    section.id, error_msg
                );
                failed.push((section.id.clone(), error_msg));
                // Continue to next section — don't stop the batch
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
