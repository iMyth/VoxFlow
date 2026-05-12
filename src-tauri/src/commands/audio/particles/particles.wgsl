// Particle kaleidoscope compute shader — GPU-accelerated rendering.
//
// CPU handles particle physics (spawn, update, remove).
// GPU handles rendering: for each pixel, check all particles with
// kaleidoscope symmetry and alpha-blend them onto the background.
//
// This is an "inverse" approach: instead of drawing each particle onto pixels,
// each pixel checks which particles affect it. This is efficient on GPU because
// all pixels run in parallel.

// ─── Uniforms ────────────────────────────────────────────────────────────────

struct Params {
    width: u32,
    height: u32,
    num_particles: u32,
    num_rings: u32,
    symmetry_folds: u32,
    frame_idx: u32,
    _pad0: u32,
    _pad1: u32,
    // Audio features
    rms: f32,
    low_energy: f32,
    mid_energy: f32,
    high_energy: f32,
    // Colors
    bg_r: f32,
    bg_g: f32,
    bg_b: f32,
    base_hue: f32,
};

// Particle data (packed for GPU transfer)
struct ParticleData {
    x: f32,
    y: f32,
    size: f32,
    life: f32,
    hue: f32,
    saturation: f32,
    brightness: f32,
    kind: u32,  // 0=Dot, 1=Glow, 2=Spark
};

// Ring data
struct RingData {
    radius: f32,
    thickness: f32,
    life: f32,
    hue: f32,
    brightness: f32,
    _pad0: f32,
    _pad1: f32,
    _pad2: f32,
};

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var<storage, read> particles: array<ParticleData>;
@group(0) @binding(2) var<storage, read> rings: array<RingData>;
@group(0) @binding(3) var<storage, read_write> output: array<u32>;

// ─── Constants ───────────────────────────────────────────────────────────────

const PI: f32 = 3.14159265;
const TAU: f32 = 6.28318530;

// ─── Entry Point ─────────────────────────────────────────────────────────────

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let px = global_id.x;
    let py = global_id.y;

    if (px >= params.width || py >= params.height) {
        return;
    }

    let idx = py * params.width + px;
    let w = f32(params.width);
    let h = f32(params.height);
    let cx = w / 2.0;
    let cy = h / 2.0;
    let scale = max(w, h) / 2.0;

    // Pixel position in normalized space (-1..1 roughly)
    let norm_x = (f32(px) - cx) / scale;
    let norm_y = (f32(py) - cy) / scale;

    // ─── Layer 1: Background gradient ────────────────────────────────────
    let max_dist_sq = (cx * cx + cy * cy);
    let pixel_dx = f32(px) - cx;
    let pixel_dy = f32(py) - cy;
    let pixel_dist_sq = pixel_dx * pixel_dx + pixel_dy * pixel_dy;
    let bg_t = min(pixel_dist_sq / max_dist_sq, 1.0);

    let center_r = min(params.bg_r * 1.3, 1.0);
    let center_g = min(params.bg_g * 1.3, 1.0);
    let center_b = min(params.bg_b * 1.3, 1.0);

    var color = vec3<f32>(
        mix(center_r, params.bg_r, bg_t),
        mix(center_g, params.bg_g, bg_t),
        mix(center_b, params.bg_b, bg_t),
    );

    // ─── Layer 2: Rings ──────────────────────────────────────────────────
    let pixel_dist = sqrt(pixel_dist_sq);
    let pixel_norm_dist = pixel_dist / scale;

    for (var ri = 0u; ri < params.num_rings; ri = ri + 1u) {
        let ring = rings[ri];
        let ring_screen_r = ring.radius * scale;
        let half_thick = ring.thickness / 2.0;
        let ring_dist = abs(pixel_dist - ring_screen_r);

        if (ring_dist < half_thick) {
            let edge_alpha = 1.0 - (ring_dist / half_thick);
            let alpha = edge_alpha * ring.life * ring.brightness * 0.7;
            let ring_color = hsl_to_rgb_gpu(ring.hue, 0.8, 0.6);
            color = mix(color, ring_color, alpha);
        }
    }

    // ─── Layer 3: Particles with kaleidoscope symmetry ───────────────────
    let angle_step = TAU / f32(params.symmetry_folds);

    // Limit particles processed per pixel for performance.
    // With 5000 particles × 8 folds × 2 mirrors = 80K checks per pixel is too much.
    // Cap at 512 particles — they are sorted by life (most visible first) on CPU side.
    let max_particles = min(params.num_particles, 512u);

    for (var pi = 0u; pi < max_particles; pi = pi + 1u) {
        let p = particles[pi];

        let alpha_base = p.life * p.brightness;
        if (alpha_base < 0.02) {
            continue;
        }

        // Size in normalized space — use as bounding radius for early-out
        let size_norm = p.size / scale;
        let size_norm_sq = size_norm * size_norm;
        // Max check radius (including glow)
        let check_radius = select(size_norm, size_norm * 1.5, p.kind == 1u);
        let check_radius_sq = check_radius * check_radius;

        // Early-out: if particle center is too far from pixel even without rotation,
        // skip it entirely. The max distance after rotation is the particle's distance
        // from origin + check_radius. If pixel is farther from origin than that, skip.
        let p_dist = sqrt(p.x * p.x + p.y * p.y);
        let pixel_dist_norm = sqrt(norm_x * norm_x + norm_y * norm_y);
        if (abs(pixel_dist_norm - p_dist) > check_radius + 0.01) {
            continue;
        }

        // Glow particles have a larger soft radius
        var glow_size_norm_sq = size_norm_sq;
        if (p.kind == 1u) {
            glow_size_norm_sq = size_norm_sq * 2.25; // 1.5x radius for glow
        }

        // Check all symmetry folds
        for (var fold = 0u; fold < params.symmetry_folds; fold = fold + 1u) {
            let angle = f32(fold) * angle_step;
            let cos_a = cos(angle);
            let sin_a = sin(angle);

            // Rotated particle position
            let rx = p.x * cos_a - p.y * sin_a;
            let ry = p.x * sin_a + p.y * cos_a;

            // Two mirror reflections per fold
            let positions = array<vec2<f32>, 2>(
                vec2<f32>(rx, ry),
                vec2<f32>(rx, -ry),
            );

            for (var mi = 0u; mi < 2u; mi = mi + 1u) {
                let pos = positions[mi];
                let dx = norm_x - pos.x;
                let dy = norm_y - pos.y;
                let dist_sq = dx * dx + dy * dy;

                // Early-out per-instance
                if (dist_sq > check_radius_sq) {
                    continue;
                }

                // Glow layer (larger, softer)
                if (p.kind == 1u && dist_sq < glow_size_norm_sq) {
                    let t = dist_sq / glow_size_norm_sq;
                    let falloff = (1.0 - t) * (1.0 - t);
                    let glow_alpha = falloff * alpha_base * 0.25;
                    let glow_color = hsl_to_rgb_gpu(p.hue, p.saturation, 0.45 + p.life * 0.2);
                    color = mix(color, glow_color, glow_alpha);
                }

                // Core particle
                if (dist_sq < size_norm_sq) {
                    let t = dist_sq / size_norm_sq;
                    // Smooth edge falloff
                    let edge_dist = sqrt(t);
                    var falloff: f32;
                    if (edge_dist > 0.7) {
                        falloff = (1.0 - edge_dist) / 0.3;
                    } else {
                        falloff = 1.0;
                    }

                    var lightness: f32;
                    if (p.kind == 1u) {
                        lightness = 0.45 + p.life * 0.2;
                    } else if (p.kind == 2u) {
                        lightness = 0.7 + p.life * 0.2;
                    } else {
                        lightness = 0.5 + p.life * 0.15;
                    }

                    let p_color = hsl_to_rgb_gpu(p.hue, p.saturation, lightness);
                    let alpha = falloff * alpha_base;
                    color = mix(color, p_color, min(alpha, 1.0));
                }
            }
        }
    }

    // ─── Layer 4: Center glow ────────────────────────────────────────────
    if (params.rms > 0.05) {
        let glow_intensity = params.rms * 0.7 + params.low_energy * 0.3;
        let glow_radius = (15.0 + glow_intensity * 50.0) / scale;
        let glow_r_sq = glow_radius * glow_radius;
        let center_dist_sq = norm_x * norm_x + norm_y * norm_y;

        if (center_dist_sq < glow_r_sq) {
            let t = center_dist_sq / glow_r_sq;
            let falloff = (1.0 - t) * (1.0 - t);

            let time_hue = (params.base_hue + f32(params.frame_idx) * 0.5) % 360.0;
            let glow_color = hsl_to_rgb_gpu(time_hue, 0.7, 0.65);
            let glow_alpha = falloff * glow_intensity * 0.4;
            color = mix(color, glow_color, glow_alpha);

            // White core
            let core_radius = glow_radius * 0.33;
            let core_r_sq = core_radius * core_radius;
            if (center_dist_sq < core_r_sq) {
                let core_t = center_dist_sq / core_r_sq;
                let core_falloff = (1.0 - core_t) * (1.0 - core_t);
                let core_alpha = core_falloff * glow_intensity * 0.6;
                color = mix(color, vec3<f32>(1.0, 1.0, 1.0), core_alpha);
            }
        }
    }

    // ─── Output ──────────────────────────────────────────────────────────
    let out_r = u32(clamp(color.x, 0.0, 1.0) * 255.0);
    let out_g = u32(clamp(color.y, 0.0, 1.0) * 255.0);
    let out_b = u32(clamp(color.z, 0.0, 1.0) * 255.0);
    output[idx] = out_r | (out_g << 8u) | (out_b << 16u) | (255u << 24u);
}

// ─── Helper Functions ────────────────────────────────────────────────────────

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
