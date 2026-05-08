//! Frame renderer — draws particles with kaleidoscope symmetry using tiny-skia,
//! pipes raw RGBA pixels directly to FFmpeg stdin (no PNG files, no disk I/O).

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use log::info;
use tiny_skia::{Color, Paint, PathBuilder, Pixmap, Transform};

use super::audio_analysis::{extract_audio_features, FrameFeatures};
use super::particle_system::{hsl_to_rgb, ParticleConfig, ParticleSystem};
use crate::core::error::AppError;
use crate::core::models::MixProgress;

use crate::commands::audio::ffmpeg::find_ffmpeg;

/// Configuration for particle video rendering.
pub struct ParticleVideoConfig {
    pub audio_path: PathBuf,
    pub output_path: PathBuf,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub symmetry_folds: u32,
    pub base_hue: f32,
    pub bg_color: (u8, u8, u8),
}

/// Render a particle visualization video from audio.
/// Pipes raw pixel data directly to FFmpeg — no temp files needed.
pub fn render_particle_video<F>(
    config: &ParticleVideoConfig,
    on_progress: F,
) -> Result<(), AppError>
where
    F: Fn(MixProgress),
{
    on_progress(MixProgress {
        percent: 0.0,
        stage: "正在分析音频特征".to_string(),
    });

    // Step 1: Extract audio features
    let features = extract_audio_features(&config.audio_path, config.fps)
        .map_err(|e| AppError::FFmpeg(format!("Audio analysis failed: {}", e)))?;

    let total_frames = features.len();
    info!(
        "[Particles] {} frames to render at {}fps ({}x{})",
        total_frames, config.fps, config.width, config.height
    );

    if total_frames == 0 {
        return Err(AppError::FFmpeg("No audio frames to render".to_string()));
    }

    on_progress(MixProgress {
        percent: 5.0,
        stage: format!("音频分析完成，共 {} 帧", total_frames),
    });

    // Step 2: Start FFmpeg process that reads raw RGBA from stdin
    let ffmpeg_bin = find_ffmpeg();
    let mut child = Command::new(&ffmpeg_bin)
        .args([
            "-y",
            // Input: raw video from pipe
            "-f", "rawvideo",
            "-pixel_format", "rgba",
            "-video_size", &format!("{}x{}", config.width, config.height),
            "-framerate", &config.fps.to_string(),
            "-i", "pipe:0",
            // Input: audio file
            "-i", &config.audio_path.to_string_lossy(),
            // Video encoding
            "-c:v", "libx264",
            "-preset", "fast",
            "-crf", "22",
            "-pix_fmt", "yuv420p",
            // Audio encoding
            "-c:a", "aac",
            "-b:a", "192k",
            "-shortest",
            &config.output_path.to_string_lossy(),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                AppError::FFmpeg("FFmpeg not found. Please install FFmpeg.".to_string())
            } else {
                AppError::FFmpeg(format!("Failed to start FFmpeg: {}", e))
            }
        })?;

    let mut stdin = child.stdin.take()
        .ok_or_else(|| AppError::FFmpeg("Failed to open FFmpeg stdin".to_string()))?;

    // Step 3: Initialize particle system
    let particle_config = ParticleConfig {
        symmetry_folds: config.symmetry_folds,
        max_spawn_rate: 12,
        speed_multiplier: 1.0,
        base_hue: config.base_hue,
        hue_range: 120.0,
    };
    let mut system = ParticleSystem::new(particle_config);

    // Step 4: Render frames and pipe directly to FFmpeg
    let render_progress_start = 5.0;
    let render_progress_end = 95.0;

    // Reuse a single pixmap across frames to avoid allocation
    let mut pixmap = Pixmap::new(config.width, config.height)
        .ok_or_else(|| AppError::FFmpeg("Failed to create pixmap".to_string()))?;

    for (frame_idx, frame_features) in features.iter().enumerate() {
        // Update particle system
        system.update(frame_features);

        // Render frame into the reused pixmap
        render_frame_into(
            &mut pixmap,
            &system,
            config.width,
            config.height,
            config.symmetry_folds,
            config.bg_color,
            frame_features,
        );

        // Write raw RGBA pixels directly to FFmpeg stdin
        if stdin.write_all(pixmap.data()).is_err() {
            break; // FFmpeg closed pipe (e.g. error)
        }

        // Progress update every 30 frames (~1 second)
        if frame_idx % 30 == 0 || frame_idx == total_frames - 1 {
            let pct = render_progress_start
                + (frame_idx as f32 / total_frames as f32) * (render_progress_end - render_progress_start);
            on_progress(MixProgress {
                percent: pct,
                stage: format!("渲染帧 {}/{}", frame_idx + 1, total_frames),
            });
        }
    }

    // Close stdin to signal EOF to FFmpeg
    drop(stdin);

    // Wait for FFmpeg to finish
    let output = child.wait_with_output()
        .map_err(|e| AppError::FFmpeg(format!("FFmpeg wait failed: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AppError::FFmpeg(format!(
            "FFmpeg encoding failed: {}",
            stderr.chars().take(500).collect::<String>()
        )));
    }

    on_progress(MixProgress {
        percent: 100.0,
        stage: "视频生成完成".to_string(),
    });

    info!("[Particles] Video rendered successfully: {:?}", config.output_path);
    Ok(())
}

/// Render a single frame with kaleidoscope symmetry into an existing pixmap.
fn render_frame_into(
    pixmap: &mut Pixmap,
    system: &ParticleSystem,
    width: u32,
    height: u32,
    symmetry_folds: u32,
    bg_color: (u8, u8, u8),
    features: &FrameFeatures,
) {
    // Clear with background color
    pixmap.fill(Color::from_rgba8(bg_color.0, bg_color.1, bg_color.2, 255));

    let cx = width as f32 / 2.0;
    let cy = height as f32 / 2.0;
    // Use the longer edge as scale so the pattern fills the width,
    // with top/bottom naturally clipped — keeps circles round.
    let scale = width.max(height) as f32 / 2.0;

    let angle_step = std::f32::consts::TAU / symmetry_folds as f32;

    // Pre-compute sin/cos for each fold
    let fold_angles: Vec<(f32, f32)> = (0..symmetry_folds)
        .map(|fold| {
            let angle = fold as f32 * angle_step;
            (angle.cos(), angle.sin())
        })
        .collect();

    // Draw each particle with N-fold symmetry
    for particle in &system.particles {
        let alpha = (particle.life * particle.brightness * 255.0).clamp(0.0, 255.0) as u8;
        if alpha < 5 {
            continue;
        }

        let (r, g, b) = hsl_to_rgb(particle.hue, particle.saturation, 0.55 + particle.life * 0.15);

        let mut paint = Paint::default();
        paint.set_color(Color::from_rgba8(r, g, b, alpha));
        paint.anti_alias = true;

        let size = particle.size * (0.5 + features.rms * 0.5);

        for &(cos_a, sin_a) in &fold_angles {
            // Rotate particle position
            let rx = particle.x * cos_a - particle.y * sin_a;
            let ry = particle.x * sin_a + particle.y * cos_a;

            // Draw original + mirror
            let positions = [(rx, ry), (rx, -ry)];

            for &(px, py) in &positions {
                let screen_x = cx + px * scale;
                let screen_y = cy + py * scale;

                // Skip if off-screen
                if screen_x < -size || screen_x > width as f32 + size
                    || screen_y < -size || screen_y > height as f32 + size
                {
                    continue;
                }

                // Draw circle
                if let Some(path) = PathBuilder::from_circle(screen_x, screen_y, size) {
                    pixmap.fill_path(
                        &path,
                        &paint,
                        tiny_skia::FillRule::Winding,
                        Transform::identity(),
                        None,
                    );
                }
            }
        }
    }

    // Center glow based on RMS
    if features.rms > 0.1 {
        let glow_size = 20.0 + features.rms * 60.0;
        let glow_alpha = (features.rms * 80.0).clamp(0.0, 80.0) as u8;
        let (gr, gg, gb) = hsl_to_rgb(system.config.base_hue, 0.8, 0.6);

        let mut glow_paint = Paint::default();
        glow_paint.set_color(Color::from_rgba8(gr, gg, gb, glow_alpha));
        glow_paint.anti_alias = true;

        if let Some(path) = PathBuilder::from_circle(cx, cy, glow_size) {
            pixmap.fill_path(
                &path,
                &glow_paint,
                tiny_skia::FillRule::Winding,
                Transform::identity(),
                None,
            );
        }
    }
}
