//! Shared utility functions for vinyl CPU rendering.

/// Alpha-blend a color onto the buffer at the given index.
#[inline]
pub(super) fn alpha_blend(buf: &mut [u8], idx: usize, r: u8, g: u8, b: u8, alpha: u8) {
    let a = alpha as u32;
    let inv_a = 255 - a;
    buf[idx] = ((r as u32 * a + buf[idx] as u32 * inv_a) >> 8) as u8;
    buf[idx + 1] = ((g as u32 * a + buf[idx + 1] as u32 * inv_a) >> 8) as u8;
    buf[idx + 2] = ((b as u32 * a + buf[idx + 2] as u32 * inv_a) >> 8) as u8;
}

/// Linear interpolation between two u8 values.
#[inline]
pub(super) fn lerp_u8(a: u8, b: u8, t: f32) -> u8 {
    (a as f32 + (b as f32 - a as f32) * t).clamp(0.0, 255.0) as u8
}

/// Linear interpolation between two f32 values.
#[inline]
pub(super) fn lerp_f32(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Fast atan2 approximation (max error ~0.01 rad).
#[inline]
pub(super) fn fast_atan2(y: f32, x: f32) -> f32 {
    if x == 0.0 && y == 0.0 {
        return 0.0;
    }

    let ax = x.abs();
    let ay = y.abs();

    let (a, offset) = if ax >= ay {
        (ay / ax, 0.0_f32)
    } else {
        (ax / ay, std::f32::consts::FRAC_PI_2)
    };

    let s = a * (std::f32::consts::FRAC_PI_4 + 0.273 * (1.0 - a));
    let mut r = if ax >= ay { s } else { offset - s };

    if x < 0.0 {
        r = std::f32::consts::PI - r;
    }
    if y < 0.0 {
        r = -r;
    }
    r
}

/// Fast sine approximation using parabolic method.
#[inline]
pub(super) fn fast_sin(x: f32) -> f32 {
    let mut x = x % std::f32::consts::TAU;
    if x > std::f32::consts::PI {
        x -= std::f32::consts::TAU;
    } else if x < -std::f32::consts::PI {
        x += std::f32::consts::TAU;
    }

    let b = 4.0 / std::f32::consts::PI;
    let c = -4.0 / (std::f32::consts::PI * std::f32::consts::PI);
    let y = b * x + c * x * x.abs();

    let p = 0.225;
    p * (y * y.abs() - y) + y
}

/// Simple HSL to RGB conversion.
pub(super) fn hsl_to_rgb_simple(h: f32, s: f32, l: f32) -> (u8, u8, u8) {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let h_prime = h / 60.0;
    let x = c * (1.0 - (h_prime % 2.0 - 1.0).abs());
    let m = l - c / 2.0;

    let (r1, g1, b1) = if h_prime < 1.0 {
        (c, x, 0.0)
    } else if h_prime < 2.0 {
        (x, c, 0.0)
    } else if h_prime < 3.0 {
        (0.0, c, x)
    } else if h_prime < 4.0 {
        (0.0, x, c)
    } else if h_prime < 5.0 {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };

    (
        ((r1 + m) * 255.0).clamp(0.0, 255.0) as u8,
        ((g1 + m) * 255.0).clamp(0.0, 255.0) as u8,
        ((b1 + m) * 255.0).clamp(0.0, 255.0) as u8,
    )
}

/// Pre-compute the background with a rich radial gradient and subtle vignette.
pub(super) fn precompute_vinyl_bg(w: usize, h: usize, bg_color: (u8, u8, u8)) -> Vec<u8> {
    let cx = w as f32 / 2.0;
    let cy = h as f32 / 2.0;
    let max_dist = (cx * cx + cy * cy).sqrt();
    let (bg_r, bg_g, bg_b) = bg_color;

    let center_r = ((bg_r as f32 * 1.4).min(255.0)) as u8;
    let center_g = ((bg_g as f32 * 1.4).min(255.0)) as u8;
    let center_b = ((bg_b as f32 * 1.4).min(255.0)) as u8;

    let edge_r = (bg_r as f32 * 0.4) as u8;
    let edge_g = (bg_g as f32 * 0.4) as u8;
    let edge_b = (bg_b as f32 * 0.4) as u8;

    let mut buf = vec![0u8; w * h * 4];
    for y in 0..h {
        let dy = y as f32 - cy;
        let dy_sq = dy * dy;
        for x in 0..w {
            let dx = x as f32 - cx;
            let dist = (dx * dx + dy_sq).sqrt();
            let t = (dist / max_dist).min(1.0);
            let vignette = t * t * (3.0 - 2.0 * t);

            let idx = (y * w + x) * 4;
            buf[idx] = lerp_u8(center_r, edge_r, vignette);
            buf[idx + 1] = lerp_u8(center_g, edge_g, vignette);
            buf[idx + 2] = lerp_u8(center_b, edge_b, vignette);
            buf[idx + 3] = 255;
        }
    }
    buf
}
