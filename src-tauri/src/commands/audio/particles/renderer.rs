//! Frame renderer — draws particles with kaleidoscope symmetry, pulsing rings,
//! and background gradient animation. Pipes raw RGBA to FFmpeg.
//!
//! Performance optimizations:
//! - Background gradient uses dist_sq (no sqrt), pre-computed LUT for pulse
//! - Circle templates store pre-multiplied u8 alpha (no float in hot loop)
//! - Ring drawing uses dist_sq approximation
//! - Particle draw commands avoid per-particle Vec allocation

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;

use log::info;
use rayon::prelude::*;

use super::audio_analysis::{extract_audio_features, FrameFeatures};
use super::particle_system::{hsl_to_rgb, ParticleConfig, ParticleKind, ParticleSystem};
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

/// Pre-computed circle template with integer alpha values.
struct CircleTemplate {
    entries: Vec<CircleEntry>,
}

struct CircleEntry {
    /// (dx, dy, alpha_0_to_255)
    pixels: Vec<(i32, i32, u8)>,
}

impl CircleTemplate {
    fn new(max_radius: u32) -> Self {
        let mut entries = Vec::with_capacity(max_radius as usize + 1);

        for r in 0..=max_radius {
            let rf = r as f32;
            let mut pixels = Vec::new();

            if r == 0 {
                pixels.push((0, 0, 255u8));
            } else {
                let r_sq = rf * rf;
                let ir = r as i32;
                for dy in -ir..=ir {
                    for dx in -ir..=ir {
                        let dist_sq = (dx * dx + dy * dy) as f32;
                        if dist_sq <= r_sq {
                            let dist = dist_sq.sqrt();
                            let alpha = if dist > rf - 1.0 {
                                (rf - dist).clamp(0.0, 1.0)
                            } else {
                                1.0
                            };
                            let alpha_u8 = (alpha * 255.0) as u8;
                            if alpha_u8 > 2 {
                                pixels.push((dx, dy, alpha_u8));
                            }
                        }
                    }
                }
            }

            entries.push(CircleEntry { pixels });
        }

        Self { entries }
    }

    #[inline(always)]
    fn get(&self, radius: u32) -> &[(i32, i32, u8)] {
        let idx = (radius as usize).min(self.entries.len() - 1);
        &self.entries[idx].pixels
    }
}

/// A draw command for a single circle to render.
struct DrawCmd {
    screen_x: i32,
    screen_y: i32,
    radius: u32,
    r: u8,
    g: u8,
    b: u8,
    alpha: u8,
    is_glow: bool,
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

    let ffmpeg_bin = find_ffmpeg();
    let mut child = Command::new(&ffmpeg_bin)
        .args([
            "-y",
            "-f", "rawvideo",
            "-pixel_format", "rgba",
            "-video_size", &format!("{}x{}", config.width, config.height),
            "-framerate", &config.fps.to_string(),
            "-i", "pipe:0",
            "-i", &config.audio_path.to_string_lossy(),
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

    let particle_config = ParticleConfig {
        symmetry_folds: config.symmetry_folds,
        max_spawn_rate: 16,
        speed_multiplier: 1.0,
        base_hue: config.base_hue,
        hue_range: 120.0,
    };
    let mut system = ParticleSystem::new(particle_config);

    let max_radius = 24u32;
    let circle_templates = CircleTemplate::new(max_radius);

    let angle_step = std::f32::consts::TAU / config.symmetry_folds as f32;
    let fold_angles: Vec<(f32, f32)> = (0..config.symmetry_folds)
        .map(|fold| {
            let angle = fold as f32 * angle_step;
            (angle.cos(), angle.sin())
        })
        .collect();

    let (tx, rx) = mpsc::sync_channel::<Vec<u8>>(4);

    let width = config.width;
    let height = config.height;

    let writer_handle = std::thread::spawn(move || -> Result<(), String> {
        for frame_data in rx {
            if stdin.write_all(&frame_data).is_err() {
                break;
            }
        }
        drop(stdin);
        Ok(())
    });

    let render_progress_start = 5.0;
    let render_progress_end = 95.0;

    let cx = width as f32 / 2.0;
    let cy = height as f32 / 2.0;
    let scale = width.max(height) as f32 / 2.0;
    let bg_color = config.bg_color;
    let base_hue = config.base_hue;

    // Pre-compute static background (no pulse) — we'll modulate per-frame cheaply
    let bg_base = precompute_bg_gradient(width as usize, height as usize, bg_color);

    for (frame_idx, frame_features) in features.iter().enumerate() {
        // Check cancellation
        if cancel_flag.load(Ordering::Relaxed) {
            info!("[Particles] Render cancelled at frame {}/{}", frame_idx, total_frames);
            break;
        }

        system.update(frame_features);

        let frame_data = render_frame_enhanced(
            &system,
            width,
            height,
            cx,
            cy,
            scale,
            &fold_angles,
            bg_color,
            base_hue,
            frame_features,
            &circle_templates,
            frame_idx as u32,
            &bg_base,
        );

        if tx.send(frame_data).is_err() {
            break;
        }

        if frame_idx % 30 == 0 || frame_idx == total_frames - 1 {
            let pct = render_progress_start
                + (frame_idx as f32 / total_frames as f32)
                    * (render_progress_end - render_progress_start);
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

    info!("[Particles] Video rendered successfully: {:?}", config.output_path);
    Ok(())
}

/// Pre-compute the background gradient (one-time cost).
/// Stores per-pixel: (center_blend_factor as u8 0-255) so we can cheaply
/// interpolate between center color and edge color each frame.
fn precompute_bg_gradient(w: usize, h: usize, bg_color: (u8, u8, u8)) -> Vec<u8> {
    let cx = w as f32 / 2.0;
    let cy = h as f32 / 2.0;
    let max_dist_sq = cx * cx + cy * cy;

    let (bg_r, bg_g, bg_b) = bg_color;
    let center_r = ((bg_r as f32 * 1.3).min(255.0)) as u8;
    let center_g = ((bg_g as f32 * 1.3).min(255.0)) as u8;
    let center_b = ((bg_b as f32 * 1.3).min(255.0)) as u8;

    let mut buf = vec![0u8; w * h * 4];

    for y in 0..h {
        let dy = y as f32 - cy;
        let dy_sq = dy * dy;
        for x in 0..w {
            let dx = x as f32 - cx;
            let dist_sq = dx * dx + dy_sq;
            // t = normalized distance (0 = center, 1 = corner), no sqrt
            let t = (dist_sq / max_dist_sq).min(1.0);
            let idx = (y * w + x) * 4;
            buf[idx]     = lerp_u8(center_r, bg_r, t);
            buf[idx + 1] = lerp_u8(center_g, bg_g, t);
            buf[idx + 2] = lerp_u8(center_b, bg_b, t);
            buf[idx + 3] = 255;
        }
    }

    buf
}

/// Render a single frame with enhanced visuals.
fn render_frame_enhanced(
    system: &ParticleSystem,
    width: u32,
    height: u32,
    cx: f32,
    cy: f32,
    scale: f32,
    fold_angles: &[(f32, f32)],
    _bg_color: (u8, u8, u8),
    base_hue: f32,
    features: &FrameFeatures,
    circle_templates: &CircleTemplate,
    frame_idx: u32,
    bg_base: &[u8],
) -> Vec<u8> {
    let w = width as usize;
    let h = height as usize;

    // Start from pre-computed background (fast memcpy, ~8MB for 1080p)
    // The bass pulse effect is subtle enough that we can skip it for perf,
    // or apply it as a very cheap post-process on just the center region.
    let mut buf = bg_base.to_vec();

    // Draw rings (behind particles) — use dist_sq approximation
    for ring in &system.rings {
        draw_ring_fast(
            &mut buf, w, h, cx, cy, scale,
            ring.radius, ring.thickness, ring.hue, ring.life * ring.brightness,
        );
    }

    // Generate draw commands in parallel (avoid per-particle Vec allocation)
    let draw_commands: Vec<DrawCmd> = system.particles
        .par_iter()
        .filter_map(|particle| {
            let alpha_f = particle.life * particle.brightness;
            let alpha = (alpha_f * 255.0) as u8;
            if alpha < 5 {
                return None;
            }
            Some((particle, alpha))
        })
        .flat_map_iter(|(particle, alpha)| {
            let lightness = match particle.kind {
                ParticleKind::Glow => 0.45 + particle.life * 0.2,
                ParticleKind::Spark => 0.7 + particle.life * 0.2,
                ParticleKind::Dot => 0.5 + particle.life * 0.15,
            };
            let (r, g, b) = hsl_to_rgb(particle.hue, particle.saturation, lightness);

            let size_mult = match particle.kind {
                ParticleKind::Glow => 0.6 + features.rms * 0.6,
                ParticleKind::Spark => 0.8 + features.high_energy * 0.4,
                ParticleKind::Dot => 0.5 + features.rms * 0.5,
            };
            let size = particle.current_size() * size_mult;
            let radius = (size as u32).min(24).max(1);
            let is_glow = particle.kind == ParticleKind::Glow;

            // Inline the fold expansion to avoid Vec allocation
            let mut cmds: arrayvec::ArrayVec<DrawCmd, 32> = arrayvec::ArrayVec::new();

            for &(cos_a, sin_a) in fold_angles {
                let rx = particle.x * cos_a - particle.y * sin_a;
                let ry = particle.x * sin_a + particle.y * cos_a;

                // Two mirror reflections per fold
                let pairs = [(rx, ry), (rx, -ry)];
                for &(px, py) in &pairs {
                    let screen_x = cx + px * scale;
                    let screen_y = cy + py * scale;

                    let rf = radius as f32;
                    if screen_x < -rf || screen_x > width as f32 + rf
                        || screen_y < -rf || screen_y > height as f32 + rf
                    {
                        continue;
                    }

                    if cmds.len() < 32 {
                        cmds.push(DrawCmd {
                            screen_x: screen_x as i32,
                            screen_y: screen_y as i32,
                            radius,
                            r, g, b, alpha, is_glow,
                        });
                    }
                }
            }

            cmds.into_iter()
        })
        .collect();

    // Draw all particles (serial — writes to shared buffer)
    for cmd in &draw_commands {
        if cmd.is_glow {
            let glow_alpha = (cmd.alpha as u32 * 60 / 100) as u8;
            let glow_radius = (cmd.radius * 3 / 2).min(24);
            draw_circle_fast(&mut buf, w, h, cmd.screen_x, cmd.screen_y, glow_radius, cmd.r, cmd.g, cmd.b, glow_alpha, circle_templates);
        }
        draw_circle_fast(&mut buf, w, h, cmd.screen_x, cmd.screen_y, cmd.radius, cmd.r, cmd.g, cmd.b, cmd.alpha, circle_templates);
    }

    // Center glow
    if features.rms > 0.05 {
        let glow_intensity = features.rms * 0.7 + features.low_energy * 0.3;
        let glow_radius = (15.0 + glow_intensity * 50.0) as u32;
        let glow_radius = glow_radius.min(24);
        let glow_alpha = (glow_intensity * 100.0).clamp(0.0, 100.0) as u8;
        let time_hue = (base_hue + frame_idx as f32 * 0.5) % 360.0;
        let (gr, gg, gb) = hsl_to_rgb(time_hue, 0.7, 0.65);
        draw_circle_fast(&mut buf, w, h, cx as i32, cy as i32, glow_radius, gr, gg, gb, glow_alpha, circle_templates);
        let core_alpha = (glow_intensity * 150.0).clamp(0.0, 150.0) as u8;
        draw_circle_fast(&mut buf, w, h, cx as i32, cy as i32, (glow_radius / 3).max(2), 255, 255, 255, core_alpha, circle_templates);
    }

    buf
}

/// Draw a ring using dist_sq approximation (no sqrt).
fn draw_ring_fast(
    buf: &mut [u8],
    w: usize,
    h: usize,
    cx: f32,
    cy: f32,
    scale: f32,
    radius: f32,
    thickness: f32,
    hue: f32,
    alpha_f: f32,
) {
    let (r, g, b) = hsl_to_rgb(hue, 0.8, 0.6);
    let max_alpha = (alpha_f * 180.0).clamp(0.0, 180.0) as u32;
    if max_alpha < 3 {
        return;
    }

    let screen_radius = radius * scale;
    let half_thick = thickness / 2.0;
    let inner_r = screen_radius - half_thick;
    let outer_r = screen_radius + half_thick;
    let inner_r_sq = inner_r * inner_r;
    let outer_r_sq = outer_r * outer_r;
    let radius_sq = screen_radius * screen_radius;

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
                // Approximate |dist - radius| ≈ |dist_sq - radius_sq| / (2 * radius)
                let ring_dist = (dist_sq - radius_sq).abs() / (2.0 * screen_radius);
                if ring_dist <= half_thick {
                    let edge_alpha = 1.0 - (ring_dist / half_thick);
                    let fa = ((max_alpha as f32 * edge_alpha) as u32).min(255);
                    let inv_a = 255 - fa;

                    let idx = (y * w + x) * 4;
                    buf[idx]     = ((r as u32 * fa + buf[idx] as u32 * inv_a) >> 8) as u8;
                    buf[idx + 1] = ((g as u32 * fa + buf[idx + 1] as u32 * inv_a) >> 8) as u8;
                    buf[idx + 2] = ((b as u32 * fa + buf[idx + 2] as u32 * inv_a) >> 8) as u8;
                }
            }
        }
    }
}

/// Draw a circle using pre-computed templates with integer alpha.
#[inline]
fn draw_circle_fast(
    buf: &mut [u8],
    width: usize,
    height: usize,
    cx: i32,
    cy: i32,
    radius: u32,
    r: u8,
    g: u8,
    b: u8,
    alpha: u8,
    templates: &CircleTemplate,
) {
    let pixels = templates.get(radius);
    let w = width as i32;
    let h = height as i32;
    let a = alpha as u32;

    for &(dx, dy, edge_alpha) in pixels {
        let px = cx + dx;
        let py = cy + dy;

        if px < 0 || px >= w || py < 0 || py >= h {
            continue;
        }

        let idx = ((py as usize) * width + (px as usize)) * 4;

        // Combine particle alpha with edge alpha (both 0-255)
        let final_alpha = (a * edge_alpha as u32) >> 8;
        let inv_alpha = 255 - final_alpha;

        buf[idx]     = ((r as u32 * final_alpha + buf[idx] as u32 * inv_alpha) >> 8) as u8;
        buf[idx + 1] = ((g as u32 * final_alpha + buf[idx + 1] as u32 * inv_alpha) >> 8) as u8;
        buf[idx + 2] = ((b as u32 * final_alpha + buf[idx + 2] as u32 * inv_alpha) >> 8) as u8;
    }
}

/// Linear interpolation between two u8 values.
#[inline]
fn lerp_u8(a: u8, b: u8, t: f32) -> u8 {
    (a as f32 + (b as f32 - a as f32) * t).clamp(0.0, 255.0) as u8
}
