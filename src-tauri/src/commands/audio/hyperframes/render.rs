//! Video rendering for Hyperframes compositions.
//!
//! Executes `npx hyperframes render` to convert HTML → video,
//! then optionally merges audio with ffmpeg.

use std::path::Path;
use std::process::Stdio;

use log::info;
use tauri::Emitter;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

use crate::core::error::AppError;

/// Progress event payload for the render pipeline.
#[derive(Debug, Clone, serde::Serialize)]
struct RenderProgress {
    percent: f32,
    stage: String,
}

/// Render a Hyperframes composition to a final video file.
///
/// Steps:
/// 1. Run `npx hyperframes render --output output.mp4` in the composition directory
/// 2. If audio exists, run `ffmpeg` to merge video + audio into the final output
/// 3. Emit progress events throughout
#[tauri::command]
pub async fn render_hyperframes_video(
    app: tauri::AppHandle,
    composition_dir: String,
    output_path: String,
    audio_path: Option<String>,
) -> Result<String, AppError> {
    info!(
        "[Hyperframes Render] Starting: dir={}, output={}, audio={:?}",
        composition_dir, output_path, audio_path
    );

    let comp_dir = Path::new(&composition_dir);
    if !comp_dir.join("index.html").exists() {
        return Err(AppError::FileSystem(
            "index.html not found in composition directory".to_string(),
        ));
    }

    let emit_progress = |percent: f32, stage: &str| {
        let _ = app.emit(
            "hyperframes-render-progress",
            RenderProgress {
                percent,
                stage: stage.to_string(),
            },
        );
    };

    // --- Step 1: Check for npx ---
    emit_progress(0.0, "检查环境...");

    let npx_check = Command::new("npx")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await;

    if npx_check.is_err() || !npx_check.unwrap().success() {
        return Err(AppError::FileSystem(
            "npx not found. Please install Node.js (https://nodejs.org)".to_string(),
        ));
    }

    // --- Step 2: Render HTML → video ---
    emit_progress(5.0, "正在渲染视频（可能需要 1-3 分钟）...");

    let silent_video = comp_dir.join("_render_output.mp4");
    let silent_video_str = silent_video.to_string_lossy().to_string();

    let render_result = Command::new("npx")
        .args(["hyperframes", "render", "--output", &silent_video_str])
        .current_dir(comp_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();

    let mut render_child = render_result
        .map_err(|e| AppError::FileSystem(format!("Failed to start hyperframes render: {}", e)))?;

    // Read stderr for progress (hyperframes render outputs progress to stderr)
    if let Some(stderr) = render_child.stderr.take() {
        let app_clone = app.clone();
        tokio::spawn(async move {
            let reader = BufReader::new(stderr);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                // Try to parse progress from hyperframes render output
                // Typical format: "Rendering frame 30/900 (3%)"
                if let Some(pct) = parse_render_progress(&line) {
                    let mapped = 5.0 + pct * 0.7; // Map to 5%-75% range
                    let _ = app_clone.emit(
                        "hyperframes-render-progress",
                        RenderProgress {
                            percent: mapped,
                            stage: format!("渲染中... {}%", (pct * 100.0) as u32),
                        },
                    );
                }
            }
        });
    }

    let render_status = render_child
        .wait()
        .await
        .map_err(|e| AppError::FileSystem(format!("hyperframes render process error: {}", e)))?;

    if !render_status.success() {
        // Try to read any remaining stderr output for error message
        return Err(AppError::FileSystem(format!(
            "hyperframes render failed with exit code: {:?}",
            render_status.code()
        )));
    }

    if !silent_video.exists() {
        return Err(AppError::FileSystem(
            "hyperframes render completed but output file not found".to_string(),
        ));
    }

    info!("[Hyperframes Render] Render complete: {:?}", silent_video);

    // --- Step 3: Merge audio (if provided) ---
    let final_output = Path::new(&output_path);

    if let Some(ref audio) = audio_path {
        let audio_file = Path::new(audio);
        if !audio_file.exists() {
            // No audio file, just move the silent video to output
            info!("[Hyperframes Render] Audio file not found, using silent video");
            std::fs::rename(&silent_video, final_output)
                .or_else(|_| std::fs::copy(&silent_video, final_output).map(|_| ()))
                .map_err(|e| AppError::FileSystem(format!("Failed to move output: {}", e)))?;
        } else {
            emit_progress(78.0, "正在合并音频...");

            // Check for ffmpeg
            let ffmpeg_check = Command::new("ffmpeg")
                .arg("-version")
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .await;

            if ffmpeg_check.is_err() || !ffmpeg_check.unwrap().success() {
                // ffmpeg not available, return silent video
                info!("[Hyperframes Render] ffmpeg not found, returning silent video");
                std::fs::rename(&silent_video, final_output)
                    .or_else(|_| std::fs::copy(&silent_video, final_output).map(|_| ()))
                    .map_err(|e| AppError::FileSystem(format!("Failed to move output: {}", e)))?;

                emit_progress(100.0, "完成（无音频，ffmpeg 未安装）");
                return Ok(output_path);
            }

            // Run ffmpeg to merge
            let ffmpeg_status = Command::new("ffmpeg")
                .args([
                    "-y",
                    "-i",
                    &silent_video_str,
                    "-i",
                    audio,
                    "-c:v",
                    "copy",
                    "-c:a",
                    "aac",
                    "-shortest",
                    &output_path,
                ])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .await
                .map_err(|e| AppError::FileSystem(format!("Failed to run ffmpeg: {}", e)))?;

            if !ffmpeg_status.success() {
                return Err(AppError::FileSystem(
                    "ffmpeg merge failed. The silent video is available at _render_output.mp4"
                        .to_string(),
                ));
            }

            // Clean up intermediate file
            let _ = std::fs::remove_file(&silent_video);

            info!("[Hyperframes Render] Audio merged: {}", output_path);
        }
    } else {
        // No audio requested, just move silent video to output
        std::fs::rename(&silent_video, final_output)
            .or_else(|_| std::fs::copy(&silent_video, final_output).map(|_| ()))
            .map_err(|e| AppError::FileSystem(format!("Failed to move output: {}", e)))?;
    }

    emit_progress(100.0, "渲染完成");
    info!("[Hyperframes Render] Complete: {}", output_path);
    Ok(output_path)
}

/// Try to parse a progress percentage from hyperframes render output.
/// Expected formats: "Rendering frame 30/900" or "3%" or "Progress: 45%"
pub fn parse_render_progress(line: &str) -> Option<f32> {
    // Try "frame X/Y" pattern
    if let Some(pos) = line.find('/') {
        let before_slash = &line[..pos];
        let after_slash = &line[pos + 1..];

        // Find the last number before /
        let current: f32 = before_slash
            .rsplit_once(|c: char| !c.is_ascii_digit())
            .map(|(_, n)| n)
            .unwrap_or(before_slash)
            .parse()
            .ok()?;

        // Find the first number after /
        let total: f32 = after_slash
            .split(|c: char| !c.is_ascii_digit())
            .next()?
            .parse()
            .ok()?;

        if total > 0.0 {
            return Some((current / total).min(1.0));
        }
    }

    // Try "N%" pattern
    if let Some(pos) = line.find('%') {
        let before_pct = &line[..pos];
        let num_str: String = before_pct
            .chars()
            .rev()
            .take_while(|c| c.is_ascii_digit() || *c == '.')
            .collect::<String>()
            .chars()
            .rev()
            .collect();
        if let Ok(pct) = num_str.parse::<f32>() {
            return Some((pct / 100.0).min(1.0));
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_render_progress_frame_format() {
        assert_eq!(
            parse_render_progress("Rendering frame 30/900"),
            Some(30.0 / 900.0)
        );
        assert_eq!(parse_render_progress("frame 450/900"), Some(0.5));
    }

    #[test]
    fn test_parse_render_progress_percent_format() {
        assert_eq!(parse_render_progress("Progress: 45%"), Some(0.45));
        assert_eq!(parse_render_progress("50%"), Some(0.5));
    }

    #[test]
    fn test_parse_render_progress_no_match() {
        assert_eq!(parse_render_progress("Some random log line"), None);
        assert_eq!(parse_render_progress(""), None);
    }
}
