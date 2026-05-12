//! Particle kaleidoscope video renderer — orchestration layer.
//!
//! Handles audio analysis, FFmpeg pipeline, and frame loop.
//! Delegates actual frame rendering to GPU (gpu.rs) or CPU (draw.rs).

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use log::info;

use super::audio_analysis::extract_audio_features;
use super::draw::{precompute_bg_gradient, render_frame_cpu, CircleTemplate};
use super::gpu::{GpuParticleRenderer, ParticleParams};
use super::particle_system::{ParticleConfig, ParticleSystem};
use crate::commands::audio::ffmpeg::spawn_video_encoder;
use crate::core::error::AppError;
use crate::core::models::MixProgress;

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
pub fn render_particle_video<F>(
    config: &ParticleVideoConfig,
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

    // Render at half resolution, FFmpeg upscales with lanczos
    let render_width = config.width / 2;
    let render_height = config.height / 2;

    // Start FFmpeg encoder pipeline
    let (encoder, tx) = spawn_video_encoder(
        render_width, render_height,
        config.width, config.height,
        config.fps,
        &config.audio_path.to_string_lossy(),
        &config.output_path.to_string_lossy(),
    )?;

    // Initialize particle system
    let particle_config = ParticleConfig {
        symmetry_folds: config.symmetry_folds,
        max_spawn_rate: 16,
        speed_multiplier: 1.0,
        base_hue: config.base_hue,
        hue_range: 120.0,
    };
    let mut system = ParticleSystem::new(particle_config);

    // Try GPU renderer
    let gpu_renderer = GpuParticleRenderer::new(render_width, render_height);
    let use_gpu = gpu_renderer.is_some();

    if use_gpu {
        info!("[Particles] Using GPU-accelerated rendering");
    } else {
        info!("[Particles] GPU unavailable, falling back to CPU (rayon)");
    }

    // CPU fallback resources
    let circle_templates = CircleTemplate::new(24);
    let angle_step = std::f32::consts::TAU / config.symmetry_folds as f32;
    let fold_angles: Vec<(f32, f32)> = (0..config.symmetry_folds)
        .map(|fold| {
            let angle = fold as f32 * angle_step;
            (angle.cos(), angle.sin())
        })
        .collect();

    let width = render_width;
    let height = render_height;
    let cx = width as f32 / 2.0;
    let cy = height as f32 / 2.0;
    let scale = width.max(height) as f32 / 2.0;
    let bg_color = config.bg_color;
    let base_hue = config.base_hue;

    let bg_base = precompute_bg_gradient(width as usize, height as usize, bg_color);

    // ─── Frame Loop ──────────────────────────────────────────────────────
    for (frame_idx, frame_features) in features.iter().enumerate() {
        if cancel_flag.load(Ordering::Relaxed) {
            info!("[Particles] Render cancelled at frame {}/{}", frame_idx, total_frames);
            break;
        }

        system.update(frame_features);

        let frame_data = if let Some(ref gpu) = gpu_renderer {
            let params = ParticleParams {
                width: render_width,
                height: render_height,
                num_particles: system.particles.len().min(2000) as u32,
                num_rings: system.rings.len().min(32) as u32,
                symmetry_folds: config.symmetry_folds,
                frame_idx: frame_idx as u32,
                _pad0: 0,
                _pad1: 0,
                rms: frame_features.rms,
                low_energy: frame_features.low_energy,
                mid_energy: frame_features.mid_energy,
                high_energy: frame_features.high_energy,
                bg_r: bg_color.0 as f32 / 255.0,
                bg_g: bg_color.1 as f32 / 255.0,
                bg_b: bg_color.2 as f32 / 255.0,
                base_hue,
            };
            gpu.render_frame(&params, &system)
        } else {
            render_frame_cpu(
                &system, width, height, cx, cy, scale,
                &fold_angles, bg_color, base_hue,
                frame_features, &circle_templates, frame_idx as u32, &bg_base,
            )
        };

        if tx.send(frame_data).is_err() {
            break;
        }

        if frame_idx % 30 == 0 || frame_idx == total_frames - 1 {
            let pct = 5.0 + (frame_idx as f32 / total_frames as f32) * 90.0;
            on_progress(MixProgress {
                percent: pct,
                stage: format!("渲染帧 {}/{}", frame_idx + 1, total_frames),
            });
        }
    }

    drop(tx);

    encoder.finish()?;

    on_progress(MixProgress {
        percent: 100.0,
        stage: "视频生成完成".to_string(),
    });

    info!("[Particles] Video rendered successfully (GPU={}): {:?}", use_gpu, config.output_path);
    Ok(())
}
