//! Ink diffusion renderer — Gray-Scott reaction-diffusion with audio-reactive parameters.
//!
//! The Gray-Scott model simulates two chemicals (U and V) that react and diffuse:
//!   dU/dt = Du * laplacian(U) - U*V² + f*(1-U)
//!   dV/dt = Dv * laplacian(V) + U*V² - (f+k)*V
//!
//! Audio controls:
//! - Bass energy → injects new "ink drops" (seeds V chemical)
//! - Mid energy → modulates feed rate (f) for pattern morphology
//! - High energy → modulates kill rate (k) for pattern sharpness
//! - RMS → overall simulation speed (steps per frame)
//!
//! Performance optimizations:
//! - Rayon parallel rows for simulation step
//! - Rayon parallel rows for frame rendering (bilinear upscale)
//! - Downscaled simulation grid (4x)

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;

use log::info;
use rayon::prelude::*;

use crate::commands::audio::ffmpeg::find_ffmpeg;
use crate::commands::audio::particles::audio_analysis::extract_audio_features;
use crate::core::error::AppError;
use crate::core::models::MixProgress;

/// Configuration for ink diffusion video rendering.
pub struct InkVideoConfig {
    pub audio_path: PathBuf,
    pub output_path: PathBuf,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    /// Foreground/accent color as (r, g, b) — ink color
    pub fg_color: (u8, u8, u8),
    /// Background color as (r, g, b) — "paper" color
    pub bg_color: (u8, u8, u8),
}

/// The reaction-diffusion simulation grid.
/// Uses a downscaled grid for performance, then upscales for rendering.
struct RDGrid {
    width: usize,
    height: usize,
    /// Chemical U concentration (0.0 - 1.0)
    u: Vec<f32>,
    /// Chemical V concentration (0.0 - 1.0)
    v: Vec<f32>,
    /// Scratch buffers for Laplacian computation
    u_next: Vec<f32>,
    v_next: Vec<f32>,
}

impl RDGrid {
    fn new(width: usize, height: usize) -> Self {
        let size = width * height;
        let u = vec![1.0_f32; size];
        let v = vec![0.0_f32; size];
        let u_next = vec![0.0_f32; size];
        let v_next = vec![0.0_f32; size];

        Self { width, height, u, v, u_next, v_next }
    }

    /// Seed a circular drop of V chemical at (cx, cy) with given radius.
    fn seed_drop(&mut self, cx: f32, cy: f32, radius: f32, intensity: f32) {
        let r_sq = radius * radius;
        let ri = radius as i32 + 1;
        let cx_i = cx as i32;
        let cy_i = cy as i32;

        for dy in -ri..=ri {
            let py = cy_i + dy;
            if py < 0 || py >= self.height as i32 { continue; }
            let dy_sq = (dy * dy) as f32;
            for dx in -ri..=ri {
                let px = cx_i + dx;
                if px < 0 || px >= self.width as i32 { continue; }
                let dist_sq = dx as f32 * dx as f32 + dy_sq;
                if dist_sq < r_sq {
                    let falloff = 1.0 - (dist_sq / r_sq);
                    let idx = py as usize * self.width + px as usize;
                    self.v[idx] = (self.v[idx] + intensity * falloff).min(1.0);
                    self.u[idx] = (self.u[idx] - intensity * falloff * 0.5).max(0.0);
                }
            }
        }
    }

    /// Run one simulation step with given parameters using rayon parallel rows.
    fn step(&mut self, f: f32, k: f32, du: f32, dv: f32) {
        let w = self.width;
        let h = self.height;

        // Parallel computation of next state per row
        let u_slice = &self.u;
        let v_slice = &self.v;

        let results: Vec<(usize, f32, f32)> = (0..h)
            .into_par_iter()
            .flat_map_iter(|y| {
                let y_above = if y == 0 { h - 1 } else { y - 1 };
                let y_below = if y == h - 1 { 0 } else { y + 1 };

                (0..w).map(move |x| {
                    let x_left = if x == 0 { w - 1 } else { x - 1 };
                    let x_right = if x == w - 1 { 0 } else { x + 1 };

                    let idx = y * w + x;
                    let u_val = u_slice[idx];
                    let v_val = v_slice[idx];

                    // 5-point Laplacian stencil
                    let lap_u = u_slice[y_above * w + x]
                        + u_slice[y_below * w + x]
                        + u_slice[y * w + x_left]
                        + u_slice[y * w + x_right]
                        - 4.0 * u_val;

                    let lap_v = v_slice[y_above * w + x]
                        + v_slice[y_below * w + x]
                        + v_slice[y * w + x_left]
                        + v_slice[y * w + x_right]
                        - 4.0 * v_val;

                    let uvv = u_val * v_val * v_val;

                    let u_new = (u_val + du * lap_u - uvv + f * (1.0 - u_val)).clamp(0.0, 1.0);
                    let v_new = (v_val + dv * lap_v + uvv - (f + k) * v_val).clamp(0.0, 1.0);

                    (idx, u_new, v_new)
                })
            })
            .collect();

        for (idx, u_new, v_new) in results {
            self.u_next[idx] = u_new;
            self.v_next[idx] = v_new;
        }

        // Swap buffers
        std::mem::swap(&mut self.u, &mut self.u_next);
        std::mem::swap(&mut self.v, &mut self.v_next);
    }
}

/// Render an ink diffusion video from audio.
pub fn render_ink_video<F>(
    config: &InkVideoConfig,
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
        "[Ink] {} frames to render at {}fps ({}x{})",
        total_frames, config.fps, config.width, config.height
    );

    if total_frames == 0 {
        return Err(AppError::FFmpeg("No audio frames to render".to_string()));
    }

    on_progress(MixProgress {
        percent: 5.0,
        stage: format!("准备完成，共 {} 帧", total_frames),
    });

    // Simulation grid — downscaled for performance
    let grid_scale = 4;
    let grid_w = (config.width / grid_scale) as usize;
    let grid_h = (config.height / grid_scale) as usize;
    let mut grid = RDGrid::new(grid_w, grid_h);

    // Seed initial drops in the center
    let cx = grid_w as f32 / 2.0;
    let cy = grid_h as f32 / 2.0;
    grid.seed_drop(cx, cy, 8.0, 0.9);
    grid.seed_drop(cx - 20.0, cy + 10.0, 5.0, 0.8);
    grid.seed_drop(cx + 15.0, cy - 12.0, 6.0, 0.85);

    // Run a few warmup steps so there's something visible from frame 1
    for _ in 0..80 {
        grid.step(0.037, 0.06, 0.19, 0.09);
    }

    // Render at half resolution for performance, FFmpeg upscales with lanczos
    let render_width = config.width / 2;
    let render_height = config.height / 2;
    let scale_filter = format!("scale={}:{}:flags=lanczos", config.width, config.height);

    // Start FFmpeg
    let ffmpeg_bin = find_ffmpeg();
    let mut child = Command::new(&ffmpeg_bin)
        .args([
            "-y",
            "-f", "rawvideo",
            "-pixel_format", "rgba",
            "-video_size", &format!("{}x{}", render_width, render_height),
            "-framerate", &config.fps.to_string(),
            "-i", "pipe:0",
            "-i", &config.audio_path.to_string_lossy(),
            "-vf", &scale_filter,
            "-c:v", "libx264",
            "-preset", "ultrafast",
            "-tune", "animation",
            "-crf", "22",
            "-pix_fmt", "yuv420p",
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

    let (tx, rx) = mpsc::sync_channel::<Vec<u8>>(4);

    let writer_handle = std::thread::spawn(move || -> Result<(), String> {
        for frame_data in rx {
            if stdin.write_all(&frame_data).is_err() {
                break;
            }
        }
        drop(stdin);
        Ok(())
    });

    let width = render_width;
    let height = render_height;
    let fg_color = config.fg_color;
    let bg_color = config.bg_color;

    // Smooth audio features
    let mut smooth_rms: f32 = 0.0;
    let mut smooth_low: f32 = 0.0;
    let mut smooth_mid: f32 = 0.0;
    let mut smooth_high: f32 = 0.0;

    // Deterministic pseudo-random for drop positions
    let phi: f32 = 1.618033988749895;
    let mut drop_seed: f32 = 0.0;

    // Track time since last drop to avoid over-seeding
    let mut frames_since_drop: u32 = 0;

    for (frame_idx, frame_features) in features.iter().enumerate() {
        if cancel_flag.load(Ordering::Relaxed) {
            info!("[Ink] Render cancelled at frame {}/{}", frame_idx, total_frames);
            break;
        }

        // Smooth audio features
        let smoothing = 0.12;
        smooth_rms += (frame_features.rms - smooth_rms) * smoothing;
        smooth_low += (frame_features.low_energy - smooth_low) * smoothing;
        smooth_mid += (frame_features.mid_energy - smooth_mid) * smoothing;
        smooth_high += (frame_features.high_energy - smooth_high) * smoothing;

        // Audio-reactive parameters
        let f = 0.034 + smooth_mid * 0.012;
        let k = 0.058 + smooth_high * 0.008;
        let du = 0.19 + smooth_rms * 0.02;
        let dv = 0.09 + smooth_rms * 0.01;

        // Simulation steps per frame (more when audio is active)
        let steps_per_frame = 3 + (smooth_rms * 5.0) as u32;

        // Inject new drops on bass hits
        frames_since_drop += 1;
        if frame_features.low_energy > 0.4 && frames_since_drop > 15 {
            drop_seed += phi;
            let drop_x = ((drop_seed * 127.1).sin() * 0.4 + 0.5) * grid_w as f32;
            let drop_y = ((drop_seed * 311.7).cos() * 0.4 + 0.5) * grid_h as f32;
            let drop_radius = 3.0 + frame_features.low_energy * 6.0;
            let drop_intensity = 0.6 + frame_features.low_energy * 0.4;
            grid.seed_drop(drop_x, drop_y, drop_radius, drop_intensity);
            frames_since_drop = 0;
        }

        // Also inject on strong transients
        if frame_features.rms > 0.6 && frames_since_drop > 8 {
            drop_seed += phi;
            let drop_x = ((drop_seed * 73.3).sin() * 0.35 + 0.5) * grid_w as f32;
            let drop_y = ((drop_seed * 43.7).cos() * 0.35 + 0.5) * grid_h as f32;
            let drop_radius = 2.0 + frame_features.rms * 4.0;
            grid.seed_drop(drop_x, drop_y, drop_radius, 0.7);
            frames_since_drop = 0;
        }

        // Run simulation steps
        for _ in 0..steps_per_frame {
            grid.step(f, k, du, dv);
        }

        // Render frame (upscale grid to output resolution) — parallel
        let frame_data = render_ink_frame(
            width, height,
            &grid,
            grid_w, grid_h,
            fg_color, bg_color,
            smooth_rms,
        );

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

    writer_handle
        .join()
        .map_err(|_| AppError::FFmpeg("Writer thread panicked".to_string()))?
        .map_err(|e| AppError::FFmpeg(e))?;

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

    info!("[Ink] Video rendered successfully: {:?}", config.output_path);
    Ok(())
}

/// Render the simulation grid to an output frame with bilinear upscaling.
/// Uses rayon to parallelize row rendering.
fn render_ink_frame(
    width: u32,
    height: u32,
    grid: &RDGrid,
    grid_w: usize,
    grid_h: usize,
    fg_color: (u8, u8, u8),
    bg_color: (u8, u8, u8),
    rms: f32,
) -> Vec<u8> {
    let w = width as usize;
    let h = height as usize;

    let x_scale = grid_w as f32 / width as f32;
    let y_scale = grid_h as f32 / height as f32;

    // Secondary color
    let secondary = (
        ((255 - fg_color.0) as f32 * 0.5 + fg_color.0 as f32 * 0.5) as u8,
        ((255 - fg_color.1) as f32 * 0.3 + fg_color.1 as f32 * 0.7) as u8,
        fg_color.2,
    );

    let pulse = 1.0 + rms * 0.15;

    let grid_u = &grid.u;
    let grid_v = &grid.v;

    // Parallel row rendering
    let buf: Vec<u8> = (0..h)
        .into_par_iter()
        .flat_map_iter(move |py| {
            let gy = py as f32 * y_scale;
            let gy0 = (gy as usize).min(grid_h - 1);
            let gy1 = (gy0 + 1).min(grid_h - 1);
            let fy = gy - gy0 as f32;

            (0..w).flat_map(move |px| {
                let gx = px as f32 * x_scale;
                let gx0 = (gx as usize).min(grid_w - 1);
                let gx1 = (gx0 + 1).min(grid_w - 1);
                let fx = gx - gx0 as f32;

                // Bilinear interpolation of V chemical
                let v00 = grid_v[gy0 * grid_w + gx0];
                let v10 = grid_v[gy0 * grid_w + gx1];
                let v01 = grid_v[gy1 * grid_w + gx0];
                let v11 = grid_v[gy1 * grid_w + gx1];

                let v_top = v00 + (v10 - v00) * fx;
                let v_bot = v01 + (v11 - v01) * fx;
                let v_val = v_top + (v_bot - v_top) * fy;

                // Bilinear interpolation of U chemical
                let u00 = grid_u[gy0 * grid_w + gx0];
                let u10 = grid_u[gy0 * grid_w + gx1];
                let u01 = grid_u[gy1 * grid_w + gx0];
                let u11 = grid_u[gy1 * grid_w + gx1];

                let u_top = u00 + (u10 - u00) * fx;
                let u_bot = u01 + (u11 - u01) * fx;
                let u_val = u_top + (u_bot - u_top) * fy;

                let ink_amount = v_val.clamp(0.0, 1.0);
                let depth = (1.0 - u_val).clamp(0.0, 1.0);

                if ink_amount < 0.01 {
                    [bg_color.0, bg_color.1, bg_color.2, 255u8]
                } else {
                    let ink_r = lerp_f32(fg_color.0 as f32, secondary.0 as f32, depth * 0.6);
                    let ink_g = lerp_f32(fg_color.1 as f32, secondary.1 as f32, depth * 0.6);
                    let ink_b = lerp_f32(fg_color.2 as f32, secondary.2 as f32, depth * 0.6);

                    let r = lerp_f32(bg_color.0 as f32, ink_r * pulse, ink_amount);
                    let g = lerp_f32(bg_color.1 as f32, ink_g * pulse, ink_amount);
                    let b = lerp_f32(bg_color.2 as f32, ink_b * pulse, ink_amount);

                    [
                        r.clamp(0.0, 255.0) as u8,
                        g.clamp(0.0, 255.0) as u8,
                        b.clamp(0.0, 255.0) as u8,
                        255u8,
                    ]
                }
                .into_iter()
            })
        })
        .collect();

    buf
}

/// Linear interpolation between two f32 values.
#[inline]
fn lerp_f32(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}
