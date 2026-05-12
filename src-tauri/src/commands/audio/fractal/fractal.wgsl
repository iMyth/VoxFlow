// Mandelbrot fractal compute shader — GPU-accelerated rendering.
//
// Each invocation computes one pixel. The shader performs:
// 1. Cardioid/period-2 bulb early-out
// 2. Mandelbrot iteration with smooth coloring
// 3. Audio-reactive palette generation

struct Params {
    width: u32,
    height: u32,
    max_iter: u32,
    _pad0: u32,
    center_x: f32,
    center_y: f32,
    scale: f32,
    aspect: f32,
    fg_r: f32,
    fg_g: f32,
    fg_b: f32,
    hue_offset: f32,
    glow_intensity: f32,
    brightness_boost: f32,
    bg_r: f32,
    bg_g: f32,
    bg_b: f32,
    _pad1: f32,
    _pad2: f32,
    _pad3: f32,
};

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var<storage, read_write> output: array<u32>;

// Use f64 emulation via two f32 (double-single) for deep zoom precision.
// For moderate zoom depths (< 1e12), single f32 is sufficient.
// We use f32 here for maximum GPU compatibility and speed.

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let px = global_id.x;
    let py = global_id.y;

    if (px >= params.width || py >= params.height) {
        return;
    }

    let idx = py * params.width + px;

    let x_min = params.center_x - params.scale * params.aspect * 0.5;
    let y_min = params.center_y - params.scale * 0.5;
    let x_step = params.scale * params.aspect / f32(params.width);
    let y_step = params.scale / f32(params.height);

    let cr = x_min + f32(px) * x_step;
    let ci = y_min + f32(py) * y_step;

    // Cardioid check
    let q = (cr - 0.25) * (cr - 0.25) + ci * ci;
    if (q * (q + (cr - 0.25)) <= 0.25 * ci * ci) {
        output[idx] = pack_color(params.bg_r, params.bg_g, params.bg_b, 1.0);
        return;
    }

    // Period-2 bulb check
    if ((cr + 1.0) * (cr + 1.0) + ci * ci <= 0.0625) {
        output[idx] = pack_color(params.bg_r, params.bg_g, params.bg_b, 1.0);
        return;
    }

    // Mandelbrot iteration
    var zr: f32 = 0.0;
    var zi: f32 = 0.0;
    var zr2: f32 = 0.0;
    var zi2: f32 = 0.0;
    var iter: u32 = 0u;
    let bailout_sq: f32 = 65536.0;
    let max_iter = params.max_iter;

    loop {
        if (zr2 + zi2 > bailout_sq || iter >= max_iter) {
            break;
        }
        zi = 2.0 * zr * zi + ci;
        zr = zr2 - zi2 + cr;
        zr2 = zr * zr;
        zi2 = zi * zi;
        iter = iter + 1u;
    }

    if (iter >= max_iter) {
        output[idx] = pack_color(params.bg_r, params.bg_g, params.bg_b, 1.0);
    } else {
        // Smooth coloring
        let log_zn = log(zr2 + zi2) * 0.5;
        let nu = log(log_zn / log(2.0)) / log(2.0);
        let smooth_val = max(f32(iter) + 1.0 - nu, 0.0);
        let t = smooth_val / f32(max_iter);

        let color = fractal_palette(t);
        output[idx] = pack_color(color.x, color.y, color.z, 1.0);
    }
}

fn fractal_palette(t: f32) -> vec3<f32> {
    let phase = t * 4.0 + params.hue_offset * 0.01;
    let tau = 6.283185307;

    let base_r = params.fg_r;
    let base_g = params.fg_g;
    let base_b = params.fg_b;

    var r = 0.5 + 0.5 * cos(tau * (phase * 1.0 + base_r * 0.5));
    var g = 0.5 + 0.5 * cos(tau * (phase * 0.8 + base_g * 0.5 + 0.1));
    var b = 0.5 + 0.5 * cos(tau * (phase * 0.6 + base_b * 0.5 + 0.2));

    // Edge glow
    var edge_glow: f32 = 0.0;
    if (t < 0.1) {
        edge_glow = (1.0 - t * 10.0) * params.glow_intensity;
    }

    r = clamp((r + edge_glow) * params.brightness_boost, 0.0, 1.0);
    g = clamp((g + edge_glow) * params.brightness_boost, 0.0, 1.0);
    b = clamp((b + edge_glow) * params.brightness_boost, 0.0, 1.0);

    return vec3<f32>(r, g, b);
}

fn pack_color(r: f32, g: f32, b: f32, a: f32) -> u32 {
    let ri = u32(r * 255.0);
    let gi = u32(g * 255.0);
    let bi = u32(b * 255.0);
    let ai = u32(a * 255.0);
    return ri | (gi << 8u) | (bi << 16u) | (ai << 24u);
}
