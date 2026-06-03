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

use super::ffmpeg_utils::{copy_video_file, merge_video_with_audio};

/// Node.js environment: npx path + the bin directory it lives in.
///
/// The bin directory must be added to PATH when spawning npx/node processes,
/// otherwise npx can't find `node` or globally installed packages like `hyperframes`.
pub struct NodeEnv {
    /// Absolute path to npx binary
    pub npx: String,
    /// Directory containing npx (and node, hyperframes, etc.)
    pub bin_dir: String,
}

/// Resolve the Node.js environment by probing well-known locations.
///
/// Tauri production builds do not inherit the user's shell PATH (e.g. nvm,
/// `/opt/homebrew/bin` are missing), so we must find npx/node explicitly.
pub fn find_node_env() -> NodeEnv {
    // 1. Well-known absolute paths (Homebrew, system Node, MacPorts)
    let candidates = [
        "/opt/homebrew/bin", // Homebrew on Apple Silicon (M1/M2/M3)
        "/usr/local/bin",    // Homebrew on Intel Mac / system Node
        "/opt/local/bin",    // MacPorts
        "/usr/bin",          // System / manual install
    ];
    for bin_dir in &candidates {
        let npx = format!("{}/npx", bin_dir);
        if std::path::Path::new(&npx).exists() {
            return NodeEnv {
                npx,
                bin_dir: bin_dir.to_string(),
            };
        }
    }

    // 2. Probe user-level version managers (nvm, volta, fnm)
    if let Some(home) = std::env::var_os("HOME") {
        let home = std::path::PathBuf::from(&home);

        // nvm: ~/.nvm/versions/node/<version>/bin/
        let nvm_dir = home.join(".nvm/versions/node");
        if let Some(bin_dir) = find_latest_bin_dir(&nvm_dir, "npx") {
            return NodeEnv {
                npx: format!("{}/npx", bin_dir),
                bin_dir,
            };
        }

        // volta: ~/.volta/bin/
        let volta_dir = home.join(".volta/bin");
        if volta_dir.join("npx").exists() {
            let bin_dir = volta_dir.to_string_lossy().to_string();
            return NodeEnv {
                npx: format!("{}/npx", bin_dir),
                bin_dir,
            };
        }

        // fnm: ~/.fnm/node-versions/<version>/installation/bin/
        let fnm_dir = home.join(".fnm/node-versions");
        if let Some(bin_dir) = find_latest_bin_dir(&fnm_dir, "npx") {
            return NodeEnv {
                npx: format!("{}/npx", bin_dir),
                bin_dir,
            };
        }

        // fnm (alt): ~/.local/share/fnm/node-versions/<version>/installation/bin/
        let fnm_alt = home.join(".local/share/fnm/node-versions");
        if let Some(bin_dir) = find_latest_bin_dir(&fnm_alt, "npx") {
            return NodeEnv {
                npx: format!("{}/npx", bin_dir),
                bin_dir,
            };
        }
    }

    // 3. Ask the login shell — inherits the user's full PATH
    if let Ok(output) = std::process::Command::new("/bin/sh")
        .args(["-lc", "which npx"])
        .output()
    {
        if output.status.success() {
            let npx_path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !npx_path.is_empty() {
                let bin_dir = std::path::Path::new(&npx_path)
                    .parent()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_default();
                return NodeEnv {
                    npx: npx_path,
                    bin_dir,
                };
            }
        }
    }

    // 4. Last resort — hope PATH is set correctly
    NodeEnv {
        npx: "npx".to_string(),
        bin_dir: String::new(),
    }
}

/// Look for `<bin>` inside subdirectories of `parent_dir`, picking the latest
/// (lexicographically greatest) version directory. Returns the bin directory path.
fn find_latest_bin_dir(parent_dir: &std::path::Path, bin: &str) -> Option<String> {
    let mut best: Option<std::path::PathBuf> = None;
    if let Ok(entries) = std::fs::read_dir(parent_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            // Try <version>/installation/bin/<bin> first (fnm), then <version>/bin/<bin> (nvm)
            let candidate = path.join("installation/bin");
            let candidate = if candidate.join(bin).exists() {
                candidate
            } else {
                let alt = path.join("bin");
                if alt.join(bin).exists() {
                    alt
                } else {
                    continue;
                }
            };
            let dominated = best
                .as_ref()
                .map(|b| b.as_os_str() < candidate.as_os_str())
                .unwrap_or(true);
            if dominated {
                best = Some(candidate);
            }
        }
    }
    best.map(|p| p.to_string_lossy().to_string())
}

/// Prepend a directory to the current PATH environment variable.
pub fn prepend_to_path(dir: &str) -> String {
    let current_path = std::env::var("PATH").unwrap_or_default();
    if current_path.is_empty() {
        dir.to_string()
    } else {
        format!("{}:{}", dir, current_path)
    }
}

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

    let node_env = find_node_env();
    info!(
        "[Hyperframes Render] Node env: npx={}, bin_dir={}",
        node_env.npx, node_env.bin_dir
    );

    let mut npx_check = Command::new(&node_env.npx);
    npx_check
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if !node_env.bin_dir.is_empty() {
        // Prepend node bin dir to PATH so npx can find node
        let path = prepend_to_path(&node_env.bin_dir);
        npx_check.env("PATH", &path);
    }
    let npx_check_status = npx_check.status().await;

    if npx_check_status.is_err() || !npx_check_status.unwrap().success() {
        return Err(AppError::FileSystem(
            "npx not found. Please install Node.js (https://nodejs.org)".to_string(),
        ));
    }

    // --- Step 2: Render HTML → video ---
    emit_progress(5.0, "正在渲染视频（可能需要 1-3 分钟）...");

    let silent_video = comp_dir.join("_render_output.mp4");
    let silent_video_str = silent_video.to_string_lossy().to_string();

    let mut render_cmd = Command::new(&node_env.npx);
    render_cmd
        .args(["hyperframes", "render", "--output", &silent_video_str])
        .current_dir(comp_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if !node_env.bin_dir.is_empty() {
        let path = prepend_to_path(&node_env.bin_dir);
        render_cmd.env("PATH", &path);
    }
    let render_result = render_cmd.spawn();

    let mut render_child = render_result
        .map_err(|e| AppError::FileSystem(format!("Failed to start hyperframes render: {}", e)))?;

    // Read stderr for progress and error capture
    let stderr_capture = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let stderr_capture_clone = stderr_capture.clone();

    if let Some(stderr) = render_child.stderr.take() {
        let app_clone = app.clone();
        tokio::spawn(async move {
            let reader = BufReader::new(stderr);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                // Capture all stderr for error reporting
                if let Ok(mut captured) = stderr_capture_clone.lock() {
                    captured.push_str(&line);
                    captured.push('\n');
                }
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
        let stderr_output = stderr_capture.lock().map(|s| s.clone()).unwrap_or_default();
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

    info!("[Hyperframes Render] Render complete: {:?}", silent_video);

    // --- Step 3: Merge audio (if provided) ---

    if let Some(ref audio) = audio_path {
        let audio_file = Path::new(audio);
        if !audio_file.exists() {
            // No audio file, just copy the silent video to output
            info!("[Hyperframes Render] Audio file not found, using silent video");
            copy_video_file(&silent_video_str, &output_path)?;
        } else {
            emit_progress(78.0, "正在合并音频...");

            // Use shared merge function
            merge_video_with_audio(&silent_video_str, audio, &output_path).await?;

            info!("[Hyperframes Render] Audio merged: {}", output_path);
        }
    } else {
        // No audio requested, just copy silent video to output
        copy_video_file(&silent_video_str, &output_path)?;
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
