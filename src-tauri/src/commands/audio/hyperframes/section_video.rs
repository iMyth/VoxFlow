//! Section-level video generation command.
//!
//! Generates a complete video for a single ScriptSection by:
//! 1. Loading section data from DB
//! 2. Computing section timeline
//! 3. Merging section audio
//! 4. Generating HTML composition via agent
//! 5. Rendering HTML to MP4 via `npx hyperframes render`
//! 6. Merging audio into the final video

use std::process::Stdio;
use std::sync::Mutex;
use std::time::Duration;

use log::info;
use tauri::{Emitter, Manager};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::time::timeout;

use crate::core::config::ConfigManager;
use crate::core::db::Database;
use crate::core::error::AppError;

use super::agent::{generate_with_agent, AgentConfig};
use super::render::parse_render_progress;
use super::section_audio::merge_section_audio;
use super::section_types::{SectionProgress, SectionStyleConfig, SectionVideoResult};
use super::timeline::compute_section_timeline;

/// Emit a section-video-progress event.
fn emit_section_progress(app: &tauri::AppHandle, section_id: &str, percent: f32, stage: &str) {
    let _ = app.emit(
        "section-video-progress",
        SectionProgress {
            section_id: section_id.to_string(),
            percent,
            stage: stage.to_string(),
        },
    );
}

/// Generate HTML composition for a section (Phase 1 - can run in parallel).
///
/// This function handles:
/// - Loading section data from DB
/// - Computing timeline
/// - Merging section audio
/// - Calling LLM agent to generate HTML
/// - Writing HTML and meta.json files
///
/// Returns the path to the composition directory.
pub async fn generate_section_html(
    app: tauri::AppHandle,
    db: tauri::State<'_, Mutex<Database>>,
    project_id: String,
    section_id: String,
    style_config: SectionStyleConfig,
) -> Result<std::path::PathBuf, AppError> {
    info!(
        "[Section HTML] Starting generation: project={}, section={}",
        project_id, section_id
    );

    // --- Determine output directory ---
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::FileSystem(format!("Failed to resolve app data dir: {}", e)))?;
    let export_dir = app_data_dir
        .join("projects")
        .join(&project_id)
        .join("export")
        .join("sections")
        .join(&section_id);
    let composition_dir = export_dir.join("composition");

    // Create directories
    std::fs::create_dir_all(&composition_dir).map_err(|e| {
        AppError::FileSystem(format!("Failed to create composition directory: {}", e))
    })?;
    std::fs::create_dir_all(composition_dir.join("assets")).map_err(|e| {
        AppError::FileSystem(format!("Failed to create assets directory: {}", e))
    })?;

    // --- Load data from DB ---
    let (script_lines, fragments) = {
        let db = db.lock().map_err(|e| AppError::Database(e.to_string()))?;
        let lines = db.load_script_lines(&project_id)?;
        let frags = db.list_audio_fragments(&project_id)?;
        (lines, frags)
    };

    // --- Compute section timeline ---
    emit_section_progress(&app, &section_id, 5.0, "timeline_computation");

    let timeline_entries = compute_section_timeline(&section_id, &script_lines, &fragments);

    if timeline_entries.is_empty() {
        return Err(AppError::FileSystem(
            "No timeline data available for this section (all lines missing audio)".to_string(),
        ));
    }

    info!(
        "[Section HTML] Timeline computed: {} entries for section {}",
        timeline_entries.len(),
        section_id
    );

    // --- Merge section audio ---
    emit_section_progress(&app, &section_id, 15.0, "audio_merge");

    let audio_output_path = composition_dir.join("assets").join("audio.mp3");
    let audio_result =
        merge_section_audio(&section_id, &script_lines, &fragments, &audio_output_path).await?;

    info!(
        "[Section HTML] Audio merged: {}ms, path={}",
        audio_result.total_duration_ms, audio_result.file_path
    );

    // --- Generate HTML composition via agent ---
    emit_section_progress(&app, &section_id, 20.0, "html_generation");

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
    let section_id_clone = section_id.clone();
    let progress_counter = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
    let on_progress: Box<dyn Fn(&str) + Send + Sync> = Box::new(move |stage: &str| {
        let count = progress_counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let internal_percent = (1.0 - (-0.15 * count as f64).exp()) * 100.0;
        let mapped_percent = 20.0 + (internal_percent / 100.0) * 30.0;
        emit_section_progress(
            &app_clone,
            &section_id_clone,
            mapped_percent.min(48.0) as f32,
            stage,
        );
    });

    let html = generate_with_agent(
        &timeline_entries,
        &agent_config,
        Some(on_progress),
        style_config.user_prompt.as_deref(),
    )
    .await
    .map_err(AppError::LlmService)?;

    emit_section_progress(&app, &section_id, 50.0, "html_generation");

    // --- Write HTML composition files ---
    std::fs::write(composition_dir.join("index.html"), &html)
        .map_err(|e| AppError::FileSystem(format!("Failed to write index.html: {}", e)))?;

    let total_duration = timeline_entries
        .iter()
        .map(|e| e.start_time + e.duration)
        .fold(0.0_f64, f64::max);
    let meta_json = serde_json::json!({
        "id": &section_id,
        "title": format!("Section {}", section_id),
        "width": 1920,
        "height": 1080,
        "fps": 30,
        "duration": total_duration
    });
    let meta_str = serde_json::to_string_pretty(&meta_json).unwrap_or_default();
    std::fs::write(composition_dir.join("meta.json"), meta_str)
        .map_err(|e| AppError::FileSystem(format!("Failed to write meta.json: {}", e)))?;

    info!(
        "[Section HTML] HTML generation complete: section={}, dir={:?}",
        section_id, composition_dir
    );

    Ok(composition_dir)
}

/// Render HTML composition to video (Phase 2 - must run sequentially).
///
/// This function handles:
/// - Rendering HTML to silent MP4 via npx hyperframes render
/// - Merging audio into the final video
///
/// Should be called after generate_section_html() has completed.
pub async fn render_section_video(
    app: tauri::AppHandle,
    project_id: String,
    section_id: String,
) -> Result<SectionVideoResult, AppError> {
    info!(
        "[Section Render] Starting render: project={}, section={}",
        project_id, section_id
    );

    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::FileSystem(format!("Failed to resolve app data dir: {}", e)))?;
    let export_dir = app_data_dir
        .join("projects")
        .join(&project_id)
        .join("export")
        .join("sections")
        .join(&section_id);
    let composition_dir = export_dir.join("composition");
    let audio_output_path = composition_dir.join("assets").join("audio.mp3");
    let output_video_path = export_dir.join("output.mp4");

    // --- Render HTML → MP4 ---
    emit_section_progress(&app, &section_id, 55.0, "rendering");

    let silent_video = composition_dir.join("_render_output.mp4");
    let silent_video_str = silent_video.to_string_lossy().to_string();

    let node_env = super::render::find_node_env();
    info!(
        "[Section Render] Node env: npx={}, bin_dir={}",
        node_env.npx, node_env.bin_dir
    );

    // Create render task with timeout (5 minutes max)
    let render_result = timeout(Duration::from_secs(300), async {
        let mut render_cmd = Command::new(&node_env.npx);
        render_cmd
            .args(["hyperframes", "render", "--output", &silent_video_str])
            .current_dir(&composition_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if !node_env.bin_dir.is_empty() {
            let path = super::render::prepend_to_path(&node_env.bin_dir);
            render_cmd.env("PATH", &path);
        }
        let render_result = render_cmd.spawn();

        let mut render_child = render_result.map_err(|e| {
            AppError::FileSystem(format!("Failed to start hyperframes render: {}", e))
        })?;

        // Read stderr for progress and error capture
        let stderr_capture = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
        let stderr_capture_clone = stderr_capture.clone();

        if let Some(stderr) = render_child.stderr.take() {
            let app_clone = app.clone();
            let section_id_clone = section_id.clone();
            tokio::spawn(async move {
                let reader = BufReader::new(stderr);
                let mut lines = reader.lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    if let Ok(mut captured) = stderr_capture_clone.lock() {
                        captured.push_str(&line);
                        captured.push('\n');
                    }
                    if let Some(pct) = parse_render_progress(&line) {
                        let mapped = 55.0 + pct * 33.0;
                        emit_section_progress(
                            &app_clone,
                            &section_id_clone,
                            mapped as f32,
                            "rendering",
                        );
                    }
                }
            });
        }

        let render_status = render_child.wait().await.map_err(|e| {
            AppError::FileSystem(format!("hyperframes render process error: {}", e))
        })?;

        if !render_status.success() {
            let stderr_output = stderr_capture
                .lock()
                .map(|s| s.clone())
                .unwrap_or_default();
            return Err(AppError::FileSystem(format!(
                "hyperframes render failed with exit code: {:?}\nstderr: {}",
                render_status.code(),
                stderr_output.trim()
            )));
        }

        if !silent_video.exists() {
            return Err(AppError::FileSystem(
                "hyperframes render completed but output file not found".to_string(),
            ));
        }

        info!("[Section Render] Render complete: {:?}", silent_video);
        Ok(())
    })
    .await;

    // Handle timeout
    match render_result {
        Ok(Ok(())) => {}
        Ok(Err(e)) => return Err(e),
        Err(_) => {
            return Err(AppError::FileSystem(
                "Video rendering timed out (5 minutes). Please try again or reduce the video length."
                    .to_string(),
            ));
        }
    }

    // --- Merge audio into video ---
    emit_section_progress(&app, &section_id, 90.0, "rendering");

    let output_path_str = output_video_path.to_string_lossy().to_string();

    if audio_output_path.exists() {
        let audio_path_str = audio_output_path.to_string_lossy().to_string();

        let ffmpeg_status = Command::new("ffmpeg")
            .args([
                "-y",
                "-i",
                &silent_video_str,
                "-i",
                &audio_path_str,
                "-c:v",
                "copy",
                "-c:a",
                "aac",
                "-shortest",
                &output_path_str,
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await
            .map_err(|e| AppError::FileSystem(format!("Failed to run ffmpeg: {}", e)))?;

        if !ffmpeg_status.success() {
            info!("[Section Render] ffmpeg merge failed, using silent video");
            std::fs::rename(&silent_video, &output_video_path)
                .or_else(|_| std::fs::copy(&silent_video, &output_video_path).map(|_| ()))
                .map_err(|e| AppError::FileSystem(format!("Failed to move output: {}", e)))?;
        } else {
            let _ = std::fs::remove_file(&silent_video);
        }
    } else {
        std::fs::rename(&silent_video, &output_video_path)
            .or_else(|_| std::fs::copy(&silent_video, &output_video_path).map(|_| ()))
            .map_err(|e| AppError::FileSystem(format!("Failed to move output: {}", e)))?;
    }

    // --- Done ---
    emit_section_progress(&app, &section_id, 100.0, "done");

    let file_size_bytes = std::fs::metadata(&output_video_path)
        .map(|m| m.len())
        .unwrap_or(0);

    // Get duration from meta.json or audio file
    let duration_ms = 0i64; // Will be calculated from meta.json if needed

    let result = SectionVideoResult {
        section_id: section_id.clone(),
        video_path: output_path_str,
        duration_ms,
        file_size_bytes,
    };

    info!(
        "[Section Render] Generation complete: section={}, duration={}ms, size={}bytes",
        section_id, result.duration_ms, result.file_size_bytes
    );

    Ok(result)
}

/// Generate video for a single ScriptSection (convenience wrapper).
///
/// Combines generate_section_html() and render_section_video() for single-section generation.
#[tauri::command]
pub async fn generate_section_video(
    app: tauri::AppHandle,
    db: tauri::State<'_, Mutex<Database>>,
    project_id: String,
    section_id: String,
    style_config: SectionStyleConfig,
) -> Result<SectionVideoResult, AppError> {
    info!(
        "[Section Video] Starting generation: project={}, section={}, mode={:?}",
        project_id, section_id, style_config.mode
    );

    // Phase 1: Generate HTML
    generate_section_html(
        app.clone(),
        db,
        project_id.clone(),
        section_id.clone(),
        style_config,
    )
    .await?;

    // Phase 2: Render video
    render_section_video(app, project_id, section_id).await
}

/// Check if a section video file exists.
#[tauri::command]
pub fn check_section_video_exists(
    app: tauri::AppHandle,
    project_id: String,
    section_id: String,
) -> Result<bool, AppError> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::FileSystem(format!("Failed to resolve app data dir: {}", e)))?;

    let output_video_path = app_data_dir
        .join("projects")
        .join(&project_id)
        .join("export")
        .join("sections")
        .join(&section_id)
        .join("output.mp4");

    Ok(output_video_path.exists())
}
