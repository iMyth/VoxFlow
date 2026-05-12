// Vinyl disc visualization — GPU fragment shader.
//
// Renders all visual layers in a single pass per pixel:
// 1. Background gradient + vignette
// 2. Bokeh particles (soft dots)
// 3. Outer glow rings (bass/mid reactive)
// 4. EQ spectrum bars (radial)
// 5. Spinning disc with texture rotation
// 6. Iridescent edge shimmer
// 7. Center spindle hole
// 8. Specular highlight

// ─── Uniforms ────────────────────────────────────────────────────────────────

struct Params {
    width: f32,
    height: f32,
    disc_radius: f32,
    angle: f32,
    // Audio features (smoothed)
    rms: f32,
    low_energy: f32,
    mid_energy: f32,
    high_energy: f32,
    // Colors
    fg_r: f32,
    fg_g: f32,
    fg_b: f32,
    bg_r: f32,
    bg_g: f32,
    bg_b: f32,
    // Time / frame
    frame_time: f32,
    pulse_scale: f32,
    // EQ rotation offset
    eq_rotation: f32,
    _pad1: f32,
    _pad2: f32,
    _pad3: f32,
};

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var disc_texture: texture_2d<f32>;
@group(0) @binding(2) var disc_sampler: sampler;
@group(0) @binding(3) var<storage, read_write> output: array<u32>;

// ─── Constants ───────────────────────────────────────────────────────────────

const PI: f32 = 3.14159265;
const TAU: f32 = 6.28318530;
const NUM_EQ_BARS: u32 = 32u;
const NUM_BOKEH: u32 = 20u;

// ─── Entry Point ─────────────────────────────────────────────────────────────

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let px = global_id.x;
    let py = global_id.y;
    let w = u32(params.width);
    let h = u32(params.height);

    if (px >= w || py >= h) {
        return;
    }

    let idx = py * w + px;
    let x = f32(px);
    let y = f32(py);
    let cx = params.width / 2.0;
    let cy = params.height / 2.0;
    let dx = x - cx;
    let dy = y - cy;
    let dist_sq = dx * dx + dy * dy;
    let dist = sqrt(dist_sq);

    // ─── Layer 1: Background gradient + vignette ─────────────────────────
    let max_dist = sqrt(cx * cx + cy * cy);
    let norm_dist = dist / max_dist;
    let vignette = norm_dist * norm_dist * (3.0 - 2.0 * norm_dist);

    let center_r = min(params.bg_r * 1.4, 1.0);
    let center_g = min(params.bg_g * 1.4, 1.0);
    let center_b = min(params.bg_b * 1.4, 1.0);
    let edge_r = params.bg_r * 0.4;
    let edge_g = params.bg_g * 0.4;
    let edge_b = params.bg_b * 0.4;

    var color = vec3<f32>(
        mix(center_r, edge_r, vignette),
        mix(center_g, edge_g, vignette),
        mix(center_b, edge_b, vignette),
    );

    // ─── Layer 2: Bokeh particles ────────────────────────────────────────
    let phi: f32 = 1.618034;
    for (var i = 0u; i < NUM_BOKEH; i = i + 1u) {
        let t = f32(i) * phi;
        let bx = fract(sin(t * 127.1) * 0.5 + 0.5) * params.width;
        let by = fract(cos(t * 311.7) * 0.5 + 0.5) * params.height;
        let bvx = sin(t * 73.3) * 0.3;
        let bvy = cos(t * 43.7) * 0.2 - 0.1;
        let b_radius = 3.0 + abs(sin(t * 17.1)) * 12.0;
        let b_alpha_base = 0.03 + abs(cos(t * 7.3)) * 0.06;

        let bpx = (bx + bvx * params.frame_time) % params.width;
        let bpy = ((by + bvy * params.frame_time) % params.height + params.height) % params.height;

        let bdx = x - bpx;
        let bdy = y - bpy;
        let b_dist_sq = bdx * bdx + bdy * bdy;
        let b_r_sq = b_radius * b_radius;

        if (b_dist_sq < b_r_sq) {
            let bt = b_dist_sq / b_r_sq;
            let falloff = (1.0 - bt) * (1.0 - bt);
            let pulse = 1.0 + params.rms * 0.5;
            let b_alpha = b_alpha_base * pulse * falloff;
            let b_color = vec3<f32>(
                mix(params.fg_r, 1.0, 0.3),
                mix(params.fg_g, 1.0, 0.3),
                mix(params.fg_b, 1.0, 0.3),
            );
            color = mix(color, b_color, b_alpha);
        }
    }

    // ─── Layer 3: Outer glow rings ───────────────────────────────────────
    let disc_r = params.disc_radius * params.pulse_scale;

    // Bass ring
    let bass_ring_r = disc_r + 20.0 + params.low_energy * 25.0;
    let bass_ring_w = 3.0 + params.low_energy * 8.0;
    let bass_ring_alpha = params.low_energy * 0.4;
    color = apply_ring(color, dist, bass_ring_r, bass_ring_w,
        vec3<f32>(params.fg_r, params.fg_g, params.fg_b), bass_ring_alpha);

    // Mid ring
    let mid_ring_r = disc_r + 12.0 + params.mid_energy * 12.0;
    let mid_ring_w = 2.0 + params.mid_energy * 5.0;
    let mid_ring_alpha = params.mid_energy * 0.28;
    let mid_color = vec3<f32>(
        mix(params.fg_r, 1.0, 0.3),
        mix(params.fg_g, 1.0, 0.3),
        mix(params.fg_b, 1.0, 0.3),
    );
    color = apply_ring(color, dist, mid_ring_r, mid_ring_w, mid_color, mid_ring_alpha);

    // ─── Layer 4: EQ spectrum bars ───────────────────────────────────────
    let pixel_angle = atan2(dy, dx);
    let bar_inner_r = disc_r + 28.0;
    let bar_max_height = disc_r * 0.55;

    if (dist > bar_inner_r && dist < bar_inner_r + bar_max_height) {
        // Find which bar this pixel belongs to
        let norm_angle = (pixel_angle - params.eq_rotation + TAU) % TAU;
        let bar_idx_f = norm_angle / TAU * f32(NUM_EQ_BARS);
        let bar_idx = u32(bar_idx_f);
        let bar_frac = fract(bar_idx_f);

        // Bar width check (center 70% of each bar slot)
        if (bar_frac > 0.15 && bar_frac < 0.85) {
            let band_t = f32(bar_idx) / f32(NUM_EQ_BARS) * 3.0;
            var energy: f32;
            if (band_t < 1.0) {
                energy = mix(params.low_energy, params.mid_energy, band_t);
            } else if (band_t < 2.0) {
                energy = mix(params.mid_energy, params.high_energy, band_t - 1.0);
            } else {
                energy = mix(params.high_energy, params.low_energy, band_t - 2.0);
            }

            let total_energy = clamp(energy * 0.7 + params.rms * 0.3, 0.0, 1.0);
            let bar_height = bar_max_height * total_energy * (0.4 + params.rms * 0.6);
            let bar_dist = dist - bar_inner_r;

            if (bar_dist < bar_height) {
                let t = bar_dist / bar_height;
                let bar_color = vec3<f32>(
                    mix(params.fg_r, 1.0, t * t * 0.7),
                    mix(params.fg_g, 1.0, t * t * 0.7),
                    mix(params.fg_b, 1.0, t * t * 0.7),
                );
                let bar_alpha = (1.0 - t * t * 0.5) * 0.86;
                color = mix(color, bar_color, bar_alpha);
            }
        }
    }

    // ─── Layer 5: Spinning disc with texture ─────────────────────────────
    if (dist < disc_r) {
        // Rotate UV coordinates
        let cos_a = cos(params.angle);
        let sin_a = sin(params.angle);
        let inv_scale = 1.0 / params.pulse_scale;
        let udx = dx * inv_scale;
        let udy = dy * inv_scale;
        let tex_u = (udx * cos_a + udy * sin_a) / (params.disc_radius * 2.0) + 0.5;
        let tex_v = (-udx * sin_a + udy * cos_a) / (params.disc_radius * 2.0) + 0.5;

        if (tex_u >= 0.0 && tex_u <= 1.0 && tex_v >= 0.0 && tex_v <= 1.0) {
            let tex_color = textureSampleLevel(disc_texture, disc_sampler, vec2<f32>(tex_u, tex_v), 0.0);

            if (tex_color.a > 0.0) {
                // Anti-aliased disc edge
                let edge_fade = clamp(disc_r - dist, 0.0, 1.5) / 1.5;
                let final_alpha = tex_color.a * edge_fade;
                color = mix(color, tex_color.rgb, final_alpha);
            }
        }

        // ─── Layer 6: Iridescent edge shimmer ────────────────────────────
        let edge_width = 6.0;
        let edge_inner = disc_r - edge_width;
        if (dist > edge_inner && dist < disc_r) {
            let edge_t = (dist - edge_inner) / edge_width;
            let fade = edge_t * (1.0 - edge_t) * 4.0;
            let shimmer_input = pixel_angle * 3.0 + params.frame_time * 0.06;
            let shimmer = sin(shimmer_input) * 0.5 + 0.5;
            let hue_raw = (pixel_angle + params.angle * 0.5 + params.frame_time * 0.03) * 57.3;
            let hue = ((hue_raw % 360.0) + 360.0) % 360.0;
            let iri_color = hsl_to_rgb_gpu(hue, 0.7, 0.6);
            let iri_alpha = fade * shimmer * 0.14;
            color = mix(color, iri_color, iri_alpha);
        }

        // ─── Layer 7: Center spindle hole ────────────────────────────────
        let hole_radius = params.disc_radius * 0.025;
        if (dist < hole_radius + 2.0) {
            if (dist < hole_radius) {
                let hole_edge = clamp(hole_radius - dist, 0.0, 1.5) / 1.5;
                color = mix(color, vec3<f32>(params.bg_r, params.bg_g, params.bg_b), hole_edge);
            } else {
                // Metallic ring
                let ring_t = (dist - hole_radius) / 2.0;
                let brightness = 0.31 + ring_t * 0.24;
                color = mix(color, vec3<f32>(brightness, brightness, brightness), 0.7);
            }
        }
    }

    // ─── Layer 8: Specular highlight ─────────────────────────────────────
    let hl_angle = params.angle * 0.05;
    let hl_offset_x = cos(hl_angle) * disc_r * 0.15;
    let hl_offset_y = sin(hl_angle) * disc_r * 0.15;
    let hl_cx = cx - disc_r * 0.2 + hl_offset_x;
    let hl_cy = cy - disc_r * 0.2 + hl_offset_y;
    let hl_radius = disc_r * 0.35;
    let hl_dx = x - hl_cx;
    let hl_dy = y - hl_cy;
    let hl_dist_sq = hl_dx * hl_dx + hl_dy * hl_dy;
    let hl_r_sq = hl_radius * hl_radius;

    if (hl_dist_sq < hl_r_sq && dist < disc_r) {
        let hl_t = 1.0 - (hl_dist_sq / hl_r_sq);
        let hl_alpha = hl_t * hl_t * hl_t * 0.18;
        color = mix(color, vec3<f32>(1.0, 1.0, 1.0), hl_alpha);
    }

    // ─── Layer 9: Inner glow ring ────────────────────────────────────────
    let inner_ring_r = disc_r * 0.92;
    let inner_ring_alpha = params.high_energy * 0.2;
    color = apply_ring(color, dist, inner_ring_r, 2.0, vec3<f32>(1.0, 1.0, 1.0), inner_ring_alpha);

    // ─── Output ──────────────────────────────────────────────────────────
    let out_r = u32(clamp(color.x, 0.0, 1.0) * 255.0);
    let out_g = u32(clamp(color.y, 0.0, 1.0) * 255.0);
    let out_b = u32(clamp(color.z, 0.0, 1.0) * 255.0);
    output[idx] = out_r | (out_g << 8u) | (out_b << 16u) | (255u << 24u);
}

// ─── Helper Functions ────────────────────────────────────────────────────────

fn apply_ring(base: vec3<f32>, dist: f32, ring_r: f32, ring_w: f32, ring_color: vec3<f32>, max_alpha: f32) -> vec3<f32> {
    let half_w = ring_w / 2.0;
    let ring_dist = abs(dist - ring_r);
    if (ring_dist < half_w) {
        let t = 1.0 - (ring_dist / half_w);
        let alpha = t * t * max_alpha;
        return mix(base, ring_color, alpha);
    }
    return base;
}

fn hsl_to_rgb_gpu(h: f32, s: f32, l: f32) -> vec3<f32> {
    let c = (1.0 - abs(2.0 * l - 1.0)) * s;
    let h_prime = h / 60.0;
    let x = c * (1.0 - abs(h_prime % 2.0 - 1.0));
    let m = l - c / 2.0;

    var r1: f32 = 0.0;
    var g1: f32 = 0.0;
    var b1: f32 = 0.0;

    if (h_prime < 1.0) {
        r1 = c; g1 = x; b1 = 0.0;
    } else if (h_prime < 2.0) {
        r1 = x; g1 = c; b1 = 0.0;
    } else if (h_prime < 3.0) {
        r1 = 0.0; g1 = c; b1 = x;
    } else if (h_prime < 4.0) {
        r1 = 0.0; g1 = x; b1 = c;
    } else if (h_prime < 5.0) {
        r1 = x; g1 = 0.0; b1 = c;
    } else {
        r1 = c; g1 = 0.0; b1 = x;
    }

    return vec3<f32>(r1 + m, g1 + m, b1 + m);
}
