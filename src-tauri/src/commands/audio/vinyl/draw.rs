//! CPU fallback drawing functions for vinyl disc rendering.
//!
//! Contains all layer-rendering functions used when GPU is unavailable.

use rayon::prelude::*;

use super::texture::{BokehParticle, CoverTexture};
use super::utils::*;
use crate::commands::audio::particles::audio_analysis::FrameFeatures;

/// Render a single vinyl frame with all visual layers (CPU path).
pub(super) fn render_vinyl_frame(
    width: u32,
    height: u32,
    cover: &CoverTexture,
    angle: f32,
    smoothed: &FrameFeatures,
    raw: &FrameFeatures,
    fg_color: (u8, u8, u8),
    bg_color: (u8, u8, u8),
    frame_idx: u32,
    bg_frame: &[u8],
    bokeh_particles: &[BokehParticle],
) -> Vec<u8> {
    let w = width as usize;
    let h = height as usize;

    let mut buf = bg_frame.to_vec();

    let cx = width as f32 / 2.0;
    let cy = height as f32 / 2.0;

    // Layer 1: Background bokeh particles
    draw_bokeh_layer(
        &mut buf,
        w,
        h,
        bokeh_particles,
        frame_idx,
        fg_color,
        smoothed.rms,
    );

    // Layer 2: Outer glow rings
    let disc_radius = cover.size as f32 / 2.0;

    let bass_ring_radius = disc_radius + 20.0 + smoothed.low_energy * 25.0;
    let bass_ring_width = 3.0 + smoothed.low_energy * 8.0;
    let bass_ring_alpha = (smoothed.low_energy * 100.0).clamp(0.0, 100.0) as u8;
    if bass_ring_alpha > 5 {
        draw_glow_ring(
            &mut buf,
            w,
            h,
            cx,
            cy,
            bass_ring_radius,
            bass_ring_width,
            fg_color,
            bass_ring_alpha,
        );
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
        draw_glow_ring(
            &mut buf,
            w,
            h,
            cx,
            cy,
            mid_ring_radius,
            mid_ring_width,
            mid_color,
            mid_ring_alpha,
        );
    }

    // Layer 3: EQ spectrum bars
    draw_eq_bars(
        &mut buf,
        w,
        h,
        cx,
        cy,
        disc_radius,
        smoothed,
        raw,
        fg_color,
        frame_idx,
    );

    // Layer 4: Disc + iridescent edge
    let pulse_scale = 1.0 + smoothed.low_energy * 0.02;
    blit_disc_with_edge(&mut buf, w, h, cx, cy, cover, angle, pulse_scale, frame_idx);

    // Layer 5: Center spindle hole
    let hole_radius = (cover.size as f32 * 0.025) as i32;
    draw_spindle_hole(&mut buf, w, h, cx, cy, hole_radius, bg_color);

    // Layer 6: Specular highlight
    let highlight_angle = angle * 0.05;
    let hl_offset_x = highlight_angle.cos() * disc_radius * 0.15;
    let hl_offset_y = highlight_angle.sin() * disc_radius * 0.15;
    draw_specular_highlight(
        &mut buf,
        w,
        h,
        cx - disc_radius * 0.2 + hl_offset_x,
        cy - disc_radius * 0.2 + hl_offset_y,
        disc_radius * 0.35,
        disc_radius * pulse_scale,
        cx,
        cy,
    );

    // Layer 7: Inner glow ring
    let inner_ring_radius = disc_radius * pulse_scale * 0.92;
    let inner_ring_alpha = (smoothed.high_energy * 50.0).clamp(0.0, 50.0) as u8;
    if inner_ring_alpha > 3 {
        draw_glow_ring(
            &mut buf,
            w,
            h,
            cx,
            cy,
            inner_ring_radius,
            2.0,
            (255, 255, 255),
            inner_ring_alpha,
        );
    }

    buf
}

// ─── Layer Drawing Functions ─────────────────────────────────────────────────

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
        let ri = radius as i32 + 1;
        let y_start = (py as i32 - ri).max(0) as usize;
        let y_end = (py as i32 + ri + 1).min(h as i32) as usize;
        let x_start = (px as i32 - ri).max(0) as usize;
        let x_end = (px as i32 + ri + 1).min(w as i32) as usize;

        for y in y_start..y_end {
            let dy = y as f32 - py;
            for x in x_start..x_end {
                let dx = x as f32 - px;
                let dist_sq = dx * dx + dy * dy;
                if dist_sq < r_sq {
                    let t = dist_sq / r_sq;
                    let falloff = (1.0 - t) * (1.0 - t);
                    let a = (alpha as f32 * falloff) as u8;
                    if a > 1 {
                        alpha_blend(buf, (y * w + x) * 4, r, g, b, a);
                    }
                }
            }
        }
    }
}

fn draw_eq_bars(
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
    let num_bars: u32 = 32;
    let bar_inner_radius = disc_radius + 28.0;
    let bar_max_height = disc_radius * 0.55;
    let bar_width = 5.0;
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

        let total_energy = (energy * 0.7 + raw.rms * 0.3).clamp(0.0, 1.0);
        let bar_height = bar_max_height * total_energy * (0.4 + smoothed.rms * 0.6);
        if bar_height < 3.0 {
            continue;
        }

        let cos_a = bar_angle.cos();
        let sin_a = bar_angle.sin();
        let steps = bar_height as u32;
        let half_width = bar_width / 2.0;

        for step in 0..steps {
            let r = bar_inner_radius + step as f32;
            let base_px = cx + r * cos_a;
            let base_py = cy + r * sin_a;
            let t = step as f32 / steps as f32;
            let bar_r = lerp_u8(fg_color.0, 255, t * t * 0.7);
            let bar_g = lerp_u8(fg_color.1, 255, t * t * 0.7);
            let bar_b = lerp_u8(fg_color.2, 255, t * t * 0.7);
            let bar_alpha = (220.0 * (1.0 - t * t * 0.5)) as u8;

            let perp_x = -sin_a;
            let perp_y = cos_a;
            for wi in -3i32..=3 {
                let wt = wi as f32 / 4.0;
                let px = (base_px + perp_x * wt * half_width) as i32;
                let py = (base_py + perp_y * wt * half_width) as i32;
                if px >= 0 && px < w as i32 && py >= 0 && py < h as i32 {
                    let edge_dist = (wt.abs() * 2.0 - 0.5).max(0.0) * 2.0;
                    let edge_alpha = ((1.0 - edge_dist) * bar_alpha as f32) as u8;
                    if edge_alpha > 2 {
                        alpha_blend(
                            buf,
                            (py as usize * w + px as usize) * 4,
                            bar_r,
                            bar_g,
                            bar_b,
                            edge_alpha,
                        );
                    }
                }
            }
        }

        if bar_height > 10.0 {
            let tip_r = bar_inner_radius + bar_height;
            let tip_x = cx + tip_r * cos_a;
            let tip_y = cy + tip_r * sin_a;
            draw_soft_dot(
                buf,
                w,
                h,
                tip_x,
                tip_y,
                3.0 + total_energy * 2.0,
                255,
                255,
                255,
                (total_energy * 50.0) as u8,
            );
        }
    }
}

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
    let inner_radius_sq = (radius - 1.0) * (radius - 1.0);

    let edge_width = 6.0;
    let edge_inner_r_sq = (radius - edge_width) * (radius - edge_width);
    let edge_outer_r_sq = radius * radius;
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
                                ops.push((
                                    buf_idx,
                                    tex_pixels[tex_idx],
                                    tex_pixels[tex_idx + 1],
                                    tex_pixels[tex_idx + 2],
                                    (ta as f32 * edge_dist) as u8,
                                ));
                            }
                        } else {
                            ops.push((
                                buf_idx,
                                tex_pixels[tex_idx],
                                tex_pixels[tex_idx + 1],
                                tex_pixels[tex_idx + 2],
                                ta,
                            ));
                        }
                    }
                }

                if dist_sq >= edge_inner_r_sq && dist_sq <= edge_outer_r_sq {
                    let pixel_angle = fast_atan2(dy, dx);
                    let hue_raw = (pixel_angle + angle_offset) * 57.3;
                    let hue = ((hue_raw % 360.0) + 360.0) % 360.0;
                    let (ir, ig, ib) = hsl_to_rgb_simple(hue, 0.7, 0.6);
                    let edge_t = (dist_sq - edge_inner_r_sq) * inv_range_sq;
                    let fade = (edge_t * (1.0 - edge_t) * 4.0).clamp(0.0, 1.0);
                    let shimmer = fast_sin(pixel_angle * 3.0 + time * 2.0) * 0.5 + 0.5;
                    let alpha = (fade * shimmer * 35.0) as u8;
                    if alpha > 2 {
                        ops.push((buf_idx, ir, ig, ib, alpha));
                    }
                }
            }
            ops
        })
        .collect();

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
                    let a = if dist_sq > hole_inner_sq {
                        ((hole_r_sq - dist_sq) / (hole_r_sq - hole_inner_sq) * 255.0).min(255.0)
                            as u8
                    } else {
                        255
                    };
                    alpha_blend(
                        buf,
                        (py as usize * w + px as usize) * 4,
                        bg_r,
                        bg_g,
                        bg_b,
                        a,
                    );
                }
            }
        }
    }

    let ring_outer = hole_radius + 2;
    let ring_outer_sq = (ring_outer * ring_outer) as f32;
    for dy in -ring_outer..=ring_outer {
        for dx in -ring_outer..=ring_outer {
            let dist_sq = (dx * dx + dy * dy) as f32;
            if dist_sq > hole_r_sq && dist_sq <= ring_outer_sq {
                let px = cx as i32 + dx;
                let py = cy as i32 + dy;
                if px >= 0 && px < w as i32 && py >= 0 && py < h as i32 {
                    let t = (dist_sq - hole_r_sq) / (ring_outer_sq - hole_r_sq);
                    let brightness = 80 + (t * 60.0) as u8;
                    alpha_blend(
                        buf,
                        (py as usize * w + px as usize) * 4,
                        brightness,
                        brightness,
                        brightness,
                        180,
                    );
                }
            }
        }
    }
}

fn draw_specular_highlight(
    buf: &mut [u8],
    w: usize,
    h: usize,
    hl_cx: f32,
    hl_cy: f32,
    hl_radius: f32,
    disc_radius: f32,
    disc_cx: f32,
    disc_cy: f32,
) {
    let hl_r_sq = hl_radius * hl_radius;
    let disc_r_sq = disc_radius * disc_radius;
    let y_start = ((hl_cy - hl_radius) as i32).max(0) as usize;
    let y_end = ((hl_cy + hl_radius) as i32 + 1).min(h as i32) as usize;
    let x_start = ((hl_cx - hl_radius) as i32).max(0) as usize;
    let x_end = ((hl_cx + hl_radius) as i32 + 1).min(w as i32) as usize;

    for y in y_start..y_end {
        let dy = y as f32 - hl_cy;
        for x in x_start..x_end {
            let dx = x as f32 - hl_cx;
            let dist_sq = dx * dx + dy * dy;
            if dist_sq < hl_r_sq {
                let disc_dx = x as f32 - disc_cx;
                let disc_dy = y as f32 - disc_cy;
                if disc_dx * disc_dx + disc_dy * disc_dy < disc_r_sq {
                    let t = 1.0 - (dist_sq / hl_r_sq);
                    let alpha = (t * t * t * 45.0) as u8;
                    if alpha > 1 {
                        alpha_blend(buf, (y * w + x) * 4, 255, 255, 255, alpha);
                    }
                }
            }
        }
    }
}

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
    let inner_r_sq = (radius - half_w) * (radius - half_w);
    let outer_r_sq = (radius + half_w) * (radius + half_w);
    let radius_sq = radius * radius;

    let y_start = ((cy - radius - half_w) as i32).max(0) as usize;
    let y_end = ((cy + radius + half_w) as i32 + 1).min(h as i32) as usize;
    let x_start = ((cx - radius - half_w) as i32).max(0) as usize;
    let x_end = ((cx + radius + half_w) as i32 + 1).min(w as i32) as usize;

    for y in y_start..y_end {
        let dy = y as f32 - cy;
        let dy_sq = dy * dy;
        for x in x_start..x_end {
            let dx = x as f32 - cx;
            let dist_sq = dx * dx + dy_sq;
            if dist_sq >= inner_r_sq && dist_sq <= outer_r_sq {
                let ring_dist = (dist_sq - radius_sq).abs() / (2.0 * radius);
                if ring_dist <= half_w {
                    let t = 1.0 - (ring_dist / half_w);
                    let alpha = (t * t * max_alpha as f32) as u8;
                    if alpha > 2 {
                        alpha_blend(buf, (y * w + x) * 4, color.0, color.1, color.2, alpha);
                    }
                }
            }
        }
    }
}

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
        for x in x_start..x_end {
            let dx = x as f32 - cx;
            let dist_sq = dx * dx + dy * dy;
            if dist_sq < r_sq {
                let t = dist_sq / r_sq;
                let alpha = ((1.0 - t) * (1.0 - t) * max_alpha as f32) as u8;
                if alpha > 1 {
                    alpha_blend(buf, (y * w + x) * 4, r, g, b, alpha);
                }
            }
        }
    }
}
