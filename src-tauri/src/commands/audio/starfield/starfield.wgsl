// Starfield tunnel compute shader — GPU-accelerated rendering.
//
// Renders a "flying through space" effect with:
// 1. Deep space background with subtle nebula
// 2. Stars as bright points with depth-based size and trails
// 3. Central warp tunnel glow
// 4. Audio-reactive speed lines

struct Params {
    width: u32,
    height: u32,
    num_stars: u32,
    _pad0: u32,
    rms: f32,
    low_energy: f32,
    mid_energy: f32,
    high_energy: f32,
    fg_r: f32,
    fg_g: f32,
    fg_b: f32,
    bg_r: f32,
    bg_g: f32,
    bg_b: f32,
    frame_time: f32,
    warp_intensity: f32,
};

// Star data from CPU
struct StarData {
    // Current screen position
    sx: f32,
    sy: f32,
    // Previous screen position (for trail)
    prev_sx: f32,
    prev_sy: f32,
    // Depth factor (0=far, 1=near)
    depth: f32,
    // Color hue
    hue: f32,
    // Brightness
    brightness: f32,
    // Has previous position (for trail)
    has_trail: f32,
};

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var<storage, read> stars: array<StarData>;
@group(0) @binding(2) var<storage, read_write> output: array<u32>;

const PI: f32 = 3.14159265;
const TAU: f32 = 6.28318530;

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

    let x = f32(px);
    let y = f32(py);
    let dx = x - cx;
    let dy = y - cy;
    let dist = sqrt(dx * dx + dy * dy);
    let max_dist = sqrt(cx * cx + cy * cy);
    let norm_dist = dist / max_dist;

    // ─── Layer 1: Deep space background ──────────────────────────────────
    let vignette = norm_dist * norm_dist;
    var color = vec3<f32>(
        params.bg_r * (1.0 - vignette * 0.6),
        params.bg_g * (1.0 - vignette * 0.6),
        params.bg_b * (1.0 - vignette * 0.6),
    );

    // Subtle nebula effect (procedural noise-like pattern)
    let nebula_angle = atan2(dy, dx);
    let nebula_phase = nebula_angle * 2.0 + params.frame_time * 0.005;
    let nebula_r = sin(nebula_phase * 1.3 + norm_dist * 4.0) * 0.5 + 0.5;
    let nebula_g = sin(nebula_phase * 0.7 + norm_dist * 3.0 + 1.0) * 0.5 + 0.5;
    let nebula_b = sin(nebula_phase * 1.1 + norm_dist * 5.0 + 2.0) * 0.5 + 0.5;
    let nebula_intensity = (1.0 - norm_dist) * 0.03 * (1.0 + params.mid_energy * 0.5);
    let nebula_color = vec3<f32>(
        params.fg_r * nebula_r,
        params.fg_g * nebula_g,
        params.fg_b * nebula_b,
    );
    color = color + nebula_color * nebula_intensity;

    // ─── Layer 2: Warp speed lines (radial streaks) ──────────────────────
    if (params.warp_intensity > 0.05 && norm_dist > 0.1) {
        let angle = atan2(dy, dx);
        // Create radial streaks using high-frequency angular pattern
        let streak_freq = 80.0;
        let streak = abs(sin(angle * streak_freq + params.frame_time * 0.1));
        let streak_mask = pow(streak, 20.0); // Very sharp peaks
        let radial_fade = norm_dist * (1.0 - norm_dist) * 4.0; // Fade at center and edges
        let streak_alpha = streak_mask * radial_fade * params.warp_intensity * 0.15;
        let streak_color = vec3<f32>(
            mix(params.fg_r, 1.0, 0.5),
            mix(params.fg_g, 1.0, 0.5),
            mix(params.fg_b, 1.0, 0.7),
        );
        color = mix(color, streak_color, streak_alpha);
    }

    // ─── Layer 3: Stars ──────────────────────────────────────────────────
    // Cap at 600 stars per pixel to bound GPU workload
    let max_stars = min(params.num_stars, 600u);
    for (var i = 0u; i < max_stars; i = i + 1u) {
        let star = stars[i];

        if (star.depth < 0.01) {
            continue;
        }

        // Star core
        let sdx = x - star.sx;
        let sdy = y - star.sy;
        let star_dist_sq = sdx * sdx + sdy * sdy;

        // Star radius based on depth (near = bigger)
        let star_radius = 1.0 + star.depth * star.depth * 4.0;
        let star_r_sq = star_radius * star_radius;

        if (star_dist_sq < star_r_sq * 4.0) { // Check within glow radius
            let star_color = star_hsl_to_rgb(star.hue, 0.6, 0.8);
            let white = vec3<f32>(1.0, 1.0, 1.0);
            let final_star_color = mix(white, star_color, star.depth * 0.7);

            if (star_dist_sq < star_r_sq) {
                // Core: bright white/colored
                let core_t = star_dist_sq / star_r_sq;
                let core_alpha = (1.0 - core_t) * star.brightness * star.depth;
                color = mix(color, final_star_color, min(core_alpha, 1.0));
            } else {
                // Glow halo
                let glow_t = (star_dist_sq - star_r_sq) / (star_r_sq * 3.0);
                let glow_alpha = (1.0 - glow_t) * star.brightness * star.depth * 0.3;
                if (glow_alpha > 0.01) {
                    color = mix(color, final_star_color, glow_alpha);
                }
            }
        }

        // Trail (line from prev to current)
        if (star.has_trail > 0.5 && star.depth > 0.15) {
            let trail_alpha = point_to_line_dist(x, y, star.prev_sx, star.prev_sy, star.sx, star.sy, 1.5);
            if (trail_alpha > 0.01) {
                let trail_color = mix(vec3<f32>(1.0, 1.0, 1.0), star_hsl_to_rgb(star.hue, 0.5, 0.7), 0.4);
                color = mix(color, trail_color, trail_alpha * star.depth * star.brightness * 0.5);
            }
        }
    }

    // ─── Layer 4: Central warp glow ──────────────────────────────────────
    let glow_radius = 0.15 + params.rms * 0.1 + params.low_energy * 0.08;
    if (norm_dist < glow_radius) {
        let glow_t = norm_dist / glow_radius;
        let glow_falloff = (1.0 - glow_t) * (1.0 - glow_t);
        let glow_alpha = glow_falloff * (0.2 + params.rms * 0.3);
        let glow_color = vec3<f32>(
            mix(params.fg_r, 1.0, 0.6),
            mix(params.fg_g, 1.0, 0.6),
            mix(params.fg_b, 1.0, 0.8),
        );
        color = mix(color, glow_color, glow_alpha);
    }

    // ─── Output ──────────────────────────────────────────────────────────
    let out_r = u32(clamp(color.x, 0.0, 1.0) * 255.0);
    let out_g = u32(clamp(color.y, 0.0, 1.0) * 255.0);
    let out_b = u32(clamp(color.z, 0.0, 1.0) * 255.0);
    output[idx] = out_r | (out_g << 8u) | (out_b << 16u) | (255u << 24u);
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn star_hsl_to_rgb(h: f32, s: f32, l: f32) -> vec3<f32> {
    let c = (1.0 - abs(2.0 * l - 1.0)) * s;
    let h_prime = h / 60.0;
    let x = c * (1.0 - abs(h_prime % 2.0 - 1.0));
    let m = l - c / 2.0;

    var r1: f32 = 0.0;
    var g1: f32 = 0.0;
    var b1: f32 = 0.0;

    if (h_prime < 1.0) { r1 = c; g1 = x; }
    else if (h_prime < 2.0) { r1 = x; g1 = c; }
    else if (h_prime < 3.0) { g1 = c; b1 = x; }
    else if (h_prime < 4.0) { g1 = x; b1 = c; }
    else if (h_prime < 5.0) { r1 = x; b1 = c; }
    else { r1 = c; b1 = x; }

    return vec3<f32>(r1 + m, g1 + m, b1 + m);
}

// Distance from point to line segment, returns alpha (0-1) based on proximity
fn point_to_line_dist(px: f32, py: f32, x0: f32, y0: f32, x1: f32, y1: f32, width: f32) -> f32 {
    let line_dx = x1 - x0;
    let line_dy = y1 - y0;
    let line_len_sq = line_dx * line_dx + line_dy * line_dy;

    if (line_len_sq < 1.0) {
        return 0.0;
    }

    // Project point onto line, clamped to segment
    let t = clamp(((px - x0) * line_dx + (py - y0) * line_dy) / line_len_sq, 0.0, 1.0);
    let proj_x = x0 + t * line_dx;
    let proj_y = y0 + t * line_dy;

    let dist_sq = (px - proj_x) * (px - proj_x) + (py - proj_y) * (py - proj_y);
    let width_sq = width * width;

    if (dist_sq > width_sq) {
        return 0.0;
    }

    return 1.0 - (dist_sq / width_sq);
}
