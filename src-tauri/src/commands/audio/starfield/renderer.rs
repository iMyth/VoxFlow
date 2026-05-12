//! Starfield renderer — classic "flying through space" effect with audio-reactive
//! speed, trails, and color bursts. Optimized for real-time-ish rendering.
//!
//! Performance strategy:
//! - Background is pre-computed once and memcpy'd each frame
//! - No per-frame sorting (stars are small, overlap is fine)
//! - Minimal per-pixel math (no sqrt in hot loops)
//! - Trail drawing uses fast integer Bresenham

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;

use log::info;
use rand::Rng;

use crate::commands::audio::ffmpeg::find_ffmpeg;
use crate::commands::audio::particles::audio_analysis::{extract_audio_features, FrameFeatures};
use crate::commands::audio::particles::particle_system::hsl_to_rgb;
use crate::core::error::AppError;
use crate::core::models::MixProgress;

/// Configuration for starfield video rendering.
pub struct StarfieldVideoConfig {
    pub audio_path: PathBuf,
    pub output_path: PathBuf,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub fg_color: (u8, u8, u8),
    pub bg_color: (u8, u8, u8),
    pub bg_image_path: Option<PathBuf>,
}

/// A single star in 3D space.
#[derive(Clone)]
struct Star {
    x: f32,
    y: f32,
    z: f32,
    prev_sx: f32,
    prev_sy: f32,
    has_prev: bool,
    hue: f32,
    speed_mult: f32,
    brightness: f32,
}

/// Starfield state.
struct Starfield {
    stars: Vec<Star>,
    max_depth: f32,
    smooth_rms: f32,
    smooth_low: f32,
    prev_rms: f32,
    beat_cooldown: u32,
    is_beat: bool,
    frame_count: u32,
}

impl Starfield {
    fn new(num_stars: usize, max_depth: f32) -> Self {
        let mut rng = rand::thread_rng();
        let stars: Vec<Star> = (0..num_stars)
            .map(|_| Star {
                x: rng.gen::<f32>() * 2.0 - 1.0,
                y: rng.gen::<f32>() * 2.0 - 1.0,
                z: rng.gen::<f32>() * max_depth,
                prev_sx: 0.0,
                prev_sy: 0.0,
                has_prev: false,
                hue: rng.gen::<f32>() * 360.0,
                speed_mult: 0.5 + rng.gen::<f32>() * 1.0,
                brightness: 0.7 + rng.gen::<f32>() * 0.3,
            })
            .collect();

        Self {
            stars,
            max_depth,
            smooth_rms: 0.0,
            smooth_low: 0.0,
            prev_rms: 0.0,
            beat_cooldown: 0,
            is_beat: false,
            frame_count: 0,
        }
    }

    fn update(&mut self, features: &FrameFeatures, width: u32, height: u32) {
        self.frame_count += 1;
        let mut rng = rand::thread_rng();

        self.smooth_rms += (features.rms - self.smooth_rms) * 0.12;
        self.smooth_low += (features.low_energy - self.smooth_low) * 0.10;

        self.is_beat = self.smooth_rms > self.prev_rms + 0.12 && self.beat_cooldown == 0;
        if self.is_beat {
            self.beat_cooldown = 10;
        }
        if self.beat_cooldown > 0 {
            self.beat_cooldown -= 1;
        }
        self.prev_rms = self.smooth_rms;

        let base_speed = 0.012;
        let audio_mod = self.smooth_rms * 0.008;
        let global_speed = base_speed + audio_mod;

        let cx = width as f32 / 2.0;
        let cy = height as f32 / 2.0;
        let focal = width as f32 * 0.6;

        for star in &mut self.stars {
            if star.z > 0.01 {
                let sx = cx + (star.x * focal) / star.z;
                let sy = cy + (star.y * focal) / star.z;
                star.prev_sx = sx;
                star.prev_sy = sy;
                star.has_prev = true;
            }

            star.z -= global_speed * star.speed_mult;

            if star.z <= 0.01 {
                star.x = rng.gen::<f32>() * 2.0 - 1.0;
                star.y = rng.gen::<f32>() * 2.0 - 1.0;
                star.z = self.max_depth - rng.gen::<f32>() * 0.2;
                star.has_prev = false;
                star.speed_mult = 0.5 + rng.gen::<f32>() * 1.0;
                star.brightness = 0.7 + rng.gen::<f32>() * 0.3;
                star.hue = if self.is_beat {
                    (self.frame_count as f32 * 2.0) % 360.0
                } else {
                    rng.gen::<f32>() * 360.0
                };
            }
        }

        if self.is_beat {
            for _ in 0..50 {
                if self.stars.len() < 1200 {
                    self.stars.push(Star {
                        x: rng.gen::<f32>() * 2.0 - 1.0,
                        y: rng.gen::<f32>() * 2.0 - 1.0,
                        z: self.max_depth * (0.7 + rng.gen::<f32>() * 0.3),
                        prev_sx: 0.0,
                        prev_sy: 0.0,
                        has_prev: false,
                        hue: (self.frame_count as f32 * 2.0 + rng.gen::<f32>() * 30.0) % 360.0,
                        speed_mult: 0.8 + rng.gen::<f32>() * 0.8,
                        brightness: 0.9 + rng.gen::<f32>() * 0.1,
                    });
                }
            }
        }

        if self.stars.len() > 1200 {
            self.stars.truncate(1000);
        }
    }
}

/// Render a starfield visualization video from audio.
pub fn render_starfield_video<F>(
    config: &StarfieldVideoConfig,
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
        "[Starfield] {} frames to render at {}fps ({}x{})",
        total_frames, config.fps, config.width, config.height
    );

    if total_frames == 0 {
        return Err(AppError::FFmpeg("No audio frames to render".to_string()));
    }

    let width = config.width;
    let height = config.height;
    let w = width as usize;
    let h = height as usize;

    // Pre-compute background once (the expensive radial gradient or image blend)
    on_progress(MixProgress {
        percent: 2.0,
        stage: "正在准备背景".to_string(),
    });

    let bg_frame = precompute_background(
        w, h,
        config.bg_color,
        config.bg_image_path.as_ref(),
    );

    on_progress(MixProgress {
        percent: 5.0,
        stage: format!("准备完成，共 {} 帧", total_frames),
    });

    // Start FFmpeg
    let ffmpeg_bin = find_ffmpeg();
    let mut child = Command::new(&ffmpeg_bin)
        .args([
            "-y",
            "-f", "rawvideo",
            "-pixel_format", "rgba",
            "-video_size", &format!("{}x{}", width, height),
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

    let num_stars = 800;
    let max_depth = 1.5f32;
    let mut starfield = Starfield::new(num_stars, max_depth);

    let fg_color = config.fg_color;

    for (frame_idx, frame_features) in features.iter().enumerate() {
        if cancel_flag.load(Ordering::Relaxed) {
            info!("[Starfield] Render cancelled at frame {}/{}", frame_idx, total_frames);
            break;
        }

        starfield.update(frame_features, width, height);

        let frame_data = render_starfield_frame(
            &starfield,
            width, height,
            fg_color,
            &bg_frame,
            frame_features,
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

    info!("[Starfield] Video rendered successfully: {:?}", config.output_path);
    Ok(())
}

/// Pre-compute the background frame (radial gradient or image blend).
/// This is done ONCE and then memcpy'd into each frame buffer.
fn precompute_background(
    w: usize,
    h: usize,
    bg_color: (u8, u8, u8),
    bg_image_path: Option<&PathBuf>,
) -> Vec<u8> {
    let mut buf = vec![0u8; w * h * 4];
    let cx = w as f32 / 2.0;
    let cy = h as f32 / 2.0;

    if let Some(path) = bg_image_path {
        if let Ok(tex) = load_bg_image(path, w as u32, h as u32) {
            for i in 0..(w * h) {
                let idx = i * 4;
                let tex_r = tex[idx] as u32;
                let tex_g = tex[idx + 1] as u32;
                let tex_b = tex[idx + 2] as u32;
                buf[idx]     = ((bg_color.0 as u32 * 160 + tex_r * 95) >> 8) as u8;
                buf[idx + 1] = ((bg_color.1 as u32 * 160 + tex_g * 95) >> 8) as u8;
                buf[idx + 2] = ((bg_color.2 as u32 * 160 + tex_b * 95) >> 8) as u8;
                buf[idx + 3] = 255;
            }
            return buf;
        }
    }

    // Radial gradient — use squared distance to avoid sqrt
    let max_dist_sq = cx * cx + cy * cy;
    let (br, bg_val, bb) = bg_color;
    for y in 0..h {
        let dy = y as f32 - cy;
        let dy_sq = dy * dy;
        for x in 0..w {
            let dx = x as f32 - cx;
            let dist_sq = dx * dx + dy_sq;
            let t = (dist_sq / max_dist_sq).min(1.0); // No sqrt needed!
            let idx = (y * w + x) * 4;
            let factor = 1.0 + (1.0 - t) * 0.15;
            buf[idx]     = (br as f32 * factor).min(255.0) as u8;
            buf[idx + 1] = (bg_val as f32 * factor).min(255.0) as u8;
            buf[idx + 2] = (bb as f32 * factor).min(255.0) as u8;
            buf[idx + 3] = 255;
        }
    }

    buf
}

/// Render a single starfield frame. Background is pre-computed and just copied in.
fn render_starfield_frame(
    starfield: &Starfield,
    width: u32,
    height: u32,
    fg_color: (u8, u8, u8),
    bg_frame: &[u8],
    features: &FrameFeatures,
) -> Vec<u8> {
    let w = width as usize;
    let h = height as usize;

    // Start from pre-computed background (fast memcpy)
    let mut buf = bg_frame.to_vec();

    let cx = width as f32 / 2.0;
    let cy = height as f32 / 2.0;
    let focal = width as f32 * 0.6;
    let max_depth = starfield.max_depth;

    // Draw stars directly — no sorting needed (small dots, alpha blended)
    for star in &starfield.stars {
        if star.z <= 0.01 {
            continue;
        }

        // Project to screen
        let inv_z = 1.0 / star.z;
        let sx = cx + star.x * focal * inv_z;
        let sy = cy + star.y * focal * inv_z;

        // Skip if off-screen (with small margin)
        if sx < -10.0 || sx > width as f32 + 10.0 || sy < -10.0 || sy > height as f32 + 10.0 {
            continue;
        }

        // Depth factor: 0 = far, 1 = near
        let depth_factor = 1.0 - (star.z / max_depth);
        let star_alpha = (depth_factor * star.brightness * 255.0).min(255.0) as u8;
        if star_alpha < 8 {
            continue;
        }

        // Size: 1px far → 8px near (quadratic growth)
        let star_size = (1.0 + depth_factor * depth_factor * 7.0) as u32;

        // Color
        let (sr, sg, sb) = if starfield.is_beat || depth_factor > 0.7 {
            let (hr, hg, hb) = hsl_to_rgb(star.hue, 0.7, 0.7);
            let blend = depth_factor * 0.6;
            (lerp_u8(255, hr, blend), lerp_u8(255, hg, blend), lerp_u8(255, hb, blend))
        } else {
            let w_val = depth_factor * 0.3;
            ((255.0 - w_val * 30.0) as u8, (255.0 - w_val * 10.0) as u8, 255u8)
        };

        // Draw trail (fast line)
        if star.has_prev && depth_factor > 0.15 {
            let trail_alpha = (star_alpha >> 1) as u8; // 50% of star alpha
            let tr = (sr >> 1) + (sr >> 2); // ~75% brightness
            let tg = (sg >> 1) + (sg >> 2);
            let tb = (sb >> 1) + (sb >> 2);
            draw_line_fast(&mut buf, w, h, star.prev_sx, star.prev_sy, sx, sy, tr, tg, tb, trail_alpha);
        }

        // Draw star dot
        if star_size <= 1 {
            // Single pixel — fastest path
            let ix = sx as i32;
            let iy = sy as i32;
            if ix >= 0 && ix < w as i32 && iy >= 0 && iy < h as i32 {
                let idx = (iy as usize * w + ix as usize) * 4;
                blend_pixel(&mut buf, idx, sr, sg, sb, star_alpha);
            }
        } else {
            draw_star_dot_fast(&mut buf, w, h, sx, sy, star_size, sr, sg, sb, star_alpha);
        }
    }

    // Center glow (small, cheap)
    if features.rms > 0.05 {
        let glow_alpha = (30.0 + features.rms * 40.0) as u8;
        let glow_radius = (6 + (features.low_energy * 12.0) as i32).min(18);
        draw_soft_glow_fast(&mut buf, w, h, cx, cy, glow_radius, fg_color.0, fg_color.1, fg_color.2, glow_alpha);
    }

    buf
}

/// Draw a star dot using a simple filled circle (no sqrt, uses r² comparison).
#[inline]
fn draw_star_dot_fast(
    buf: &mut [u8],
    w: usize,
    h: usize,
    cx: f32,
    cy: f32,
    radius: u32,
    r: u8,
    g: u8,
    b: u8,
    alpha: u8,
) {
    let ir = radius as i32;
    let icx = cx as i32;
    let icy = cy as i32;
    let r_sq = (radius * radius) as i32;
    let r_sq_outer = ((radius + 1) * (radius + 1)) as i32; // For anti-alias ring

    for dy in -ir..=ir {
        let py = icy + dy;
        if py < 0 || py >= h as i32 {
            continue;
        }
        let dy_sq = dy * dy;
        for dx in -ir..=ir {
            let px = icx + dx;
            if px < 0 || px >= w as i32 {
                continue;
            }
            let dist_sq = dx * dx + dy_sq;
            if dist_sq <= r_sq {
                let idx = (py as usize * w + px as usize) * 4;
                blend_pixel(buf, idx, r, g, b, alpha);
            } else if dist_sq <= r_sq_outer {
                // Anti-alias edge: half alpha
                let idx = (py as usize * w + px as usize) * 4;
                blend_pixel(buf, idx, r, g, b, alpha >> 1);
            }
        }
    }
}

/// Draw a line using fast integer stepping (no sub-pixel blending).
#[inline]
fn draw_line_fast(
    buf: &mut [u8],
    w: usize,
    h: usize,
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    r: u8,
    g: u8,
    b: u8,
    alpha: u8,
) {
    let dx = x1 - x0;
    let dy = y1 - y0;
    let steps = (dx.abs().max(dy.abs())) as u32;
    if steps == 0 || steps > 200 {
        return; // Skip degenerate or extremely long lines
    }

    let x_inc = dx / steps as f32;
    let y_inc = dy / steps as f32;
    let mut x = x0;
    let mut y = y0;

    for _ in 0..=steps {
        let ix = x as i32;
        let iy = y as i32;
        if ix >= 0 && ix < w as i32 && iy >= 0 && iy < h as i32 {
            let idx = (iy as usize * w + ix as usize) * 4;
            blend_pixel(buf, idx, r, g, b, alpha);
        }
        x += x_inc;
        y += y_inc;
    }
}

/// Draw a soft glow using squared distance (no sqrt).
fn draw_soft_glow_fast(
    buf: &mut [u8],
    w: usize,
    h: usize,
    cx: f32,
    cy: f32,
    radius: i32,
    r: u8,
    g: u8,
    b: u8,
    max_alpha: u8,
) {
    let icx = cx as i32;
    let icy = cy as i32;
    let r_sq = (radius * radius) as f32;

    for dy in -radius..=radius {
        let py = icy + dy;
        if py < 0 || py >= h as i32 {
            continue;
        }
        let dy_sq = (dy * dy) as f32;
        for dx in -radius..=radius {
            let px = icx + dx;
            if px < 0 || px >= w as i32 {
                continue;
            }
            let dist_sq = (dx * dx) as f32 + dy_sq;
            if dist_sq <= r_sq {
                // Quadratic falloff using dist_sq/r_sq (no sqrt!)
                let t = 1.0 - dist_sq / r_sq;
                let a = (max_alpha as f32 * t) as u8;
                if a > 2 {
                    let idx = (py as usize * w + px as usize) * 4;
                    blend_pixel(buf, idx, r, g, b, a);
                }
            }
        }
    }
}

/// Fast alpha blend a single pixel.
#[inline(always)]
fn blend_pixel(buf: &mut [u8], idx: usize, r: u8, g: u8, b: u8, alpha: u8) {
    let a = alpha as u32;
    let inv_a = 255 - a;
    buf[idx]     = ((r as u32 * a + buf[idx] as u32 * inv_a) >> 8) as u8;
    buf[idx + 1] = ((g as u32 * a + buf[idx + 1] as u32 * inv_a) >> 8) as u8;
    buf[idx + 2] = ((b as u32 * a + buf[idx + 2] as u32 * inv_a) >> 8) as u8;
}

/// Load and resize a background image. Returns RGBA buffer.
fn load_bg_image(path: &PathBuf, width: u32, height: u32) -> Result<Vec<u8>, String> {
    let img = image::open(path)
        .map_err(|e| format!("Failed to load background image: {}", e))?;
    let resized = img.resize_to_fill(width, height, image::imageops::FilterType::Lanczos3);
    let rgba = resized.to_rgba8();
    Ok(rgba.into_raw())
}

/// Linear interpolation between two u8 values.
#[inline]
fn lerp_u8(a: u8, b: u8, t: f32) -> u8 {
    (a as f32 + (b as f32 - a as f32) * t).clamp(0.0, 255.0) as u8
}
