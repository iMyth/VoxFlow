//! Starfield tunnel renderer — "flying through space" with audio-reactive effects.
//!
//! Enhanced visuals: spiral motion, nebula background, warp speed lines, richer colors.
//! Performance: GPU-accelerated via wgpu, with CPU fallback.

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;

use log::info;
use rand::Rng;

use super::gpu::{GpuStar, GpuStarfieldRenderer, StarfieldParams};
use crate::commands::audio::ffmpeg::find_ffmpeg;
use crate::commands::audio::particles::audio_analysis::{extract_audio_features, FrameFeatures};
use crate::core::error::AppError;
use crate::core::models::MixProgress;

/// Configuration for starfield video rendering.
#[allow(dead_code)]
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

/// Starfield simulation state.
struct Starfield {
    stars: Vec<Star>,
    max_depth: f32,
    smooth_rms: f32,
    smooth_low: f32,
    smooth_mid: f32,
    prev_rms: f32,
    beat_cooldown: u32,
    is_beat: bool,
    frame_count: u32,
    warp_intensity: f32,
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
                speed_mult: 0.4 + rng.gen::<f32>() * 1.2,
                brightness: 0.6 + rng.gen::<f32>() * 0.4,
            })
            .collect();

        Self {
            stars,
            max_depth,
            smooth_rms: 0.0,
            smooth_low: 0.0,
            smooth_mid: 0.0,
            prev_rms: 0.0,
            beat_cooldown: 0,
            is_beat: false,
            frame_count: 0,
            warp_intensity: 0.0,
        }
    }

    fn update(&mut self, features: &FrameFeatures, width: u32, height: u32) {
        self.frame_count += 1;
        let mut rng = rand::thread_rng();

        self.smooth_rms += (features.rms - self.smooth_rms) * 0.15;
        self.smooth_low += (features.low_energy - self.smooth_low) * 0.12;
        self.smooth_mid += (features.mid_energy - self.smooth_mid) * 0.12;

        // Beat detection
        self.is_beat = self.smooth_rms > self.prev_rms + 0.10 && self.beat_cooldown == 0;
        if self.is_beat {
            self.beat_cooldown = 8;
            self.warp_intensity = 1.0; // Flash warp on beat
        }
        if self.beat_cooldown > 0 {
            self.beat_cooldown -= 1;
        }
        self.prev_rms = self.smooth_rms;

        // Warp intensity decays
        self.warp_intensity *= 0.92;

        let base_speed = 0.015;
        let audio_speed = self.smooth_rms * 0.012 + self.smooth_low * 0.005;
        let global_speed = base_speed + audio_speed;

        let cx = width as f32 / 2.0;
        let cy = height as f32 / 2.0;
        let focal = width as f32 * 0.6;

        // Subtle spiral rotation (audio-reactive)
        let spiral_angle = self.smooth_mid * 0.003;
        let cos_s = spiral_angle.cos();
        let sin_s = spiral_angle.sin();

        for star in &mut self.stars {
            // Save previous screen position for trail
            if star.z > 0.01 {
                let inv_z = 1.0 / star.z;
                let sx = cx + star.x * focal * inv_z;
                let sy = cy + star.y * focal * inv_z;
                star.prev_sx = sx;
                star.prev_sy = sy;
                star.has_prev = true;
            }

            // Move star toward camera
            star.z -= global_speed * star.speed_mult;

            // Apply subtle spiral rotation
            let new_x = star.x * cos_s - star.y * sin_s;
            let new_y = star.x * sin_s + star.y * cos_s;
            star.x = new_x;
            star.y = new_y;

            // Respawn if past camera
            if star.z <= 0.01 {
                star.x = rng.gen::<f32>() * 2.0 - 1.0;
                star.y = rng.gen::<f32>() * 2.0 - 1.0;
                star.z = self.max_depth - rng.gen::<f32>() * 0.3;
                star.has_prev = false;
                star.speed_mult = 0.4 + rng.gen::<f32>() * 1.2;
                star.brightness = 0.6 + rng.gen::<f32>() * 0.4;
                // More colorful stars
                star.hue = if self.is_beat {
                    (self.frame_count as f32 * 3.0 + rng.gen::<f32>() * 40.0) % 360.0
                } else {
                    // Bias toward blue/purple/cyan for space feel
                    180.0 + rng.gen::<f32>() * 180.0
                };
            }
        }

        // Burst new stars on beat
        if self.is_beat {
            let burst_count = 80.min(1500 - self.stars.len());
            for _ in 0..burst_count {
                self.stars.push(Star {
                    x: rng.gen::<f32>() * 2.0 - 1.0,
                    y: rng.gen::<f32>() * 2.0 - 1.0,
                    z: self.max_depth * (0.6 + rng.gen::<f32>() * 0.4),
                    prev_sx: 0.0,
                    prev_sy: 0.0,
                    has_prev: false,
                    hue: (self.frame_count as f32 * 3.0 + rng.gen::<f32>() * 60.0) % 360.0,
                    speed_mult: 0.6 + rng.gen::<f32>() * 1.0,
                    brightness: 0.8 + rng.gen::<f32>() * 0.2,
                });
            }
        }

        // Cap star count
        if self.stars.len() > 1500 {
            self.stars.truncate(1200);
        }
    }

    /// Convert stars to GPU format for the current frame.
    fn to_gpu_stars(&self, width: u32, height: u32) -> Vec<GpuStar> {
        let cx = width as f32 / 2.0;
        let cy = height as f32 / 2.0;
        let focal = width as f32 * 0.6;
        let max_depth = self.max_depth;

        self.stars
            .iter()
            .filter(|s| s.z > 0.01)
            .take(2000)
            .map(|star| {
                let inv_z = 1.0 / star.z;
                let sx = cx + star.x * focal * inv_z;
                let sy = cy + star.y * focal * inv_z;
                let depth = 1.0 - (star.z / max_depth);

                GpuStar {
                    sx,
                    sy,
                    prev_sx: star.prev_sx,
                    prev_sy: star.prev_sy,
                    depth,
                    hue: star.hue,
                    brightness: star.brightness,
                    has_trail: if star.has_prev { 1.0 } else { 0.0 },
                }
            })
            .collect()
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

    // Render at half resolution, FFmpeg upscales with lanczos
    let render_width = config.width / 2;
    let render_height = config.height / 2;
    let scale_filter = format!("scale={}:{}:flags=lanczos", config.width, config.height);

    // Try GPU renderer
    let gpu_renderer = GpuStarfieldRenderer::new(render_width, render_height);
    let use_gpu = gpu_renderer.is_some();

    if use_gpu {
        info!("[Starfield] Using GPU-accelerated rendering");
    } else {
        info!("[Starfield] GPU unavailable, falling back to CPU");
    }

    on_progress(MixProgress {
        percent: 5.0,
        stage: format!("准备完成，共 {} 帧", total_frames),
    });

    // Start FFmpeg
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

    let (tx, rx) = mpsc::sync_channel::<Vec<u8>>(4);
    let writer_handle = std::thread::spawn(move || -> Result<(), String> {
        for frame_data in rx {
            if stdin.write_all(&frame_data).is_err() { break; }
        }
        drop(stdin);
        Ok(())
    });

    // Initialize starfield with more stars
    let mut starfield = Starfield::new(1200, 2.0);

    let fg_color = config.fg_color;
    let bg_color = config.bg_color;

    // ─── Frame Loop ──────────────────────────────────────────────────────
    for (frame_idx, frame_features) in features.iter().enumerate() {
        if cancel_flag.load(Ordering::Relaxed) {
            info!("[Starfield] Render cancelled at frame {}/{}", frame_idx, total_frames);
            break;
        }

        starfield.update(frame_features, render_width, render_height);

        let frame_data = if let Some(ref gpu) = gpu_renderer {
            let gpu_stars = starfield.to_gpu_stars(render_width, render_height);
            let params = StarfieldParams {
                width: render_width,
                height: render_height,
                num_stars: gpu_stars.len() as u32,
                _pad0: 0,
                rms: starfield.smooth_rms,
                low_energy: starfield.smooth_low,
                mid_energy: starfield.smooth_mid,
                high_energy: frame_features.high_energy,
                fg_r: fg_color.0 as f32 / 255.0,
                fg_g: fg_color.1 as f32 / 255.0,
                fg_b: fg_color.2 as f32 / 255.0,
                bg_r: bg_color.0 as f32 / 255.0,
                bg_g: bg_color.1 as f32 / 255.0,
                bg_b: bg_color.2 as f32 / 255.0,
                frame_time: frame_idx as f32,
                warp_intensity: starfield.warp_intensity,
            };
            gpu.render_frame(&params, &gpu_stars)
        } else {
            // Simple CPU fallback — just black with white dots
            render_starfield_frame_cpu(&starfield, render_width, render_height, bg_color)
        };

        if tx.send(frame_data).is_err() { break; }

        if frame_idx % 30 == 0 || frame_idx == total_frames - 1 {
            let pct = 5.0 + (frame_idx as f32 / total_frames as f32) * 90.0;
            on_progress(MixProgress {
                percent: pct,
                stage: format!("渲染帧 {}/{}", frame_idx + 1, total_frames),
            });
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
        return Err(AppError::FFmpeg(format!(
            "FFmpeg encoding failed: {}",
            stderr.chars().take(500).collect::<String>()
        )));
    }

    on_progress(MixProgress {
        percent: 100.0,
        stage: "视频生成完成".to_string(),
    });

    info!("[Starfield] Video rendered successfully (GPU={}): {:?}", use_gpu, config.output_path);
    Ok(())
}

/// Simple CPU fallback — basic star rendering without fancy effects.
fn render_starfield_frame_cpu(
    starfield: &Starfield,
    width: u32,
    height: u32,
    bg_color: (u8, u8, u8),
) -> Vec<u8> {
    let w = width as usize;
    let h = height as usize;
    let mut buf = vec![0u8; w * h * 4];

    // Fill background
    for i in 0..(w * h) {
        let idx = i * 4;
        buf[idx] = bg_color.0;
        buf[idx + 1] = bg_color.1;
        buf[idx + 2] = bg_color.2;
        buf[idx + 3] = 255;
    }

    let cx = width as f32 / 2.0;
    let cy = height as f32 / 2.0;
    let focal = width as f32 * 0.6;

    for star in &starfield.stars {
        if star.z <= 0.01 { continue; }

        let inv_z = 1.0 / star.z;
        let sx = (cx + star.x * focal * inv_z) as i32;
        let sy = (cy + star.y * focal * inv_z) as i32;

        if sx < 0 || sx >= w as i32 || sy < 0 || sy >= h as i32 { continue; }

        let depth = 1.0 - (star.z / starfield.max_depth);
        let alpha = (depth * star.brightness * 255.0).min(255.0) as u32;
        let inv_a = 255 - alpha;

        let idx = (sy as usize * w + sx as usize) * 4;
        buf[idx]     = ((255 * alpha + buf[idx] as u32 * inv_a) >> 8) as u8;
        buf[idx + 1] = ((255 * alpha + buf[idx + 1] as u32 * inv_a) >> 8) as u8;
        buf[idx + 2] = ((255 * alpha + buf[idx + 2] as u32 * inv_a) >> 8) as u8;
    }

    buf
}
