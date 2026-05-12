//! Fractal zoom renderer — infinite Mandelbrot zoom with audio-reactive coloring.
//!
//! The camera continuously zooms into a carefully chosen deep point in the
//! Mandelbrot set. Audio energy controls:
//! - Zoom speed (bass = faster dive)
//! - Color palette cycling (mid = hue rotation)
//! - Iteration glow intensity (high = brighter edges)
//!
//! Performance: Uses wgpu compute shaders for GPU-accelerated rendering.
//! Falls back to CPU (rayon) if GPU is unavailable.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use log::info;
use rayon::prelude::*;

use super::gpu::{FractalParams, GpuFractalRenderer};
use crate::commands::audio::ffmpeg::spawn_video_encoder;
use crate::commands::audio::particles::audio_analysis::extract_audio_features;
use crate::core::error::AppError;
use crate::core::models::MixProgress;

/// Configuration for fractal video rendering.
pub struct FractalVideoConfig {
    pub audio_path: PathBuf,
    pub output_path: PathBuf,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    /// Foreground/accent color as (r, g, b) — base palette color
    pub fg_color: (u8, u8, u8),
    /// Background color as (r, g, b) — color for points inside the set
    pub bg_color: (u8, u8, u8),
}

/// A deep zoom target point in the Mandelbrot set.
struct ZoomTarget {
    cx: f64,
    cy: f64,
}

/// Several interesting deep zoom targets — carefully chosen points on the
/// Mandelbrot set boundary that produce rich, never-ending detail.
const ZOOM_TARGETS: &[ZoomTarget] = &[
    // Seahorse valley spiral
    ZoomTarget {
        cx: -0.743643887037158704752191506114774,
        cy: 0.131825904205311970493132056385139,
    },
    // Elephant valley
    ZoomTarget {
        cx: 0.281717921930775,
        cy: 0.5771052841488505,
    },
    // Double spiral
    ZoomTarget {
        cx: -0.8624011862235098,
        cy: 0.21478827404879898,
    },
    // Mini-brot in antenna
    ZoomTarget {
        cx: -1.7497591451,
        cy: 0.0000000001,
    },
    // Spiral near seahorse valley
    ZoomTarget {
        cx: -0.7463,
        cy: 0.1102,
    },
    // Deep spiral arm
    ZoomTarget {
        cx: -0.16,
        cy: 1.0405,
    },
];

/// Render a fractal zoom video from audio.
pub fn render_fractal_video<F>(
    config: &FractalVideoConfig,
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
        "[Fractal] {} frames to render at {}fps ({}x{})",
        total_frames, config.fps, config.width, config.height
    );

    if total_frames == 0 {
        return Err(AppError::FFmpeg("No audio frames to render".to_string()));
    }

    on_progress(MixProgress {
        percent: 5.0,
        stage: format!("准备完成，共 {} 帧", total_frames),
    });

    // Choose zoom target based on audio length (deterministic)
    let target_idx = (total_frames / 100) % ZOOM_TARGETS.len();

    // Render at half resolution for performance, FFmpeg upscales with lanczos
    let render_width = config.width / 2;
    let render_height = config.height / 2;

    // Try to initialize GPU renderer
    let gpu_renderer = GpuFractalRenderer::new(render_width, render_height);
    let use_gpu = gpu_renderer.is_some();

    if use_gpu {
        info!("[Fractal] Using GPU-accelerated rendering");
        on_progress(MixProgress {
            percent: 5.0,
            stage: "GPU 加速已启用，准备渲染".to_string(),
        });
    } else {
        info!("[Fractal] GPU unavailable, falling back to CPU (rayon)");
        on_progress(MixProgress {
            percent: 5.0,
            stage: "GPU 不可用，使用 CPU 渲染".to_string(),
        });
    }

    // Start FFmpeg encoder pipeline
    let (encoder, tx) = spawn_video_encoder(
        render_width, render_height,
        config.width, config.height,
        config.fps,
        &config.audio_path.to_string_lossy(),
        &config.output_path.to_string_lossy(),
    )?;

    let width = render_width;
    let height = render_height;
    let fg_color = config.fg_color;
    let bg_color = config.bg_color;

    // Zoom state
    let mut zoom: f64 = 0.5; // Start zoomed out
    let base_zoom_speed: f64 = 1.008; // ~0.8% per frame base zoom
    let mut smooth_rms: f32 = 0.0;
    let mut smooth_low: f32 = 0.0;
    let mut smooth_mid: f32 = 0.0;
    let mut smooth_high: f32 = 0.0;
    let mut hue_offset: f32 = 0.0;

    // Maximum zoom depth before precision loss causes pure-color frames.
    // For f32 GPU shader: ~10^5 is safe (f32 has ~7 decimal digits).
    // We reset to a new target when approaching this limit.
    let max_zoom: f64 = 80_000.0;
    let mut current_target_idx = target_idx;
    let mut current_target = &ZOOM_TARGETS[current_target_idx];

    for (frame_idx, frame_features) in features.iter().enumerate() {
        if cancel_flag.load(Ordering::Relaxed) {
            info!(
                "[Fractal] Render cancelled at frame {}/{}",
                frame_idx, total_frames
            );
            break;
        }

        // Smooth audio features
        let smoothing = 0.12;
        smooth_rms += (frame_features.rms - smooth_rms) * smoothing;
        smooth_low += (frame_features.low_energy - smooth_low) * smoothing;
        smooth_mid += (frame_features.mid_energy - smooth_mid) * smoothing;
        smooth_high += (frame_features.high_energy - smooth_high) * smoothing;

        // Audio-reactive zoom speed: bass makes it zoom faster
        let zoom_multiplier = 1.0 + smooth_low as f64 * 0.015 + smooth_rms as f64 * 0.005;
        zoom *= base_zoom_speed * zoom_multiplier;

        // Reset zoom when approaching f32 precision limit to avoid pure-color frames.
        // Jump to the next target point for visual variety.
        if zoom > max_zoom {
            current_target_idx = (current_target_idx + 1) % ZOOM_TARGETS.len();
            current_target = &ZOOM_TARGETS[current_target_idx];
            zoom = 0.5; // Reset to zoomed-out view
            info!(
                "[Fractal] Zoom reset at frame {} → target {}",
                frame_idx, current_target_idx
            );
        }

        // Hue rotation driven by mid frequencies
        hue_offset += smooth_mid * 0.5 + 0.1;

        // Max iterations increase with zoom depth, capped for performance
        let max_iter = (80.0 + (zoom.ln() * 10.0).min(120.0)) as u32;

        let frame_data = if let Some(ref gpu) = gpu_renderer {
            // GPU path
            let scale = 2.0 / zoom as f32;
            let aspect = width as f32 / height as f32;

            let params = FractalParams {
                width,
                height,
                max_iter,
                _pad0: 0,
                center_x: current_target.cx as f32,
                center_y: current_target.cy as f32,
                scale,
                aspect,
                fg_r: fg_color.0 as f32 / 255.0,
                fg_g: fg_color.1 as f32 / 255.0,
                fg_b: fg_color.2 as f32 / 255.0,
                hue_offset,
                glow_intensity: 0.3 + smooth_high * 0.7,
                brightness_boost: 1.0 + smooth_rms * 0.3,
                bg_r: bg_color.0 as f32 / 255.0,
                bg_g: bg_color.1 as f32 / 255.0,
                bg_b: bg_color.2 as f32 / 255.0,
                _pad1: 0.0,
                _pad2: 0.0,
                _pad3: 0.0,
            };

            gpu.render_frame(&params)
        } else {
            // CPU fallback path
            render_fractal_frame_cpu(
                width,
                height,
                current_target.cx,
                current_target.cy,
                zoom,
                max_iter,
                fg_color,
                bg_color,
                hue_offset,
                smooth_high,
                smooth_rms,
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

    info!(
        "[Fractal] Video rendered successfully (GPU={}): {:?}",
        use_gpu, config.output_path
    );
    Ok(())
}

// ─── CPU Fallback ────────────────────────────────────────────────────────────

/// Render a single fractal frame using rayon parallel rows (CPU fallback).
fn render_fractal_frame_cpu(
    width: u32,
    height: u32,
    center_x: f64,
    center_y: f64,
    zoom: f64,
    max_iter: u32,
    fg_color: (u8, u8, u8),
    bg_color: (u8, u8, u8),
    hue_offset: f32,
    high_energy: f32,
    rms: f32,
) -> Vec<u8> {
    let w = width as usize;
    let h = height as usize;

    let aspect = width as f64 / height as f64;
    let scale = 2.0 / zoom;

    let x_min = center_x - scale * aspect * 0.5;
    let y_min = center_y - scale * 0.5;
    let x_step = scale * aspect / width as f64;
    let y_step = scale / height as f64;

    // Glow intensity from high frequencies
    let glow_intensity = 0.3 + high_energy * 0.7;
    // Brightness boost from overall energy
    let brightness_boost = 1.0 + rms * 0.3;

    // Parallel row computation with rayon
    let buf: Vec<u8> = (0..h)
        .into_par_iter()
        .flat_map_iter(move |py| {
            let ci = y_min + py as f64 * y_step;
            (0..w).flat_map(move |px| {
                let cr = x_min + px as f64 * x_step;

                let (iter, smooth_val) = mandelbrot_smooth(cr, ci, max_iter);

                if iter >= max_iter {
                    [bg_color.0, bg_color.1, bg_color.2, 255]
                } else {
                    let t = smooth_val / max_iter as f64;
                    let (r, g, b) = fractal_palette(
                        t,
                        fg_color,
                        hue_offset,
                        glow_intensity,
                        brightness_boost,
                    );
                    [r, g, b, 255]
                }
                .into_iter()
            })
        })
        .collect();

    buf
}

/// Mandelbrot iteration with smooth (continuous) escape value.
#[inline]
fn mandelbrot_smooth(cr: f64, ci: f64, max_iter: u32) -> (u32, f64) {
    let mut zr = 0.0_f64;
    let mut zi = 0.0_f64;
    let mut zr2 = 0.0_f64;
    let mut zi2 = 0.0_f64;
    let mut iter = 0u32;

    // Cardioid / period-2 bulb check
    let q = (cr - 0.25) * (cr - 0.25) + ci * ci;
    if q * (q + (cr - 0.25)) <= 0.25 * ci * ci {
        return (max_iter, 0.0);
    }
    if (cr + 1.0) * (cr + 1.0) + ci * ci <= 0.0625 {
        return (max_iter, 0.0);
    }

    let bailout_sq = 65536.0;

    // Period detection
    let mut period_zr = 0.0_f64;
    let mut period_zi = 0.0_f64;
    let mut period_check = 8u32;

    while zr2 + zi2 <= bailout_sq && iter < max_iter {
        zi = 2.0 * zr * zi + ci;
        zr = zr2 - zi2 + cr;
        zr2 = zr * zr;
        zi2 = zi * zi;
        iter += 1;

        if zr == period_zr && zi == period_zi {
            return (max_iter, 0.0);
        }

        if iter & (period_check - 1) == 0 {
            period_zr = zr;
            period_zi = zi;
            period_check = period_check.saturating_mul(2);
        }
    }

    if iter >= max_iter {
        (max_iter, 0.0)
    } else {
        let log_zn = (zr2 + zi2).ln() * 0.5;
        let nu = (log_zn / std::f64::consts::LN_2).ln() / std::f64::consts::LN_2;
        let smooth = iter as f64 + 1.0 - nu;
        (iter, smooth.max(0.0))
    }
}

/// Generate a color from the fractal palette.
#[inline]
fn fractal_palette(
    t: f64,
    fg_color: (u8, u8, u8),
    hue_offset: f32,
    glow_intensity: f32,
    brightness_boost: f32,
) -> (u8, u8, u8) {
    let t = t as f32;
    let phase = t * 4.0 + hue_offset * 0.01;

    let base_r = fg_color.0 as f32 / 255.0;
    let base_g = fg_color.1 as f32 / 255.0;
    let base_b = fg_color.2 as f32 / 255.0;

    let r = 0.5 + 0.5 * (std::f32::consts::TAU * (phase * 1.0 + base_r * 0.5)).cos();
    let g = 0.5 + 0.5 * (std::f32::consts::TAU * (phase * 0.8 + base_g * 0.5 + 0.1)).cos();
    let b = 0.5 + 0.5 * (std::f32::consts::TAU * (phase * 0.6 + base_b * 0.5 + 0.2)).cos();

    let edge_glow = if t < 0.1 {
        (1.0 - t * 10.0) * glow_intensity
    } else {
        0.0
    };

    let r = ((r + edge_glow) * brightness_boost).clamp(0.0, 1.0);
    let g = ((g + edge_glow) * brightness_boost).clamp(0.0, 1.0);
    let b = ((b + edge_glow) * brightness_boost).clamp(0.0, 1.0);

    ((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8)
}
