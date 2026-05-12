//! CPU fallback drawing primitives for particle kaleidoscope rendering.
//!
//! Contains circle templates, draw commands, and all rasterization functions
//! used when GPU is unavailable.

use rayon::prelude::*;

use super::audio_analysis::FrameFeatures;
use super::particle_system::{hsl_to_rgb, ParticleKind, ParticleSystem};

// ─── Circle Template ─────────────────────────────────────────────────────────

/// Pre-computed circle template with integer alpha values.
pub(super) struct CircleTemplate {
    entries: Vec<CircleEntry>,
}

struct CircleEntry {
    pixels: Vec<(i32, i32, u8)>,
}

impl CircleTemplate {
    pub fn new(max_radius: u32) -> Self {
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
    pub fn get(&self, radius: u32) -> &[(i32, i32, u8)] {
        let idx = (radius as usize).min(self.entries.len() - 1);
        &self.entries[idx].pixels
    }
}

// ─── Draw Commands ───────────────────────────────────────────────────────────

/// A draw command for a single circle to render.
pub(super) struct DrawCmd {
    pub screen_x: i32,
    pub screen_y: i32,
    pub radius: u32,
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub alpha: u8,
    pub is_glow: bool,
}

// ─── Frame Rendering ─────────────────────────────────────────────────────────

/// Render a single frame with enhanced visuals (CPU fallback path).
pub(super) fn render_frame_cpu(
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

    let mut buf = bg_base.to_vec();

    // Draw rings (behind particles)
    for ring in &system.rings {
        draw_ring_fast(
            &mut buf, w, h, cx, cy, scale,
            ring.radius, ring.thickness, ring.hue, ring.life * ring.brightness,
        );
    }

    // Generate draw commands in parallel
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

            let mut cmds: arrayvec::ArrayVec<DrawCmd, 32> = arrayvec::ArrayVec::new();

            for &(cos_a, sin_a) in fold_angles {
                let rx = particle.x * cos_a - particle.y * sin_a;
                let ry = particle.x * sin_a + particle.y * cos_a;

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

// ─── Background ──────────────────────────────────────────────────────────────

/// Pre-compute the background gradient (one-time cost).
pub(super) fn precompute_bg_gradient(w: usize, h: usize, bg_color: (u8, u8, u8)) -> Vec<u8> {
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

// ─── Drawing Primitives ──────────────────────────────────────────────────────

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
        let final_alpha = (a * edge_alpha as u32) >> 8;
        let inv_alpha = 255 - final_alpha;

        buf[idx]     = ((r as u32 * final_alpha + buf[idx] as u32 * inv_alpha) >> 8) as u8;
        buf[idx + 1] = ((g as u32 * final_alpha + buf[idx + 1] as u32 * inv_alpha) >> 8) as u8;
        buf[idx + 2] = ((b as u32 * final_alpha + buf[idx + 2] as u32 * inv_alpha) >> 8) as u8;
    }
}

// ─── Utilities ───────────────────────────────────────────────────────────────

#[inline]
fn lerp_u8(a: u8, b: u8, t: f32) -> u8 {
    (a as f32 + (b as f32 - a as f32) * t).clamp(0.0, 255.0) as u8
}
