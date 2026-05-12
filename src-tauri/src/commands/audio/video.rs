//! Video export — generates visualization videos from audio using dedicated renderers.

use log::info;
use tauri::{Emitter, Manager};

use crate::core::cancel_token::VideoCancelToken;
use crate::core::error::AppError;

/// Supported visualization styles for video export.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VideoStyle {
    /// Particle kaleidoscope (audio-driven particles with symmetry)
    Particles,
    /// Vinyl/CD spinning disc with cover image
    Vinyl,
    /// Starfield tunnel (classic Windows screensaver, audio-reactive)
    Starfield,
    /// Infinite Mandelbrot fractal zoom (audio-reactive colors and speed)
    Fractal,
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

#[tauri::command]
pub async fn export_video(
    app: tauri::AppHandle,
    config: VideoExportConfig,
) -> Result<String, AppError> {
    info!(
        "[Video] export_video: audio={}, output={}, style={:?}",
        config.audio_path, config.output_path, config.style
    );

    // Reset and get the cancel flag
    let cancel_token = app.state::<VideoCancelToken>();
    cancel_token.reset();
    let cancel_flag = cancel_token.flag();

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

    let width = config.width.unwrap_or(1280);
    let height = config.height.unwrap_or(720);
    let fps = config.fps.unwrap_or(30);
    let output_path = config.output_path.clone();

    match config.style {
        VideoStyle::Particles => {
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

            let app_clone = app.clone();
            let cancel = cancel_flag.clone();

            tokio::task::spawn_blocking(move || {
                super::particles::render_particle_video(&particle_config, |progress| {
                    let _ = app_clone.emit("video-progress", progress);
                }, &cancel)
            })
            .await
            .map_err(|e| AppError::FFmpeg(format!("Particle render task failed: {}", e)))??;
        }

        VideoStyle::Vinyl => {
            let fg_color = parse_hex_color(config.fg_color.as_deref().unwrap_or("6366f1"));
            let bg_color = parse_hex_color(config.bg_color.as_deref().unwrap_or("1a1a2e"));

            let vinyl_config = super::vinyl::renderer::VinylVideoConfig {
                audio_path: std::path::PathBuf::from(&config.audio_path),
                output_path: std::path::PathBuf::from(&config.output_path),
                width,
                height,
                fps,
                cover_image_path: config.bg_image_path.as_ref().map(|p| std::path::PathBuf::from(p)),
                fg_color,
                bg_color,
            };

            let app_clone = app.clone();
            let cancel = cancel_flag.clone();

            tokio::task::spawn_blocking(move || {
                super::vinyl::render_vinyl_video(&vinyl_config, |progress| {
                    let _ = app_clone.emit("video-progress", progress);
                }, &cancel)
            })
            .await
            .map_err(|e| AppError::FFmpeg(format!("Vinyl render task failed: {}", e)))??;
        }

        VideoStyle::Starfield => {
            let fg_color = parse_hex_color(config.fg_color.as_deref().unwrap_or("6366f1"));
            let bg_color = parse_hex_color(config.bg_color.as_deref().unwrap_or("0a0a1a"));

            let starfield_config = super::starfield::renderer::StarfieldVideoConfig {
                audio_path: std::path::PathBuf::from(&config.audio_path),
                output_path: std::path::PathBuf::from(&config.output_path),
                width,
                height,
                fps,
                fg_color,
                bg_color,
                bg_image_path: config.bg_image_path.as_ref().map(|p| std::path::PathBuf::from(p)),
            };

            let app_clone = app.clone();
            let cancel = cancel_flag.clone();

            tokio::task::spawn_blocking(move || {
                super::starfield::render_starfield_video(&starfield_config, |progress| {
                    let _ = app_clone.emit("video-progress", progress);
                }, &cancel)
            })
            .await
            .map_err(|e| AppError::FFmpeg(format!("Starfield render task failed: {}", e)))??;
        }

        VideoStyle::Fractal => {
            let fg_color = parse_hex_color(config.fg_color.as_deref().unwrap_or("6366f1"));
            let bg_color = parse_hex_color(config.bg_color.as_deref().unwrap_or("0a0a12"));

            let fractal_config = super::fractal::renderer::FractalVideoConfig {
                audio_path: std::path::PathBuf::from(&config.audio_path),
                output_path: std::path::PathBuf::from(&config.output_path),
                width,
                height,
                fps,
                fg_color,
                bg_color,
            };

            let app_clone = app.clone();
            let cancel = cancel_flag.clone();

            tokio::task::spawn_blocking(move || {
                super::fractal::render_fractal_video(&fractal_config, |progress| {
                    let _ = app_clone.emit("video-progress", progress);
                }, &cancel)
            })
            .await
            .map_err(|e| AppError::FFmpeg(format!("Fractal render task failed: {}", e)))??;
        }

    }

    info!("[Video] export_video done: {}", output_path);
    Ok(output_path)
}

#[tauri::command]
pub fn cancel_video_export(app: tauri::AppHandle) {
    let cancel_token = app.state::<VideoCancelToken>();
    cancel_token.cancel();
    info!("[Video] Video export cancelled by user");
}

// ─── Utilities ───────────────────────────────────────────────────────────────

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
