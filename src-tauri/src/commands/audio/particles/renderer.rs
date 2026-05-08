//! Frame renderer — draws particles with kaleidoscope symmetry using direct pixel
//! manipulation (no path rasterization), pipes raw RGBA to FFmpeg via a multi-threaded
//! pipeline: update (serial) → render (parallel) → encode (piped).

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc;

use log::info;
use rayon::prelude::*;

use super::audio_analysis::{extract_audio_features, FrameFeatures};
use super::particle_system::{hsl_to_rgb, ParticleConfig, ParticleSystem, Particle};
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

/// Pre-computed circle template for a given radius.
/// Stores (dx, dy, alpha_multiplier) offsets relative to center.
struct CircleTemplate {
    /// For each integer radius, store the pixel offsets and alpha weights.
    entries: Vec<CircleEntry>,
}

struct CircleEntry {
    /// Pixel offsets and alpha multiplier for this radius.
    pixels: Vec<(i32, i32, f32)>,
}

impl CircleTemplate {
    /// Pre-compute circle templates for radii 1..=max_radius.
    fn new(max_radius: u32) -> Self {
        let mut entries = Vec::with_capacity(max_radius as usize + 1);

        for r in 0..=max_radius {
            let rf = r as f32;
            let mut pixels = Vec::new();

            if r == 0 {
                pixels.push((0, 0, 1.0));
            } else {
                let r_sq = rf * rf;
                let ir = r as i32;
                for dy in -ir..=ir {
                    for dx in -ir..=ir {
                        let dist_sq = (dx * dx + dy * dy) as f32;
                        if dist_sq <= r_sq {
                            // Smooth edge: anti-alias the last pixel ring
                            let dist = dist_sq.sqrt();
                            let alpha = if dist > rf - 1.0 {
                                (rf - dist).clamp(0.0, 1.0)
                            } else {
                                1.0
                            };
                            if alpha > 0.01 {
                                pixels.push((dx, dy, alpha));
                            }
                        }
                    }
                }
            }

            entries.push(CircleEntry { pixels });
        }

        Self { entries }
    }

    #[inline]
    fn get(&self, radius: u32) -> &[( i32, i32, f32)] {
        let idx = (radius as usize).min(self.entries.len() - 1);
        &self.entries[idx].pixels
    }
}

/// Render a particle visualization video from audio.
/// Uses a pipelined architecture for maximum throughput.
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

    // Step 2: Start FFmpeg process — use ultrafast preset for speed
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
            // Video encoding — ultrafast for speed, YouTube will re-encode anyway
            "-c:v", "libx264",
            "-preset", "ultrafast",
            "-tune", "stillimage",
            "-crf", "23",
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

    // Step 3: Initialize particle system and circle templates
    let particle_config = ParticleConfig {
        symmetry_folds: config.symmetry_folds,
        max_spawn_rate: 12,
        speed_multiplier: 1.0,
        base_hue: config.base_hue,
        hue_range: 120.0,
    };
    let mut system = ParticleSystem::new(particle_config);

    // Pre-compute circle templates up to max possible radius
    let max_radius = 20u32; // max particle size
    let circle_templates = CircleTemplate::new(max_radius);

    // Pre-compute fold angles
    let angle_step = std::f32::consts::TAU / config.symmetry_folds as f32;
    let fold_angles: Vec<(f32, f32)> = (0..config.symmetry_folds)
        .map(|fold| {
            let angle = fold as f32 * angle_step;
            (angle.cos(), angle.sin())
        })
        .collect();

    // Step 4: Pipelined rendering
    // We use a bounded channel so the render thread doesn't get too far ahead of FFmpeg.
    let (tx, rx) = mpsc::sync_channel::<Vec<u8>>(4);

    let width = config.width;
    let height = config.height;

    // Writer thread: receives rendered frames and pipes to FFmpeg
    let writer_handle = std::thread::spawn(move || -> Result<(), String> {
        for frame_data in rx {
            if stdin.write_all(&frame_data).is_err() {
                break;
            }
        }
        drop(stdin);
        Ok(())
    });

    // Main loop: update particles (serial), render frame (parallel pixel ops), send to writer
    let render_progress_start = 5.0;
    let render_progress_end = 95.0;

    let cx = width as f32 / 2.0;
    let cy = height as f32 / 2.0;
    let scale = width.max(height) as f32 / 2.0;
    let bg_color = config.bg_color;
    let base_hue = config.base_hue;

    for (frame_idx, frame_features) in features.iter().enumerate() {
        // Update particle system (must be serial — stateful)
        system.update(frame_features);

        // Render frame using direct pixel manipulation
        let frame_data = render_frame_fast(
            &system.particles,
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
        );

        // Send to writer thread
        if tx.send(frame_data).is_err() {
            break; // Writer died (FFmpeg error)
        }

        // Progress update every 30 frames
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

    // Signal writer we're done
    drop(tx);

    // Wait for writer thread
    writer_handle
        .join()
        .map_err(|_| AppError::FFmpeg("Writer thread panicked".to_string()))?
        .map_err(|e| AppError::FFmpeg(e))?;

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

/// Render a single frame using direct pixel manipulation with parallel scanline rendering.
/// Returns owned RGBA buffer.
fn render_frame_fast(
    particles: &[Particle],
    width: u32,
    height: u32,
    cx: f32,
    cy: f32,
    scale: f32,
    fold_angles: &[(f32, f32)],
    bg_color: (u8, u8, u8),
    base_hue: f32,
    features: &FrameFeatures,
    circle_templates: &CircleTemplate,
) -> Vec<u8> {
    let w = width as usize;
    let h = height as usize;
    let frame_size = w * h * 4;

    // Initialize buffer with background color
    let mut buf = vec![0u8; frame_size];
    // Fill background using chunks for efficiency
    for pixel in buf.chunks_exact_mut(4) {
        pixel[0] = bg_color.0;
        pixel[1] = bg_color.1;
        pixel[2] = bg_color.2;
        pixel[3] = 255;
    }

    // Pre-compute all draw commands (particle positions + colors) to avoid
    // borrow issues and enable potential future parallelism on the draw step.
    // Each draw command: (screen_x, screen_y, radius, r, g, b, alpha)
    let draw_commands: Vec<(f32, f32, u32, u8, u8, u8, u8)> = particles
        .par_iter()
        .flat_map_iter(|particle| {
            let alpha_f = particle.life * particle.brightness;
            let alpha = (alpha_f * 255.0).clamp(0.0, 255.0) as u8;
            if alpha < 5 {
                return Vec::new();
            }

            let (r, g, b) = hsl_to_rgb(particle.hue, particle.saturation, 0.55 + particle.life * 0.15);
            let size = particle.size * (0.5 + features.rms * 0.5);
            let radius = (size as u32).min(20).max(1);

            let mut cmds = Vec::with_capacity(fold_angles.len() * 2);

            for &(cos_a, sin_a) in fold_angles {
                let rx = particle.x * cos_a - particle.y * sin_a;
                let ry = particle.x * sin_a + particle.y * cos_a;

                for &(px, py) in &[(rx, ry), (rx, -ry)] {
                    let screen_x = cx + px * scale;
                    let screen_y = cy + py * scale;

                    // Skip if off-screen (with margin)
                    let rf = radius as f32;
                    if screen_x < -rf || screen_x > width as f32 + rf
                        || screen_y < -rf || screen_y > height as f32 + rf
                    {
                        continue;
                    }

                    cmds.push((screen_x, screen_y, radius, r, g, b, alpha));
                }
            }

            cmds
        })
        .collect();

    // Draw all commands onto the buffer (serial — pixel writes to shared buffer)
    for &(screen_x, screen_y, radius, r, g, b, alpha) in &draw_commands {
        draw_circle_fast(
            &mut buf,
            w,
            h,
            screen_x,
            screen_y,
            radius,
            r,
            g,
            b,
            alpha,
            circle_templates,
        );
    }

    // Center glow
    if features.rms > 0.1 {
        let glow_size = (20.0 + features.rms * 60.0) as u32;
        let glow_radius = glow_size.min(20);
        let glow_alpha = (features.rms * 80.0).clamp(0.0, 80.0) as u8;
        let (gr, gg, gb) = hsl_to_rgb(base_hue, 0.8, 0.6);
        draw_circle_fast(&mut buf, w, h, cx, cy, glow_radius, gr, gg, gb, glow_alpha, circle_templates);
    }

    buf
}

/// Draw a circle directly into the pixel buffer using pre-computed templates.
#[inline]
fn draw_circle_fast(
    buf: &mut [u8],
    width: usize,
    height: usize,
    cx: f32,
    cy: f32,
    radius: u32,
    r: u8,
    g: u8,
    b: u8,
    alpha: u8,
    templates: &CircleTemplate,
) {
    let icx = cx as i32;
    let icy = cy as i32;
    let pixels = templates.get(radius);

    let w = width as i32;
    let h = height as i32;
    let a = alpha as u32;

    for &(dx, dy, edge_alpha) in pixels {
        let px = icx + dx;
        let py = icy + dy;

        if px < 0 || px >= w || py < 0 || py >= h {
            continue;
        }

        let idx = ((py as usize) * width + (px as usize)) * 4;

        // Alpha blend: out = src * alpha + dst * (1 - alpha)
        let final_alpha = ((a * (edge_alpha * 256.0) as u32) >> 8).min(255);
        let inv_alpha = 255 - final_alpha;

        // SAFETY: bounds checked above
        buf[idx]     = ((r as u32 * final_alpha + buf[idx] as u32 * inv_alpha) >> 8) as u8;
        buf[idx + 1] = ((g as u32 * final_alpha + buf[idx + 1] as u32 * inv_alpha) >> 8) as u8;
        buf[idx + 2] = ((b as u32 * final_alpha + buf[idx + 2] as u32 * inv_alpha) >> 8) as u8;
        // Keep alpha at 255 (opaque frame)
    }
}
