//! Video export — generates a visualization video from audio using FFmpeg filters.

use log::info;
use tauri::Emitter;

use crate::core::error::AppError;
use crate::core::models::MixProgress;

use super::ffmpeg::find_ffmpeg;

/// Supported visualization styles for video export.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VideoStyle {
    /// Waveform line visualization
    Showwaves,
    /// Frequency spectrum bars
    Showfreqs,
    /// Lissajous vector scope
    Avectorscope,
    /// Spectrogram waterfall
    Showspectrum,
    /// Particle kaleidoscope (audio-driven particles with symmetry)
    Particles,
}

/// Configuration for video export.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct VideoExportConfig {
    /// Audio source file path (the mixed audio mp3)
    pub audio_path: String,
    /// Output video file path
    pub output_path: String,
    /// Visualization style
    pub style: VideoStyle,
    /// Video resolution width (default 1920)
    pub width: Option<u32>,
    /// Video resolution height (default 1080)
    pub height: Option<u32>,
    /// Foreground/waveform color (hex without #, e.g. "6366f1")
    pub fg_color: Option<String>,
    /// Background color (hex without #, e.g. "1a1a2e")
    pub bg_color: Option<String>,
    /// Optional background image path (overlays waveform on image)
    pub bg_image_path: Option<String>,
    /// Frame rate (default 30)
    pub fps: Option<u32>,
    /// Kaleidoscope symmetry folds for particle mode (default 8)
    pub symmetry_folds: Option<u32>,
}

/// Build FFmpeg arguments for video generation based on the visualization style.
fn build_video_ffmpeg_args(config: &VideoExportConfig) -> Vec<String> {
    let width = config.width.unwrap_or(1920);
    let height = config.height.unwrap_or(1080);
    let fps = config.fps.unwrap_or(30);
    let fg_color = config.fg_color.as_deref().unwrap_or("6366f1");
    let bg_color = config.bg_color.as_deref().unwrap_or("1a1a2e");

    let mut args = Vec::new();
    args.push("-y".to_string());

    // Input: audio file
    args.push("-i".to_string());
    args.push(config.audio_path.clone());

    // Optional: background image input
    if let Some(ref bg_img) = config.bg_image_path {
        args.push("-loop".to_string());
        args.push("1".to_string());
        args.push("-i".to_string());
        args.push(bg_img.clone());
    }

    // Build filter_complex based on style
    let filter = if config.bg_image_path.is_some() {
        // With background image: overlay waveform on bottom portion
        let wave_height = height / 5; // 20% of height for waveform
        let wave_filter = build_wave_filter(&config.style, width, wave_height, fps, fg_color);
        format!(
            "[0:a]{wave_filter}[waves];[1:v]scale={w}:{h},format=yuv420p[bg];[bg][waves]overlay=0:{y}:shortest=1[v]",
            wave_filter = wave_filter,
            w = width,
            h = height,
            y = height - wave_height,
        )
    } else {
        // No background image: full-frame visualization with solid background
        let vis_filter = build_full_filter(&config.style, width, height, fps, fg_color, bg_color);
        vis_filter
    };

    args.push("-filter_complex".to_string());
    args.push(filter);

    args.push("-map".to_string());
    args.push("[v]".to_string());
    args.push("-map".to_string());
    args.push("0:a".to_string());

    // Video codec settings
    args.push("-c:v".to_string());
    args.push("libx264".to_string());
    args.push("-preset".to_string());
    args.push("medium".to_string());
    args.push("-crf".to_string());
    args.push("23".to_string());
    args.push("-pix_fmt".to_string());
    args.push("yuv420p".to_string());

    // Audio codec
    args.push("-c:a".to_string());
    args.push("aac".to_string());
    args.push("-b:a".to_string());
    args.push("192k".to_string());

    // Shortest flag to stop when audio ends
    args.push("-shortest".to_string());

    args.push(config.output_path.clone());
    args
}

/// Build the visualization filter for full-frame mode (no background image).
fn build_full_filter(
    style: &VideoStyle,
    width: u32,
    height: u32,
    fps: u32,
    fg_color: &str,
    bg_color: &str,
) -> String {
    match style {
        VideoStyle::Showwaves => {
            format!(
                "[0:a]showwaves=s={w}x{h}:mode=cline:rate={fps}:colors=0x{fg}:scale=sqrt[raw];\
                 color=c=0x{bg}:s={w}x{h}:r={fps}[bg];\
                 [bg][raw]overlay=shortest=1[v]",
                w = width, h = height, fps = fps, fg = fg_color, bg = bg_color,
            )
        }
        VideoStyle::Showfreqs => {
            format!(
                "[0:a]showfreqs=s={w}x{h}:mode=bar:fscale=log:colors=0x{fg}|0x{fg}88:win_size=2048[raw];\
                 color=c=0x{bg}:s={w}x{h}:r={fps}[bg];\
                 [bg][raw]overlay=shortest=1[v]",
                w = width, h = height, fps = fps, fg = fg_color, bg = bg_color,
            )
        }
        VideoStyle::Avectorscope => {
            format!(
                "[0:a]avectorscope=s={w}x{h}:mode=lissajous:rate={fps}:rc=0x{r}:gc=0x{g}:bc=0x{b}[raw];\
                 color=c=0x{bg}:s={w}x{h}:r={fps}[bg];\
                 [bg][raw]overlay=shortest=1[v]",
                w = width, h = height, fps = fps, bg = bg_color,
                r = &fg_color[0..2], g = &fg_color[2..4], b = &fg_color[4..6],
            )
        }
        VideoStyle::Showspectrum => {
            format!(
                "[0:a]showspectrum=s={w}x{h}:mode=combined:slide=scroll:color=intensity:scale=log[v]",
                w = width, h = height,
            )
        }
        VideoStyle::Particles => {
            unreachable!("Particles style is handled separately")
        }
    }
}

/// Build a waveform filter for overlay mode (smaller height, transparent-ish).
fn build_wave_filter(
    style: &VideoStyle,
    width: u32,
    height: u32,
    fps: u32,
    fg_color: &str,
) -> String {
    match style {
        VideoStyle::Showwaves => {
            format!(
                "showwaves=s={w}x{h}:mode=cline:rate={fps}:colors=0x{fg}:scale=sqrt",
                w = width, h = height, fps = fps, fg = fg_color,
            )
        }
        VideoStyle::Showfreqs => {
            format!(
                "showfreqs=s={w}x{h}:mode=bar:fscale=log:colors=0x{fg}|0x{fg}88:win_size=2048",
                w = width, h = height, fg = fg_color,
            )
        }
        VideoStyle::Avectorscope => {
            format!(
                "avectorscope=s={w}x{h}:mode=lissajous:rate={fps}:rc=0x{r}:gc=0x{g}:bc=0x{b}",
                w = width, h = height, fps = fps,
                r = &fg_color[0..2], g = &fg_color[2..4], b = &fg_color[4..6],
            )
        }
        VideoStyle::Showspectrum => {
            format!(
                "showspectrum=s={w}x{h}:mode=combined:slide=scroll:color=intensity:scale=log",
                w = width, h = height,
            )
        }
        VideoStyle::Particles => {
            unreachable!("Particles style is handled separately")
        }
    }
}

#[tauri::command]
pub async fn export_video(
    app: tauri::AppHandle,
    config: VideoExportConfig,
) -> Result<String, AppError> {
    info!(
        "[Video] export_video: audio={}, output={}, style={:?}",
        config.audio_path, config.output_path, config.style
    );

    // Validate audio file exists
    if !std::path::Path::new(&config.audio_path).exists() {
        return Err(AppError::FileSystem(format!(
            "Audio file not found: {}. Please export audio first.",
            config.audio_path
        )));
    }

    // Validate background image if provided
    if let Some(ref bg_img) = config.bg_image_path {
        if !std::path::Path::new(bg_img).exists() {
            return Err(AppError::FileSystem(format!(
                "Background image not found: {}",
                bg_img
            )));
        }
    }

    // Ensure output directory exists
    if let Some(parent) = std::path::Path::new(&config.output_path).parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            AppError::FileSystem(format!("Failed to create output directory: {}", e))
        })?;
    }

    // Route to particle renderer for Particles style
    if matches!(config.style, VideoStyle::Particles) {
        let width = config.width.unwrap_or(1920);
        let height = config.height.unwrap_or(1080);
        let fps = config.fps.unwrap_or(30);
        let symmetry_folds = config.symmetry_folds.unwrap_or(8);
        let bg_color = parse_hex_color(config.bg_color.as_deref().unwrap_or("1a1a2e"));
        let base_hue = hex_to_hue(config.fg_color.as_deref().unwrap_or("6366f1"));

        let particle_config = super::particles::renderer::ParticleVideoConfig {
            audio_path: std::path::PathBuf::from(&config.audio_path),
            output_path: std::path::PathBuf::from(&config.output_path),
            width,
            height,
            fps,
            symmetry_folds,
            base_hue,
            bg_color,
        };

        let output_path = config.output_path.clone();
        let app_clone = app.clone();

        // Run in blocking thread since it's CPU-intensive
        tokio::task::spawn_blocking(move || {
            super::particles::render_particle_video(&particle_config, |progress| {
                let _ = app_clone.emit("video-progress", progress);
            })
        })
        .await
        .map_err(|e| AppError::FFmpeg(format!("Particle render task failed: {}", e)))??;

        return Ok(output_path);
    }

    // FFmpeg-based styles
    let _ = app.emit(
        "video-progress",
        MixProgress {
            percent: 0.0,
            stage: "正在准备视频生成".to_string(),
        },
    );

    let ffmpeg_bin = find_ffmpeg();
    let ffmpeg_args = build_video_ffmpeg_args(&config);

    info!("[Video] FFmpeg command: {} {}", ffmpeg_bin, ffmpeg_args.join(" "));

    let _ = app.emit(
        "video-progress",
        MixProgress {
            percent: 10.0,
            stage: "正在生成可视化视频".to_string(),
        },
    );

    // Get audio duration for progress estimation
    let duration_secs = get_audio_duration_secs(&ffmpeg_bin, &config.audio_path);

    // Run FFmpeg with progress parsing
    let output_path = config.output_path.clone();
    let app_clone = app.clone();

    let result = tokio::task::spawn_blocking(move || {
        let mut child = std::process::Command::new(&ffmpeg_bin)
            .args(&ffmpeg_args)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()?;

        // Read stderr for progress (ffmpeg outputs progress to stderr)
        if let Some(stderr) = child.stderr.take() {
            use std::io::{BufRead, BufReader};
            let reader = BufReader::new(stderr);
            for line in reader.lines() {
                if let Ok(line) = line {
                    // Parse "time=HH:MM:SS.ms" from ffmpeg output
                    if let Some(time_str) = extract_time_from_ffmpeg_line(&line) {
                        let current_secs = parse_time_to_secs(&time_str);
                        if let Some(total) = duration_secs {
                            if total > 0.0 {
                                let pct = (current_secs / total * 80.0 + 10.0).min(90.0);
                                let _ = app_clone.emit(
                                    "video-progress",
                                    MixProgress {
                                        percent: pct as f32,
                                        stage: format!(
                                            "正在渲染视频 {:.0}%",
                                            pct
                                        ),
                                    },
                                );
                            }
                        }
                    }
                }
            }
        }

        child.wait_with_output()
    })
    .await
    .map_err(|e| AppError::FFmpeg(format!("FFmpeg task failed: {}", e)))?
    .map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            AppError::FFmpeg(
                "FFmpeg not found. Please install FFmpeg (brew install ffmpeg) and ensure it is in your PATH."
                    .to_string(),
            )
        } else {
            AppError::FFmpeg(format!("Failed to start FFmpeg: {}", e))
        }
    })?;

    if !result.status.success() {
        let stderr = String::from_utf8_lossy(&result.stderr);
        return Err(AppError::FFmpeg(format!(
            "FFmpeg video export failed: {}",
            stderr.chars().take(500).collect::<String>()
        )));
    }

    let _ = app.emit(
        "video-progress",
        MixProgress {
            percent: 100.0,
            stage: "视频生成完成".to_string(),
        },
    );

    info!("[Video] export_video done: {}", output_path);
    Ok(output_path)
}

/// Extract time string from an ffmpeg progress line like "time=00:01:23.45"
fn extract_time_from_ffmpeg_line(line: &str) -> Option<String> {
    if let Some(idx) = line.find("time=") {
        let after = &line[idx + 5..];
        let end = after.find(|c: char| c == ' ' || c == '\n' || c == '\r').unwrap_or(after.len());
        let time_str = &after[..end];
        if time_str.contains(':') {
            return Some(time_str.to_string());
        }
    }
    None
}

/// Parse "HH:MM:SS.ms" to seconds.
fn parse_time_to_secs(time: &str) -> f64 {
    let parts: Vec<&str> = time.split(':').collect();
    if parts.len() == 3 {
        let h: f64 = parts[0].parse().unwrap_or(0.0);
        let m: f64 = parts[1].parse().unwrap_or(0.0);
        let s: f64 = parts[2].parse().unwrap_or(0.0);
        h * 3600.0 + m * 60.0 + s
    } else {
        0.0
    }
}

/// Get audio duration in seconds using ffprobe.
fn get_audio_duration_secs(ffmpeg_bin: &str, audio_path: &str) -> Option<f64> {
    // Derive ffprobe path from ffmpeg path
    let ffprobe_bin = ffmpeg_bin.replace("ffmpeg", "ffprobe");
    let output = std::process::Command::new(&ffprobe_bin)
        .args([
            "-v", "error",
            "-show_entries", "format=duration",
            "-of", "default=noprint_wrappers=1:nokey=1",
            audio_path,
        ])
        .output()
        .ok()?;

    if output.status.success() {
        let s = String::from_utf8_lossy(&output.stdout);
        s.trim().parse::<f64>().ok()
    } else {
        None
    }
}

/// Parse a hex color string (e.g. "1a1a2e") to (r, g, b).
fn parse_hex_color(hex: &str) -> (u8, u8, u8) {
    let hex = hex.trim_start_matches('#');
    if hex.len() >= 6 {
        let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(0);
        let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(0);
        let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(0);
        (r, g, b)
    } else {
        (26, 26, 46) // default dark
    }
}

/// Convert a hex color to an approximate hue value (0-360).
fn hex_to_hue(hex: &str) -> f32 {
    let (r, g, b) = parse_hex_color(hex);
    let rf = r as f32 / 255.0;
    let gf = g as f32 / 255.0;
    let bf = b as f32 / 255.0;

    let max = rf.max(gf).max(bf);
    let min = rf.min(gf).min(bf);
    let delta = max - min;

    if delta < 0.001 {
        return 0.0;
    }

    let hue = if max == rf {
        60.0 * (((gf - bf) / delta) % 6.0)
    } else if max == gf {
        60.0 * (((bf - rf) / delta) + 2.0)
    } else {
        60.0 * (((rf - gf) / delta) + 4.0)
    };

    if hue < 0.0 { hue + 360.0 } else { hue }
}
