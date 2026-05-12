//! Vinyl disc video renderer — orchestration layer.
//!
//! Handles audio analysis, FFmpeg pipeline, and frame loop.
//! Delegates actual frame rendering to GPU (gpu.rs) or CPU (draw.rs).

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;

use log::info;

use super::draw::render_vinyl_frame;
use super::gpu::{GpuVinylRenderer, VinylParams};
use super::texture::{generate_bokeh_particles, CoverTexture};
use super::utils::precompute_vinyl_bg;
use crate::commands::audio::ffmpeg::find_ffmpeg;
use crate::commands::audio::particles::audio_analysis::{extract_audio_features, FrameFeatures};
use crate::core::error::AppError;
use crate::core::models::MixProgress;

/// Configuration for vinyl video rendering.
pub struct VinylVideoConfig {
    pub audio_path: PathBuf,
    pub output_path: PathBuf,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub cover_image_path: Option<PathBuf>,
    pub fg_color: (u8, u8, u8),
    pub bg_color: (u8, u8, u8),
}

/// Render a vinyl visualization video from audio.
pub fn render_vinyl_video<F>(
    config: &VinylVideoConfig,
    on_progress: F,
    cancel_flag: &Arc<AtomicBool>,
) -> Result<(), AppError>
where
    F: Fn(MixProgress),
{
    on_progress(MixProgress {
        percent: 0.0,
        stage: "正在分析音频特征".to_string(),
    });

    let features = extract_audio_features(&config.audio_path, config.fps)
        .map_err(|e| AppError::FFmpeg(format!("Audio analysis failed: {}", e)))?;

    let total_frames = features.len();
    info!(
        "[Vinyl] {} frames to render at {}fps ({}x{})",
        total_frames, config.fps, config.width, config.height
    );

    if total_frames == 0 {
        return Err(AppError::FFmpeg("No audio frames to render".to_string()));
    }

    // Render at half resolution, FFmpeg upscales with lanczos
    let render_width = config.width / 2;
    let render_height = config.height / 2;

    // Prepare cover texture
    let disc_size = (render_height as f32 * 0.50) as u32;
    let cover_texture = if let Some(ref cover_path) = config.cover_image_path {
        on_progress(MixProgress {
            percent: 2.0,
            stage: "正在加载封面图片".to_string(),
        });
        CoverTexture::load(cover_path, disc_size)
            .unwrap_or_else(|_| CoverTexture::default_vinyl(disc_size, config.fg_color))
    } else {
        CoverTexture::default_vinyl(disc_size, config.fg_color)
    };

    let bokeh_particles = generate_bokeh_particles(render_width, render_height, 20);

    // Try GPU renderer
    let gpu_renderer = GpuVinylRenderer::new(
        render_width, render_height,
        &cover_texture.pixels, cover_texture.size,
    );
    let use_gpu = gpu_renderer.is_some();

    if use_gpu {
        info!("[Vinyl] Using GPU-accelerated rendering");
        on_progress(MixProgress { percent: 5.0, stage: "GPU 加速已启用，准备渲染".to_string() });
    } else {
        info!("[Vinyl] GPU unavailable, falling back to CPU (rayon)");
        on_progress(MixProgress { percent: 5.0, stage: format!("准备完成，共 {} 帧 (CPU 模式)", total_frames) });
    }

    // Start FFmpeg
    let scale_filter = format!("scale={}:{}:flags=lanczos", config.width, config.height);
    let ffmpeg_bin = find_ffmpeg();
    let mut child = Command::new(&ffmpeg_bin)
        .args([
            "-y", "-f", "rawvideo", "-pixel_format", "rgba",
            "-video_size", &format!("{}x{}", render_width, render_height),
            "-framerate", &config.fps.to_string(),
            "-i", "pipe:0",
            "-i", &config.audio_path.to_string_lossy(),
            "-vf", &scale_filter,
            "-c:v", "libx264", "-preset", "ultrafast", "-tune", "stillimage",
            "-crf", "23", "-pix_fmt", "yuv420p",
            "-c:a", "aac", "-b:a", "192k",
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

    // Writer thread
    let (tx, rx) = mpsc::sync_channel::<Vec<u8>>(4);
    let writer_handle = std::thread::spawn(move || -> Result<(), String> {
        for frame_data in rx {
            if stdin.write_all(&frame_data).is_err() { break; }
        }
        drop(stdin);
        Ok(())
    });

    let width = render_width;
    let height = render_height;
    let fg_color = config.fg_color;
    let bg_color = config.bg_color;

    let bg_frame = precompute_vinyl_bg(width as usize, height as usize, bg_color);
    let rotation_speed = std::f32::consts::TAU / (5.0 * config.fps as f32);

    let mut smooth_rms: f32 = 0.0;
    let mut smooth_low: f32 = 0.0;
    let mut smooth_mid: f32 = 0.0;
    let mut smooth_high: f32 = 0.0;

    // ─── Frame Loop ──────────────────────────────────────────────────────
    for (frame_idx, frame_features) in features.iter().enumerate() {
        if cancel_flag.load(Ordering::Relaxed) {
            info!("[Vinyl] Render cancelled at frame {}/{}", frame_idx, total_frames);
            break;
        }

        let smoothing = 0.15;
        smooth_rms += (frame_features.rms - smooth_rms) * smoothing;
        smooth_low += (frame_features.low_energy - smooth_low) * smoothing;
        smooth_mid += (frame_features.mid_energy - smooth_mid) * smoothing;
        smooth_high += (frame_features.high_energy - smooth_high) * smoothing;

        let smoothed = FrameFeatures {
            rms: smooth_rms,
            low_energy: smooth_low,
            mid_energy: smooth_mid,
            high_energy: smooth_high,
        };

        let angle = frame_idx as f32 * rotation_speed;

        let frame_data = if let Some(ref gpu) = gpu_renderer {
            let disc_radius = cover_texture.size as f32 / 2.0;
            let params = VinylParams {
                width: width as f32, height: height as f32,
                disc_radius, angle,
                rms: smooth_rms, low_energy: smooth_low,
                mid_energy: smooth_mid, high_energy: smooth_high,
                fg_r: fg_color.0 as f32 / 255.0, fg_g: fg_color.1 as f32 / 255.0, fg_b: fg_color.2 as f32 / 255.0,
                bg_r: bg_color.0 as f32 / 255.0, bg_g: bg_color.1 as f32 / 255.0, bg_b: bg_color.2 as f32 / 255.0,
                frame_time: frame_idx as f32,
                pulse_scale: 1.0 + smooth_low * 0.02,
                eq_rotation: frame_idx as f32 * 0.002,
                _pad1: 0.0, _pad2: 0.0, _pad3: 0.0,
            };
            gpu.render_frame(&params)
        } else {
            render_vinyl_frame(
                width, height, &cover_texture, angle,
                &smoothed, frame_features, fg_color, bg_color,
                frame_idx as u32, &bg_frame, &bokeh_particles,
            )
        };

        if tx.send(frame_data).is_err() { break; }

        if frame_idx % 30 == 0 || frame_idx == total_frames - 1 {
            let pct = 5.0 + (frame_idx as f32 / total_frames as f32) * 90.0;
            on_progress(MixProgress { percent: pct, stage: format!("渲染帧 {}/{}", frame_idx + 1, total_frames) });
        }
    }

    drop(tx);

    writer_handle.join()
        .map_err(|_| AppError::FFmpeg("Writer thread panicked".to_string()))?
        .map_err(|e| AppError::FFmpeg(e))?;

    let output = child.wait_with_output()
        .map_err(|e| AppError::FFmpeg(format!("FFmpeg wait failed: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AppError::FFmpeg(format!("FFmpeg encoding failed: {}", stderr.chars().take(500).collect::<String>())));
    }

    on_progress(MixProgress { percent: 100.0, stage: "视频生成完成".to_string() });
    info!("[Vinyl] Video rendered successfully (GPU={}): {:?}", use_gpu, config.output_path);
    Ok(())
}
