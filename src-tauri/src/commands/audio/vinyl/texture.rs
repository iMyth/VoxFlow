//! Disc texture generation and bokeh particle definitions for vinyl renderer.

use std::path::PathBuf;

/// Pre-rendered cover image as a circular texture (RGBA pixels, square).
pub(super) struct CoverTexture {
    pub pixels: Vec<u8>,
    pub size: u32,
}

impl CoverTexture {
    pub fn load(path: &PathBuf, target_size: u32) -> Result<Self, String> {
        let img = image::open(path)
            .map_err(|e| format!("Failed to load cover image: {}", e))?;

        let resized = img.resize_to_fill(
            target_size,
            target_size,
            image::imageops::FilterType::Lanczos3,
        );
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

    pub fn default_vinyl(target_size: u32, fg_color: (u8, u8, u8)) -> Self {
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
                        // Label area
                        let label_t = norm_dist / 0.28;
                        let brightness = 0.7 - label_t * 0.2;
                        pixels[idx] = (fg_color.0 as f32 * brightness).min(255.0) as u8;
                        pixels[idx + 1] = (fg_color.1 as f32 * brightness).min(255.0) as u8;
                        pixels[idx + 2] = (fg_color.2 as f32 * brightness).min(255.0) as u8;
                        pixels[idx + 3] = 255;
                    } else {
                        // Groove area
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
pub(super) struct BokehParticle {
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    pub radius: f32,
    pub alpha: f32,
    pub hue_offset: f32,
}

pub(super) fn generate_bokeh_particles(width: u32, height: u32, count: usize) -> Vec<BokehParticle> {
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
