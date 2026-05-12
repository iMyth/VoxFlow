//! Vinyl disc renderer — enhanced design with audio-reactive visuals.
//!
//! Performance optimizations:
//! - Rayon parallel row rendering for expensive layers (iridescent edge, disc blit)
//! - Pre-computed background (no per-frame recomputation)
//! - Reduced EQ bar count and simplified glow
//! - Eliminated redundant sqrt calls using dist_sq comparisons
//! - Fast atan2 and sin approximations for shimmer effects

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;

use log::info;
use rayon::prelude::*;

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

/// Pre-rendered cover image as a circular texture (RGBA pixels, square).
struct CoverTexture {
    pixels: Vec<u8>,
    size: u32,
}

impl CoverTexture {
    fn load(path: &PathBuf, target_size: u32) -> Result<Self, String> {
        let img = image::open(path)
            .map_err(|e| format!("Failed to load cover image: {}", e))?;

        let resized = img.resize_to_fill(target_size, target_size, image::imageops::FilterType::Lanczos3);
        let rgba = resized.to_rgba8();

        let size = target_size;
        let center = size as f32 / 2.0;
        let radius = center;
        let mut pixels = vec![0u8; (size * size * 4) as usize];

        for y in 0..size {
            for x in 0..size {
                let dx = x as f32 - center + 0.5;
                let dy = y as f32 - center + 0.5;
                let dist = (dx * dx + dy * dy).sqrt();

                let idx = ((y * size + x) * 4) as usize;
                let src = rgba.get_pixel(x, y);

                if dist <= radius - 1.0 {
                    pixels[idx] = src[0];
                    pixels[idx + 1] = src[1];
                    pixels[idx + 2] = src[2];
                    pixels[idx + 3] = src[3];
                } else if dist <= radius {
                    let alpha = (radius - dist).clamp(0.0, 1.0);
                    pixels[idx] = src[0];
                    pixels[idx + 1] = src[1];
                    pixels[idx + 2] = src[2];
                    pixels[idx + 3] = (src[3] as f32 * alpha) as u8;
                }
            }
        }

        Ok(Self { pixels, size })
    }

    fn default_vinyl(target_size: u32, fg_color: (u8, u8, u8)) -> Self {
        let size = target_size;
        let center = size as f32 / 2.0;
        let radius = center;
        let mut pixels = vec![0u8; (size * size * 4) as usize];

        for y in 0..size {
            for x in 0..size {
                let dx = x as f32 - center + 0.5;
                let dy = y as f32 - center + 0.5;
                let dist = (dx * dx + dy * dy).sqrt();

                let idx = ((y * size + x) * 4) as usize;

                if dist <= radius - 1.0 {
                    let norm_dist = dist / radius;

                    if norm_dist < 0.28 {
                        let label_t = norm_dist / 0.28;
                        let brightness = 0.7 - label_t * 0.2;
                        let r = (fg_color.0 as f32 * brightness).min(255.0) as u8;
                        let g = (fg_color.1 as f32 * brightness).min(255.0) as u8;
                        let b = (fg_color.2 as f32 * brightness).min(255.0) as u8;
                        pixels[idx] = r;
                        pixels[idx + 1] = g;
                        pixels[idx + 2] = b;
                        pixels[idx + 3] = 255;
                    } else {
                        let groove_freq = dist * 1.2;
                        let groove = ((groove_freq % 2.0) - 1.0).abs();
                        let micro_groove = ((dist * 4.5) % 1.0 - 0.5).abs() * 2.0;
                        let base = 22.0 + groove * 12.0 + micro_groove * 4.0;
                        let radial = (norm_dist - 0.28) / 0.72;
                        let brightness = base + radial * 6.0;

                        pixels[idx] = brightness as u8;
                        pixels[idx + 1] = brightness as u8;
                        pixels[idx + 2] = (brightness + 1.0) as u8;
                        pixels[idx + 3] = 255;
                    }
                } else if dist <= radius {
                    let alpha = (radius - dist).clamp(0.0, 1.0);
                    pixels[idx] = 25;
                    pixels[idx + 1] = 25;
                    pixels[idx + 2] = 27;
                    pixels[idx + 3] = (255.0 * alpha) as u8;
                }
            }
        }

        Self { pixels, size }
    }
}

/// Floating bokeh particle for background ambiance.
struct BokehParticle {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    radius: f32,
    alpha: f32,
    hue_offset: f32,
}

fn generate_bokeh_particles(width: u32, height: u32, count: usize) -> Vec<BokehParticle> {
    let mut particles = Vec::with_capacity(count);
    let phi = 1.618033988749895_f32;
    for i in 0..count {
        let t = i as f32 * phi;
        let x = ((t * 127.1).sin() * 0.5 + 0.5) * width as f32;
        let y = ((t * 311.7).cos() * 0.5 + 0.5) * height as f32;
        let vx = (t * 73.3).sin() * 0.3;
        let vy = (t * 43.7).cos() * 0.2 - 0.1;
        let radius = 3.0 + ((t * 17.1).sin().abs()) * 12.0;
        let alpha = 0.03 + ((t * 7.3).cos().abs()) * 0.06;
        let hue_offset = (t * 53.1) % 60.0 - 30.0;
        particles.push(BokehParticle { x, y, vx, vy, radius, alpha, hue_offset });
    }
    particles
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

    // Render at half resolution for performance, FFmpeg upscales with lanczos
    let render_width = config.width / 2;
    let render_height = config.height / 2;

    // Prepare cover texture (based on render resolution)
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

    on_progress(MixProgress {
        percent: 5.0,
        stage: format!("准备完成，共 {} 帧", total_frames),
    });

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
            "-tune", "stillimage",
            "-crf", "23",
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

    // Pre-compute static background gradient
    let bg_frame = precompute_vinyl_bg(width as usize, height as usize, bg_color);

    let rotation_speed = std::f32::consts::TAU / (5.0 * config.fps as f32);

    let mut smooth_rms: f32 = 0.0;
    let mut smooth_low: f32 = 0.0;
    let mut smooth_mid: f32 = 0.0;
    let mut smooth_high: f32 = 0.0;

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

        let frame_data = render_vinyl_frame(
            width,
            height,
            &cover_texture,
            angle,
            &smoothed,
            frame_features,
            fg_color,
            bg_color,
            frame_idx as u32,
            &bg_frame,
            &bokeh_particles,
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

    info!("[Vinyl] Video rendered successfully: {:?}", config.output_path);
    Ok(())
}

/// Pre-compute the background with a rich radial gradient and subtle vignette.
fn precompute_vinyl_bg(w: usize, h: usize, bg_color: (u8, u8, u8)) -> Vec<u8> {
    let cx = w as f32 / 2.0;
    let cy = h as f32 / 2.0;
    let max_dist = (cx * cx + cy * cy).sqrt();
    let (bg_r, bg_g, bg_b) = bg_color;

    let center_r = ((bg_r as f32 * 1.4).min(255.0)) as u8;
    let center_g = ((bg_g as f32 * 1.4).min(255.0)) as u8;
    let center_b = ((bg_b as f32 * 1.4).min(255.0)) as u8;

    let edge_r = (bg_r as f32 * 0.4) as u8;
    let edge_g = (bg_g as f32 * 0.4) as u8;
    let edge_b = (bg_b as f32 * 0.4) as u8;

    let mut buf = vec![0u8; w * h * 4];
    for y in 0..h {
        let dy = y as f32 - cy;
        let dy_sq = dy * dy;
        for x in 0..w {
            let dx = x as f32 - cx;
            let dist = (dx * dx + dy_sq).sqrt();
            let t = (dist / max_dist).min(1.0);
            let vignette = t * t * (3.0 - 2.0 * t);

            let idx = (y * w + x) * 4;
            buf[idx] = lerp_u8(center_r, edge_r, vignette);
            buf[idx + 1] = lerp_u8(center_g, edge_g, vignette);
            buf[idx + 2] = lerp_u8(center_b, edge_b, vignette);
            buf[idx + 3] = 255;
        }
    }
    buf
}

/// Render a single vinyl frame with all visual layers.
/// Key optimization: the disc blit + iridescent edge are rendered in a single
/// parallel pass over the disc bounding box, avoiding multiple passes over the same pixels.
fn render_vinyl_frame(
    width: u32,
    height: u32,
    cover: &CoverTexture,
    angle: f32,
    smoothed: &FrameFeatures,
    raw: &FrameFeatures,
    fg_color: (u8, u8, u8),
    _bg_color: (u8, u8, u8),
    frame_idx: u32,
    bg_frame: &[u8],
    bokeh_particles: &[BokehParticle],
) -> Vec<u8> {
    let w = width as usize;
    let h = height as usize;

    // Start from pre-computed background
    let mut buf = bg_frame.to_vec();

    let cx = width as f32 / 2.0;
    let cy = height as f32 / 2.0;

    // === Layer 1: Background bokeh particles ===
    draw_bokeh_layer(&mut buf, w, h, bokeh_particles, frame_idx, fg_color, smoothed.rms);

    // === Layer 2: Outer glow rings ===
    let disc_radius = cover.size as f32 / 2.0;

    let bass_ring_radius = disc_radius + 20.0 + smoothed.low_energy * 25.0;
    let bass_ring_width = 3.0 + smoothed.low_energy * 8.0;
    let bass_ring_alpha = (smoothed.low_energy * 100.0).clamp(0.0, 100.0) as u8;
    if bass_ring_alpha > 5 {
        draw_glow_ring(&mut buf, w, h, cx, cy, bass_ring_radius, bass_ring_width, fg_color, bass_ring_alpha);
    }

    let mid_ring_radius = disc_radius + 12.0 + smoothed.mid_energy * 12.0;
    let mid_ring_width = 2.0 + smoothed.mid_energy * 5.0;
    let mid_ring_alpha = (smoothed.mid_energy * 70.0).clamp(0.0, 70.0) as u8;
    if mid_ring_alpha > 5 {
        let mid_color = (
            lerp_u8(fg_color.0, 255, 0.3),
            lerp_u8(fg_color.1, 255, 0.3),
            lerp_u8(fg_color.2, 255, 0.3),
        );
        draw_glow_ring(&mut buf, w, h, cx, cy, mid_ring_radius, mid_ring_width, mid_color, mid_ring_alpha);
    }

    // === Layer 3: EQ spectrum bars (reduced count for performance) ===
    draw_eq_bars_v2(
        &mut buf, w, h, cx, cy,
        disc_radius, smoothed, raw, fg_color, frame_idx,
    );

    // === Layer 4: Disc + iridescent edge (combined parallel pass) ===
    let pulse_scale = 1.0 + smoothed.low_energy * 0.02;
    blit_disc_with_edge(
        &mut buf, w, h,
        cx, cy,
        cover,
        angle,
        pulse_scale,
        frame_idx,
    );

    // === Layer 5: Center spindle hole ===
    let hole_radius = (cover.size as f32 * 0.025) as i32;
    draw_spindle_hole(&mut buf, w, h, cx, cy, hole_radius, _bg_color);

    // === Layer 6: Specular highlight ===
    let highlight_angle = angle * 0.05;
    let hl_offset_x = highlight_angle.cos() * disc_radius * 0.15;
    let hl_offset_y = highlight_angle.sin() * disc_radius * 0.15;
    draw_specular_highlight(
        &mut buf, w, h,
        cx - disc_radius * 0.2 + hl_offset_x,
        cy - disc_radius * 0.2 + hl_offset_y,
        disc_radius * 0.35,
        disc_radius * pulse_scale,
        cx, cy,
    );

    // === Layer 7: Inner glow ring ===
    let inner_ring_radius = disc_radius * pulse_scale * 0.92;
    let inner_ring_alpha = (smoothed.high_energy * 50.0).clamp(0.0, 50.0) as u8;
    if inner_ring_alpha > 3 {
        draw_glow_ring(&mut buf, w, h, cx, cy, inner_ring_radius, 2.0, (255, 255, 255), inner_ring_alpha);
    }

    buf
}

/// Draw floating bokeh particles in the background.
fn draw_bokeh_layer(
    buf: &mut [u8],
    w: usize,
    h: usize,
    particles: &[BokehParticle],
    frame_idx: u32,
    fg_color: (u8, u8, u8),
    rms: f32,
) {
    let time = frame_idx as f32;

    for p in particles {
        let px = (p.x + p.vx * time) % w as f32;
        let py = ((p.y + p.vy * time) % h as f32 + h as f32) % h as f32;

        let pulse = 1.0 + rms * 0.5;
        let alpha = (p.alpha * pulse * 255.0).clamp(0.0, 40.0) as u8;
        if alpha < 3 {
            continue;
        }

        let r = lerp_u8(fg_color.0, 255, (p.hue_offset.abs() / 60.0).min(0.5));
        let g = lerp_u8(fg_color.1, 255, (p.hue_offset.abs() / 80.0).min(0.4));
        let b = lerp_u8(fg_color.2, 255, (p.hue_offset.abs() / 70.0).min(0.45));

        let radius = p.radius;
        let r_sq = radius * radius;
        let px_i = px as i32;
        let py_i = py as i32;
        let ri = radius as i32 + 1;

        let y_start = (py_i - ri).max(0) as usize;
        let y_end = (py_i + ri + 1).min(h as i32) as usize;
        let x_start = (px_i - ri).max(0) as usize;
        let x_end = (px_i + ri + 1).min(w as i32) as usize;

        for y in y_start..y_end {
            let dy = y as f32 - py;
            let dy_sq = dy * dy;
            for x in x_start..x_end {
                let dx = x as f32 - px;
                let dist_sq = dx * dx + dy_sq;
                if dist_sq < r_sq {
                    let t = dist_sq / r_sq;
                    let falloff = (1.0 - t) * (1.0 - t);
                    let a = (alpha as f32 * falloff) as u8;
                    if a > 1 {
                        let idx = (y * w + x) * 4;
                        alpha_blend(buf, idx, r, g, b, a);
                    }
                }
            }
        }
    }
}

/// Draw EQ bars — reduced bar count for performance.
fn draw_eq_bars_v2(
    buf: &mut [u8],
    w: usize,
    h: usize,
    cx: f32,
    cy: f32,
    disc_radius: f32,
    smoothed: &FrameFeatures,
    raw: &FrameFeatures,
    fg_color: (u8, u8, u8),
    frame_idx: u32,
) {
    let num_bars: u32 = 32; // Reduced from 48 for performance
    let bar_inner_radius = disc_radius + 28.0;
    let bar_max_height = disc_radius * 0.55;
    let bar_width = 5.0; // Slightly wider to compensate for fewer bars

    let eq_rotation = frame_idx as f32 * 0.002;

    for i in 0..num_bars {
        let bar_angle = (i as f32 / num_bars as f32) * std::f32::consts::TAU + eq_rotation;

        let band_t = (i as f32 / num_bars as f32) * 3.0;
        let energy = if band_t < 1.0 {
            lerp_f32(smoothed.low_energy, smoothed.mid_energy, band_t)
        } else if band_t < 2.0 {
            lerp_f32(smoothed.mid_energy, smoothed.high_energy, band_t - 1.0)
        } else {
            lerp_f32(smoothed.high_energy, smoothed.low_energy, band_t - 2.0)
        };

        let transient = raw.rms * 0.3;
        let total_energy = (energy * 0.7 + transient).clamp(0.0, 1.0);

        let bar_height = bar_max_height * total_energy * (0.4 + smoothed.rms * 0.6);
        if bar_height < 3.0 {
            continue;
        }

        let cos_a = bar_angle.cos();
        let sin_a = bar_angle.sin();

        // Draw bar as a line with width (step by 2 pixels for speed)
        let steps = bar_height as u32;
        let half_width = bar_width / 2.0;

        let mut step = 0u32;
        while step < steps {
            let r = bar_inner_radius + step as f32;
            let base_px = cx + r * cos_a;
            let base_py = cy + r * sin_a;

            let t = step as f32 / steps as f32;
            let color_t = t * t;
            let bar_r = lerp_u8(fg_color.0, 255, color_t * 0.7);
            let bar_g = lerp_u8(fg_color.1, 255, color_t * 0.7);
            let bar_b = lerp_u8(fg_color.2, 255, color_t * 0.7);

            let alpha_t = 1.0 - t * t * 0.5;
            let bar_alpha = (220.0 * alpha_t) as u8;

            let perp_x = -sin_a;
            let perp_y = cos_a;

            // Draw width with fewer steps (3 pixels wide instead of per-pixel)
            let width_steps = 3i32;
            for wi in -width_steps..=width_steps {
                let wt = wi as f32 / (width_steps as f32 + 1.0);
                let px = (base_px + perp_x * wt * half_width) as i32;
                let py = (base_py + perp_y * wt * half_width) as i32;

                if px >= 0 && px < w as i32 && py >= 0 && py < h as i32 {
                    let edge_dist = (wt.abs() * 2.0 - 0.5).max(0.0) * 2.0;
                    let edge_alpha = ((1.0 - edge_dist) * bar_alpha as f32) as u8;
                    if edge_alpha > 2 {
                        let idx = (py as usize * w + px as usize) * 4;
                        alpha_blend(buf, idx, bar_r, bar_g, bar_b, edge_alpha);
                    }
                }
            }

            step += 1;
        }

        // Glow at bar tip (simplified)
        if bar_height > 10.0 {
            let tip_r = bar_inner_radius + bar_height;
            let tip_x = cx + tip_r * cos_a;
            let tip_y = cy + tip_r * sin_a;
            let glow_radius = 3.0 + total_energy * 2.0;
            let glow_alpha = (total_energy * 50.0).clamp(0.0, 50.0) as u8;
            draw_soft_dot(buf, w, h, tip_x, tip_y, glow_radius, 255, 255, 255, glow_alpha);
        }
    }
}

/// Combined disc blit + iridescent edge in a single pass using rayon.
/// This is the most expensive operation — we parallelize it by rows.
fn blit_disc_with_edge(
    buf: &mut [u8],
    buf_w: usize,
    buf_h: usize,
    cx: f32,
    cy: f32,
    texture: &CoverTexture,
    angle: f32,
    scale: f32,
    frame_idx: u32,
) {
    let tex_size = texture.size as f32;
    let tex_center = tex_size / 2.0;
    let radius = tex_center * scale;
    let radius_sq = radius * radius;
    let inner_radius = radius - 1.0;
    let inner_radius_sq = inner_radius * inner_radius;

    // Iridescent edge parameters
    let edge_width = 6.0;
    let edge_inner_r = radius - edge_width;
    let edge_inner_r_sq = edge_inner_r * edge_inner_r;
    let edge_outer_r = radius;
    let edge_outer_r_sq = edge_outer_r * edge_outer_r;
    let inv_range_sq = 1.0 / (edge_outer_r_sq - edge_inner_r_sq).max(1.0);

    let cos_a = angle.cos();
    let sin_a = angle.sin();
    let inv_scale = 1.0 / scale;

    let r_i32 = radius as i32 + 1;
    let start_x = ((cx as i32) - r_i32).max(0) as usize;
    let end_x = ((cx as i32) + r_i32 + 1).min(buf_w as i32) as usize;
    let start_y = ((cy as i32) - r_i32).max(0) as usize;
    let end_y = ((cy as i32) + r_i32 + 1).min(buf_h as i32) as usize;

    let tex_size_i32 = texture.size as i32;
    let tex_stride = texture.size as usize;
    let tex_pixels = &texture.pixels;

    let time = frame_idx as f32 * 0.03;
    let angle_offset = angle * 0.5 + time;

    // Process rows in parallel, collecting overlay operations
    // Each row produces a vec of (x, r, g, b, a) operations
    let row_ops: Vec<Vec<(usize, u8, u8, u8, u8)>> = (start_y..end_y)
        .into_par_iter()
        .map(|y| {
            let dy = y as f32 - cy;
            let dy_sq = dy * dy;
            let unscaled_dy = dy * inv_scale;
            let dy_rot_x = unscaled_dy * sin_a + tex_center;
            let dy_rot_y = unscaled_dy * cos_a + tex_center;

            let mut ops: Vec<(usize, u8, u8, u8, u8)> = Vec::new();

            for x in start_x..end_x {
                let dx = x as f32 - cx;
                let dist_sq = dx * dx + dy_sq;

                if dist_sq > radius_sq {
                    continue;
                }

                let buf_idx = (y * buf_w + x) * 4;

                // Disc texture
                let unscaled_dx = dx * inv_scale;
                let src_x = unscaled_dx * cos_a + dy_rot_x;
                let src_y = -unscaled_dx * sin_a + dy_rot_y;

                let sx = src_x as i32;
                let sy = src_y as i32;

                if sx >= 0 && sx < tex_size_i32 && sy >= 0 && sy < tex_size_i32 {
                    let tex_idx = (sy as usize * tex_stride + sx as usize) * 4;
                    let ta = tex_pixels[tex_idx + 3];

                    if ta > 0 {
                        if dist_sq > inner_radius_sq {
                            let edge_dist = radius - dist_sq.sqrt();
                            if edge_dist > 0.0 {
                                let edge_alpha = (ta as f32 * edge_dist) as u8;
                                ops.push((buf_idx, tex_pixels[tex_idx], tex_pixels[tex_idx + 1], tex_pixels[tex_idx + 2], edge_alpha));
                            }
                        } else if ta == 255 {
                            // Opaque pixel — direct write (encoded as alpha=255 special case)
                            ops.push((buf_idx, tex_pixels[tex_idx], tex_pixels[tex_idx + 1], tex_pixels[tex_idx + 2], 255));
                        } else {
                            ops.push((buf_idx, tex_pixels[tex_idx], tex_pixels[tex_idx + 1], tex_pixels[tex_idx + 2], ta));
                        }
                    }
                }

                // Iridescent edge shimmer (only in the edge band)
                if dist_sq >= edge_inner_r_sq && dist_sq <= edge_outer_r_sq {
                    let pixel_angle = fast_atan2(dy, dx);
                    let hue_raw = (pixel_angle + angle_offset) * 57.3;
                    let hue = ((hue_raw % 360.0) + 360.0) % 360.0;

                    let (ir, ig, ib) = hsl_to_rgb_simple(hue, 0.7, 0.6);

                    let edge_t = (dist_sq - edge_inner_r_sq) * inv_range_sq;
                    let fade = (edge_t * (1.0 - edge_t) * 4.0).clamp(0.0, 1.0);

                    let shimmer_input = pixel_angle * 3.0 + time * 2.0;
                    let shimmer = fast_sin(shimmer_input) * 0.5 + 0.5;
                    let alpha = (fade * shimmer * 35.0) as u8;

                    if alpha > 2 {
                        ops.push((buf_idx, ir, ig, ib, alpha));
                    }
                }
            }

            ops
        })
        .collect();

    // Apply all operations to the buffer (serial, but the computation was parallel)
    for row_op in &row_ops {
        for &(idx, r, g, b, a) in row_op {
            if a == 255 {
                buf[idx] = r;
                buf[idx + 1] = g;
                buf[idx + 2] = b;
            } else {
                alpha_blend(buf, idx, r, g, b, a);
            }
        }
    }
}

/// Draw the center spindle hole.
fn draw_spindle_hole(
    buf: &mut [u8],
    w: usize,
    h: usize,
    cx: f32,
    cy: f32,
    hole_radius: i32,
    bg_color: (u8, u8, u8),
) {
    let (bg_r, bg_g, bg_b) = bg_color;
    let hole_r_sq = (hole_radius * hole_radius) as f32;
    let hole_inner_sq = ((hole_radius as f32 - 1.5) * (hole_radius as f32 - 1.5)).max(0.0);

    for dy in -hole_radius..=hole_radius {
        for dx in -hole_radius..=hole_radius {
            let dist_sq = (dx * dx + dy * dy) as f32;
            if dist_sq <= hole_r_sq {
                let px = cx as i32 + dx;
                let py = cy as i32 + dy;
                if px >= 0 && px < w as i32 && py >= 0 && py < h as i32 {
                    let idx = (py as usize * w + px as usize) * 4;
                    let a = if dist_sq > hole_inner_sq {
                        ((hole_r_sq - dist_sq) / (hole_r_sq - hole_inner_sq) * 255.0).min(255.0) as u8
                    } else {
                        255
                    };
                    alpha_blend(buf, idx, bg_r, bg_g, bg_b, a);
                }
            }
        }
    }

    // Metallic ring around hole
    let ring_outer = hole_radius + 2;
    let ring_outer_sq = (ring_outer * ring_outer) as f32;
    for dy in -ring_outer..=ring_outer {
        for dx in -ring_outer..=ring_outer {
            let dist_sq = (dx * dx + dy * dy) as f32;
            if dist_sq > hole_r_sq && dist_sq <= ring_outer_sq {
                let px = cx as i32 + dx;
                let py = cy as i32 + dy;
                if px >= 0 && px < w as i32 && py >= 0 && py < h as i32 {
                    let idx = (py as usize * w + px as usize) * 4;
                    let t = (dist_sq - hole_r_sq) / (ring_outer_sq - hole_r_sq);
                    let brightness = 80 + (t * 60.0) as u8;
                    alpha_blend(buf, idx, brightness, brightness, brightness, 180);
                }
            }
        }
    }
}

/// Draw a dynamic specular highlight on the disc surface.
fn draw_specular_highlight(
    buf: &mut [u8],
    w: usize,
    h: usize,
    highlight_cx: f32,
    highlight_cy: f32,
    highlight_radius: f32,
    disc_radius: f32,
    disc_cx: f32,
    disc_cy: f32,
) {
    let hl_r_sq = highlight_radius * highlight_radius;
    let disc_r_sq = disc_radius * disc_radius;

    let hl_y_start = ((highlight_cy - highlight_radius) as i32).max(0) as usize;
    let hl_y_end = ((highlight_cy + highlight_radius) as i32 + 1).min(h as i32) as usize;
    let hl_x_start = ((highlight_cx - highlight_radius) as i32).max(0) as usize;
    let hl_x_end = ((highlight_cx + highlight_radius) as i32 + 1).min(w as i32) as usize;

    for y in hl_y_start..hl_y_end {
        let dy = y as f32 - highlight_cy;
        let dy_sq = dy * dy;
        for x in hl_x_start..hl_x_end {
            let dx = x as f32 - highlight_cx;
            let dist_sq = dx * dx + dy_sq;
            if dist_sq < hl_r_sq {
                let disc_dx = x as f32 - disc_cx;
                let disc_dy = y as f32 - disc_cy;
                let disc_dist_sq = disc_dx * disc_dx + disc_dy * disc_dy;
                if disc_dist_sq < disc_r_sq {
                    let t = 1.0 - (dist_sq / hl_r_sq);
                    let alpha = (t * t * t * 45.0) as u8;
                    if alpha > 1 {
                        let idx = (y * w + x) * 4;
                        alpha_blend(buf, idx, 255, 255, 255, alpha);
                    }
                }
            }
        }
    }
}

/// Draw a soft glow ring.
fn draw_glow_ring(
    buf: &mut [u8],
    w: usize,
    h: usize,
    cx: f32,
    cy: f32,
    radius: f32,
    width: f32,
    color: (u8, u8, u8),
    max_alpha: u8,
) {
    let half_w = width / 2.0;
    let inner_r = radius - half_w;
    let outer_r = radius + half_w;
    let inner_r_sq = inner_r * inner_r;
    let outer_r_sq = outer_r * outer_r;
    let radius_sq = radius * radius;

    let y_start = ((cy - outer_r) as i32).max(0) as usize;
    let y_end = ((cy + outer_r) as i32 + 1).min(h as i32) as usize;
    let x_start = ((cx - outer_r) as i32).max(0) as usize;
    let x_end = ((cx + outer_r) as i32 + 1).min(w as i32) as usize;

    for y in y_start..y_end {
        let dy = y as f32 - cy;
        let dy_sq = dy * dy;
        for x in x_start..x_end {
            let dx = x as f32 - cx;
            let dist_sq = dx * dx + dy_sq;

            if dist_sq >= inner_r_sq && dist_sq <= outer_r_sq {
                let ring_dist_approx = (dist_sq - radius_sq).abs() / (2.0 * radius);
                if ring_dist_approx <= half_w {
                    let t = 1.0 - (ring_dist_approx / half_w);
                    let alpha = (t * t * max_alpha as f32) as u8;
                    if alpha > 2 {
                        let idx = (y * w + x) * 4;
                        alpha_blend(buf, idx, color.0, color.1, color.2, alpha);
                    }
                }
            }
        }
    }
}

/// Draw a soft circular dot.
fn draw_soft_dot(
    buf: &mut [u8],
    w: usize,
    h: usize,
    cx: f32,
    cy: f32,
    radius: f32,
    r: u8,
    g: u8,
    b: u8,
    max_alpha: u8,
) {
    let r_sq = radius * radius;
    let ri = radius as i32 + 1;
    let y_start = ((cy as i32) - ri).max(0) as usize;
    let y_end = ((cy as i32) + ri + 1).min(h as i32) as usize;
    let x_start = ((cx as i32) - ri).max(0) as usize;
    let x_end = ((cx as i32) + ri + 1).min(w as i32) as usize;

    for y in y_start..y_end {
        let dy = y as f32 - cy;
        let dy_sq = dy * dy;
        for x in x_start..x_end {
            let dx = x as f32 - cx;
            let dist_sq = dx * dx + dy_sq;
            if dist_sq < r_sq {
                let t = dist_sq / r_sq;
                let falloff = (1.0 - t) * (1.0 - t);
                let alpha = (falloff * max_alpha as f32) as u8;
                if alpha > 1 {
                    let idx = (y * w + x) * 4;
                    alpha_blend(buf, idx, r, g, b, alpha);
                }
            }
        }
    }
}

/// Simple HSL to RGB conversion.
fn hsl_to_rgb_simple(h: f32, s: f32, l: f32) -> (u8, u8, u8) {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let h_prime = h / 60.0;
    let x = c * (1.0 - (h_prime % 2.0 - 1.0).abs());
    let m = l - c / 2.0;

    let (r1, g1, b1) = if h_prime < 1.0 {
        (c, x, 0.0)
    } else if h_prime < 2.0 {
        (x, c, 0.0)
    } else if h_prime < 3.0 {
        (0.0, c, x)
    } else if h_prime < 4.0 {
        (0.0, x, c)
    } else if h_prime < 5.0 {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };

    (
        ((r1 + m) * 255.0).clamp(0.0, 255.0) as u8,
        ((g1 + m) * 255.0).clamp(0.0, 255.0) as u8,
        ((b1 + m) * 255.0).clamp(0.0, 255.0) as u8,
    )
}

/// Alpha-blend a color onto the buffer at the given index.
#[inline]
fn alpha_blend(buf: &mut [u8], idx: usize, r: u8, g: u8, b: u8, alpha: u8) {
    let a = alpha as u32;
    let inv_a = 255 - a;
    buf[idx]     = ((r as u32 * a + buf[idx] as u32 * inv_a) >> 8) as u8;
    buf[idx + 1] = ((g as u32 * a + buf[idx + 1] as u32 * inv_a) >> 8) as u8;
    buf[idx + 2] = ((b as u32 * a + buf[idx + 2] as u32 * inv_a) >> 8) as u8;
}

/// Linear interpolation between two u8 values.
#[inline]
fn lerp_u8(a: u8, b: u8, t: f32) -> u8 {
    (a as f32 + (b as f32 - a as f32) * t).clamp(0.0, 255.0) as u8
}

/// Linear interpolation between two f32 values.
#[inline]
fn lerp_f32(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Fast atan2 approximation (max error ~0.01 rad).
#[inline]
fn fast_atan2(y: f32, x: f32) -> f32 {
    if x == 0.0 && y == 0.0 {
        return 0.0;
    }

    let ax = x.abs();
    let ay = y.abs();

    let (a, offset) = if ax >= ay {
        (ay / ax, 0.0_f32)
    } else {
        (ax / ay, std::f32::consts::FRAC_PI_2)
    };

    let s = a * (std::f32::consts::FRAC_PI_4 + 0.273 * (1.0 - a));
    let mut r = if ax >= ay { s } else { offset - s };

    if x < 0.0 {
        r = std::f32::consts::PI - r;
    }
    if y < 0.0 {
        r = -r;
    }
    r
}

/// Fast sine approximation using parabolic method.
#[inline]
fn fast_sin(x: f32) -> f32 {
    let mut x = x % std::f32::consts::TAU;
    if x > std::f32::consts::PI {
        x -= std::f32::consts::TAU;
    } else if x < -std::f32::consts::PI {
        x += std::f32::consts::TAU;
    }

    let b = 4.0 / std::f32::consts::PI;
    let c = -4.0 / (std::f32::consts::PI * std::f32::consts::PI);
    let y = b * x + c * x * x.abs();

    let p = 0.225;
    p * (y * y.abs() - y) + y
}
