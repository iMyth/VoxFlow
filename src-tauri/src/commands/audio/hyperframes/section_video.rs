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
use tokio_util::sync::CancellationToken;

use crate::core::config::ConfigManager;
use crate::core::db::Database;
use crate::core::error::AppError;

use super::agent::{generate_with_agent, AgentConfig};
use super::ffmpeg_utils::{copy_video_file, merge_video_with_audio};
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
///
/// `cancel_token` allows cancellation at key checkpoints (before LLM call, etc.)
/// to avoid wasting API tokens on cancelled sections.
pub async fn generate_section_html(
    app: tauri::AppHandle,
    db: tauri::State<'_, Mutex<Database>>,
    project_id: String,
    section_id: String,
    style_config: SectionStyleConfig,
    cancel_token: Option<CancellationToken>,
) -> Result<std::path::PathBuf, AppError> {
    let start_time = std::time::Instant::now();
    info!(
        "[Section HTML] ===== STARTING HTML GENERATION ===== section={}",
        section_id
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

    info!("[Section HTML] Output directory: {:?}", composition_dir);

    // Create directories
    info!("[Section HTML] Creating composition directory...");
    std::fs::create_dir_all(&composition_dir).map_err(|e| {
        AppError::FileSystem(format!("Failed to create composition directory: {}", e))
    })?;
    info!("[Section HTML] Creating assets directory...");
    std::fs::create_dir_all(composition_dir.join("assets"))
        .map_err(|e| AppError::FileSystem(format!("Failed to create assets directory: {}", e)))?;
    info!("[Section HTML] Directories created successfully");

    // --- Load data from DB ---
    info!("[Section HTML] Loading data from database...");
    let (script_lines, fragments) = {
        let db = db.lock().map_err(|e| AppError::Database(e.to_string()))?;
        let lines = db.load_script_lines(&project_id)?;
        let frags = db.list_audio_fragments(&project_id)?;
        (lines, frags)
    };
    info!(
        "[Section HTML] Loaded {} script lines and {} audio fragments",
        script_lines.len(),
        fragments.len()
    );

    // --- Compute section timeline ---
    info!("[Section HTML] Computing section timeline...");
    emit_section_progress(&app, &section_id, 5.0, "timeline_computation");

    let timeline_entries = compute_section_timeline(&section_id, &script_lines, &fragments);
    info!(
        "[Section HTML] Timeline computed: {} entries",
        timeline_entries.len()
    );

    if timeline_entries.is_empty() {
        return Err(AppError::FileSystem(
            "No timeline data available for this section (all lines missing audio)".to_string(),
        ));
    }

    // --- Merge section audio ---
    info!("[Section HTML] Merging section audio...");
    emit_section_progress(&app, &section_id, 15.0, "audio_merge");

    let audio_output_path = composition_dir.join("assets").join("audio.mp3");
    let audio_result = merge_section_audio(
        &section_id,
        &script_lines,
        &fragments,
        &audio_output_path,
        false,
    )
    .await?;

    info!(
        "[Section HTML] Audio merged: {}ms (actual), path={}",
        audio_result.total_duration_ms, audio_result.file_path
    );

    // Use the ACTUAL audio duration (from ffprobe) for HTML generation.
    // This ensures the video duration exactly matches the audio duration,
    // preventing audio-video desync caused by loudnorm/resampling drift.
    let actual_audio_duration_secs = audio_result.total_duration_ms as f64 / 1000.0;

    // Round UP to the next frame boundary so the rendered video is guaranteed to be
    // at least as long as the audio. Without this, floor(duration * fps) can produce
    // a video that is one frame too short, and the `-shortest` flag in the final
    // audio merge then truncates the last bit of audio (cutting off the final word).
    // Formula: ceil(duration * fps) / fps
    let fps = 30.0_f64;
    let render_duration_secs = (actual_audio_duration_secs * fps).ceil() / fps;

    // Check cancellation before expensive LLM call
    if let Some(ref token) = cancel_token {
        if token.is_cancelled() {
            info!(
                "[Section HTML] Section {} cancelled before LLM call, aborting",
                section_id
            );
            return Err(AppError::FileSystem(
                "Section generation cancelled".to_string(),
            ));
        }
    }

    // --- Generate HTML composition via agent ---
    info!("[Section HTML] Starting LLM agent generation...");
    emit_section_progress(&app, &section_id, 20.0, "html_generation");

    let (api_endpoint, model) = {
        let db = db.lock().map_err(|e| AppError::Database(e.to_string()))?;
        let settings = db.load_settings()?;
        (settings.llm_endpoint, settings.llm_model)
    };
    info!(
        "[Section HTML] Using LLM endpoint: {}, model: {}",
        api_endpoint, model
    );

    let config_manager = ConfigManager::new(app.clone());
    let api_key = config_manager
        .load_api_key("llm")
        .map_err(|e| AppError::Config(format!("Failed to load API key: {}", e)))?
        .ok_or_else(|| AppError::Config("LLM API key not configured".to_string()))?;
    info!("[Section HTML] API key loaded successfully");

    let agent_config = AgentConfig {
        api_endpoint,
        api_key,
        model,
    };

    let app_clone = app.clone();
    let section_id_clone = section_id.clone();
    let progress_counter = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
    let llm_start = std::time::Instant::now();
    let on_progress: Box<dyn Fn(&str) + Send + Sync> = Box::new(move |stage: &str| {
        let count = progress_counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let elapsed = llm_start.elapsed().as_secs();
        info!(
            "[Section HTML] LLM progress callback: stage='{}', count={}, elapsed={}s",
            stage, count, elapsed
        );
        let internal_percent = (1.0 - (-0.15 * count as f64).exp()) * 100.0;
        let mapped_percent = 20.0 + (internal_percent / 100.0) * 30.0;
        emit_section_progress(
            &app_clone,
            &section_id_clone,
            mapped_percent.min(48.0) as f32,
            stage,
        );
    });

    info!("[Section HTML] Calling generate_with_agent...");
    let html = generate_with_agent(
        &timeline_entries,
        &agent_config,
        Some(on_progress),
        style_config.user_prompt.as_deref(),
        Some(render_duration_secs),
    )
    .await
    .map_err(AppError::LlmService)?;

    info!(
        "[Section HTML] LLM generation completed in {:.2}s, HTML size: {} bytes",
        llm_start.elapsed().as_secs_f32(),
        html.len()
    );

    emit_section_progress(&app, &section_id, 50.0, "html_generation");

    // --- Write HTML composition files ---
    info!("[Section HTML] Writing index.html...");
    std::fs::write(composition_dir.join("index.html"), &html)
        .map_err(|e| AppError::FileSystem(format!("Failed to write index.html: {}", e)))?;

    // Use rounded-up duration for meta.json — this is what hyperframes render uses
    // to determine total frame count. Must be >= audio length so the video isn't short.
    let total_duration = render_duration_secs;
    let meta_json = serde_json::json!({
        "id": &section_id,
        "title": format!("Section {}", section_id),
        "width": 1920,
        "height": 1080,
        "fps": 30,
        "duration": total_duration
    });
    let meta_str = serde_json::to_string_pretty(&meta_json).unwrap_or_default();
    info!("[Section HTML] Writing meta.json...");
    std::fs::write(composition_dir.join("meta.json"), meta_str)
        .map_err(|e| AppError::FileSystem(format!("Failed to write meta.json: {}", e)))?;

    info!(
        "[Section HTML] ===== HTML GENERATION COMPLETE ===== section={}, elapsed={:.2}s",
        section_id,
        start_time.elapsed().as_secs_f32()
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
    let start_time = std::time::Instant::now();
    info!(
        "[Section Render] ===== STARTING VIDEO RENDERING ===== section={}",
        section_id
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

    info!(
        "[Section Render] Composition directory: {:?}",
        composition_dir
    );
    info!("[Section Render] Audio path: {:?}", audio_output_path);
    info!(
        "[Section Render] Output video path: {:?}",
        output_video_path
    );

    // Check if composition files exist
    let index_html_path = composition_dir.join("index.html");
    let meta_json_path = composition_dir.join("meta.json");
    info!(
        "[Section Render] Checking if index.html exists: {}",
        index_html_path.exists()
    );
    info!(
        "[Section Render] Checking if meta.json exists: {}",
        meta_json_path.exists()
    );

    if !index_html_path.exists() {
        return Err(AppError::FileSystem(
            "index.html not found in composition directory".to_string(),
        ));
    }

    // --- Render HTML → MP4 ---
    // On macOS, the first render attempt may fail because headless Chrome requires
    // "App Management" permission. macOS shows a permission dialog, Chrome exits,
    // user grants permission, and the retry succeeds. We handle this transparently
    // with automatic retry after a short delay.
    emit_section_progress(&app, &section_id, 55.0, "rendering");

    let silent_video = composition_dir.join("_render_output.mp4");
    let silent_video_str = silent_video.to_string_lossy().to_string();

    // Calculate dynamic timeout based on video duration from meta.json.
    // Rule of thumb: ~5 seconds per second of video at 30fps rendering.
    // Minimum 5 minutes, maximum 30 minutes.
    // Long compositions (>60s) need more time per frame due to DOM complexity.
    let render_timeout_secs = {
        let base_timeout: u64 = 300; // 5 minutes minimum
        let max_timeout: u64 = 1800; // 30 minutes maximum
        if meta_json_path.exists() {
            std::fs::read_to_string(&meta_json_path)
                .ok()
                .and_then(|content| serde_json::from_str::<serde_json::Value>(&content).ok())
                .and_then(|meta| meta.get("duration").and_then(|d| d.as_f64()))
                .map(|duration_secs| {
                    // ~5 seconds of render time per second of video
                    // Longer videos are proportionally slower due to larger DOM
                    let factor = if duration_secs > 60.0 { 6.0 } else { 5.0 };
                    let estimated = (duration_secs * factor) as u64;
                    estimated.max(base_timeout).min(max_timeout)
                })
                .unwrap_or(base_timeout)
        } else {
            base_timeout
        }
    };
    info!(
        "[Section Render] Render timeout: {}s ({}min)",
        render_timeout_secs,
        render_timeout_secs / 60
    );

    let node_env = super::render::find_node_env();
    info!(
        "[Section Render] Node env: npx={}, bin_dir={}",
        node_env.npx, node_env.bin_dir
    );

    // Check if output file already exists
    if silent_video.exists() {
        info!("[Section Render] Silent video already exists, removing...");
        std::fs::remove_file(&silent_video).map_err(|e| {
            AppError::FileSystem(format!("Failed to remove existing silent video: {}", e))
        })?;
    }

    // Attempt render with retry (max 2 attempts for macOS permission flow)
    let max_attempts = 2;
    let mut last_error: Option<String> = None;

    for attempt in 1..=max_attempts {
        if attempt > 1 {
            info!(
                "[Section Render] Retry attempt {} after permission grant delay...",
                attempt
            );
            // Wait for macOS to register the permission grant
            tokio::time::sleep(Duration::from_secs(3)).await;

            // Clean up any partial output from failed attempt
            if silent_video.exists() {
                let _ = std::fs::remove_file(&silent_video);
            }

            emit_section_progress(&app, &section_id, 55.0, "rendering_retry");
        }

        info!("[Section Render] Starting render attempt {}...", attempt);

        let mut render_cmd = Command::new(&node_env.npx);
        // Use --prefer-offline to avoid downloading a non-existent "latest" version
        // when a cached version already works. Also use version range "hyperframes@0.6"
        // to get the latest compatible cached version.
        render_cmd
            .args([
                "--prefer-offline",
                "hyperframes",
                "render",
                "--output",
                &silent_video_str,
            ])
            .current_dir(&composition_dir)
            // stdout must be null (not piped-without-reader): Chrome writes progress
            // to stdout. If we pipe it but never drain, the OS buffer (64 KB) fills up
            // and the render process deadlocks on write.
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        if !node_env.bin_dir.is_empty() {
            let path = super::render::prepend_to_path(&node_env.bin_dir);
            render_cmd.env("PATH", &path);
        }
        // Create a new process group so we can kill the entire tree on timeout.
        // Without this, killing npx leaves Chrome child processes as zombies.
        #[cfg(unix)]
        unsafe {
            render_cmd.pre_exec(|| {
                // Set this process as the leader of a new process group
                libc::setpgid(0, 0);
                Ok(())
            });
        }

        let mut render_child = match render_cmd.spawn() {
            Ok(child) => {
                info!("[Section Render] Process spawned, PID: {:?}", child.id());
                child
            }
            Err(e) => {
                last_error = Some(format!("Failed to start hyperframes render: {}", e));
                continue;
            }
        };

        // Read stderr for progress and error capture
        let stderr_capture = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
        let stderr_capture_clone = stderr_capture.clone();

        if let Some(stderr) = render_child.stderr.take() {
            let app_clone = app.clone();
            let section_id_clone = section_id.clone();
            tokio::spawn(async move {
                let reader = BufReader::new(stderr);
                let mut lines = reader.lines();
                let mut line_count = 0;
                while let Ok(Some(line)) = lines.next_line().await {
                    line_count += 1;
                    if let Ok(mut captured) = stderr_capture_clone.lock() {
                        captured.push_str(&line);
                        captured.push('\n');
                    }
                    if line_count % 10 == 0 {
                        info!("[Section Render] stderr line {}: {}", line_count, line);
                    }
                    if let Some(pct) = parse_render_progress(&line) {
                        let mapped = 55.0 + pct * 33.0;
                        emit_section_progress(&app_clone, &section_id_clone, mapped, "rendering");
                    }
                }
            });
        }

        // Wait for render process with dynamic timeout
        let render_result = timeout(
            Duration::from_secs(render_timeout_secs),
            render_child.wait(),
        )
        .await;

        match render_result {
            Ok(Ok(status)) if status.success() => {
                // Render succeeded
                if silent_video.exists() {
                    info!("[Section Render] Render succeeded on attempt {}", attempt);
                    last_error = None;
                    break;
                } else {
                    last_error =
                        Some("hyperframes render completed but output file not found".to_string());
                }
            }
            Ok(Ok(status)) => {
                // Render failed with non-zero exit
                let stderr_output = stderr_capture.lock().map(|s| s.clone()).unwrap_or_default();
                let err_msg = format!(
                    "hyperframes render failed (exit {:?}): {}",
                    status.code(),
                    stderr_output.trim()
                );
                info!("[Section Render] Attempt {} failed: {}", attempt, err_msg);
                last_error = Some(err_msg);

                // Only retry on macOS permission issues — all other errors should fail immediately
                if attempt < max_attempts {
                    let stderr_lower = stderr_output.to_lowercase();
                    let is_permission_issue = stderr_lower.contains("permission")
                        || stderr_lower.contains("not permitted")
                        || stderr_lower.contains("app management")
                        || stderr_output.contains("EPERM")
                        || (stderr_output.contains("Capture failed")
                            && stderr_output.contains("zero duration"));
                    if is_permission_issue {
                        info!("[Section Render] Detected possible permission issue, will retry");
                        // Emit user-friendly message to frontend
                        emit_section_progress(&app, &section_id, 55.0, "permission_retry");
                        let _ = app.emit(
                            "section-video-permission-hint",
                            serde_json::json!({
                                "section_id": &section_id,
                                "message": "macOS 需要授予「App 管理」权限才能渲染视频。如果弹出了权限请求，请点击允许，系统将自动重试。",
                            }),
                        );
                        continue;
                    }
                }
                // Not a retriable issue — don't retry
                break;
            }
            Ok(Err(e)) => {
                last_error = Some(format!("hyperframes render process error: {}", e));
            }
            Err(_) => {
                let timeout_mins = render_timeout_secs / 60;
                info!(
                    "[Section Render] Render timed out after {} minutes, killing process tree...",
                    timeout_mins
                );

                // Kill the entire process tree, not just the parent npx process.
                // On macOS/Unix, kill the process group to ensure Chrome child processes
                // are also terminated. Otherwise they become zombies consuming 100% CPU.
                if let Some(_pid) = render_child.id() {
                    #[cfg(unix)]
                    {
                        // Kill the entire process group (negative PID = kill group)
                        // Safe because we set this process as its own group leader via setpgid
                        unsafe {
                            libc::kill(-(_pid as i32), libc::SIGKILL);
                        }
                        info!("[Section Render] Sent SIGKILL to process group {}", _pid);
                    }
                }
                // Also try the standard kill as fallback
                let _ = render_child.kill().await;

                last_error = Some(format!(
                    "视频渲染超时（{}分钟）。可能是视频过长，请尝试缩短段落或重试。",
                    timeout_mins
                ));
                // Don't retry on timeout — it's a real resource issue
                break;
            }
        }
    }

    // Check final result
    if let Some(err) = last_error {
        return Err(AppError::FileSystem(err));
    }
    if !silent_video.exists() {
        return Err(AppError::FileSystem(
            "hyperframes render completed but output file not found".to_string(),
        ));
    }

    // Validate render output is not corrupted (0 bytes or suspiciously small)
    let render_size = std::fs::metadata(&silent_video)
        .map(|m| m.len())
        .unwrap_or(0);
    if render_size < 10_000 {
        // Less than 10KB is almost certainly a corrupted/empty render
        let _ = std::fs::remove_file(&silent_video);
        return Err(AppError::FileSystem(format!(
            "视频渲染产出文件异常（仅 {} 字节），可能是渲染器崩溃。请重试。",
            render_size
        )));
    }

    let metadata = std::fs::metadata(&silent_video)
        .map_err(|e| AppError::FileSystem(format!("Failed to get silent video metadata: {}", e)))?;
    info!(
        "[Section Render] Silent video created: {:?}, size: {} bytes",
        silent_video,
        metadata.len()
    );

    // --- Merge audio into video ---
    info!("[Section Render] Starting audio merge phase...");
    emit_section_progress(&app, &section_id, 90.0, "rendering");

    let output_path_str = output_video_path.to_string_lossy().to_string();

    info!(
        "[Section Render] Checking if audio file exists: {}",
        audio_output_path.exists()
    );
    if audio_output_path.exists() {
        // Use shared merge function
        merge_video_with_audio(
            &silent_video_str,
            &audio_output_path.to_string_lossy(),
            &output_path_str,
        )
        .await?;
    } else {
        info!("[Section Render] No audio file, copying silent video to output");
        copy_video_file(&silent_video_str, &output_path_str)?;
    }

    // --- Done ---
    info!("[Section Render] Checking final output file...");
    let file_size_bytes = std::fs::metadata(&output_video_path)
        .map(|m| {
            info!("[Section Render] Final video size: {} bytes", m.len());
            m.len()
        })
        .unwrap_or(0);

    // Read duration from meta.json
    let duration_ms = if meta_json_path.exists() {
        match std::fs::read_to_string(&meta_json_path) {
            Ok(content) => match serde_json::from_str::<serde_json::Value>(&content) {
                Ok(meta) => {
                    if let Some(duration_secs) = meta.get("duration").and_then(|d| d.as_f64()) {
                        let ms = (duration_secs * 1000.0) as i64;
                        info!("[Section Render] Duration from meta.json: {}ms", ms);
                        ms
                    } else {
                        info!("[Section Render] No duration field in meta.json");
                        0
                    }
                }
                Err(e) => {
                    info!("[Section Render] Failed to parse meta.json: {}", e);
                    0
                }
            },
            Err(e) => {
                info!("[Section Render] Failed to read meta.json: {}", e);
                0
            }
        }
    } else {
        info!("[Section Render] meta.json not found");
        0
    };

    let result = SectionVideoResult {
        section_id: section_id.clone(),
        video_path: output_path_str,
        duration_ms,
        file_size_bytes,
    };

    info!(
        "[Section Render] ===== VIDEO RENDERING COMPLETE ===== section={}, duration={}ms, size={}bytes, elapsed={:.2}s",
        section_id, result.duration_ms, result.file_size_bytes,
        start_time.elapsed().as_secs_f32()
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
        None, // No cancellation for single-section generation
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
