//! Particle system with kaleidoscope symmetry.
//!
//! Particles are spawned from the center, driven by audio energy,
//! and rendered with N-fold rotational symmetry for a kaleidoscope effect.

use rand::Rng;

use super::audio_analysis::FrameFeatures;

/// A single particle in the system.
#[derive(Clone)]
pub struct Particle {
    /// Position relative to center (0,0 = center, range roughly -1..1)
    pub x: f32,
    pub y: f32,
    /// Velocity
    pub vx: f32,
    pub vy: f32,
    /// Remaining lifetime (0..1, dies at 0)
    pub life: f32,
    /// Decay rate per frame
    pub decay: f32,
    /// Size (radius in pixels)
    pub size: f32,
    /// Hue (0..360)
    pub hue: f32,
    /// Saturation (0..1)
    pub saturation: f32,
    /// Brightness/alpha (0..1)
    pub brightness: f32,
}

/// Configuration for the particle system.
#[allow(dead_code)]
pub struct ParticleConfig {
    /// Number of kaleidoscope symmetry folds (e.g. 6, 8, 12)
    pub symmetry_folds: u32,
    /// Base number of particles to spawn per frame at max energy
    pub max_spawn_rate: u32,
    /// Base particle speed multiplier
    pub speed_multiplier: f32,
    /// Base hue (0-360), shifts with audio
    pub base_hue: f32,
    /// Hue range for variation
    pub hue_range: f32,
}

impl Default for ParticleConfig {
    fn default() -> Self {
        Self {
            symmetry_folds: 8,
            max_spawn_rate: 12,
            speed_multiplier: 1.0,
            base_hue: 250.0, // Purple-blue
            hue_range: 120.0,
        }
    }
}

/// The particle system state.
pub struct ParticleSystem {
    pub particles: Vec<Particle>,
    pub config: ParticleConfig,
    frame_count: u32,
}

impl ParticleSystem {
    pub fn new(config: ParticleConfig) -> Self {
        Self {
            particles: Vec::with_capacity(2000),
            config,
            frame_count: 0,
        }
    }

    /// Update the particle system for one frame given audio features.
    pub fn update(&mut self, features: &FrameFeatures) {
        self.frame_count += 1;
        let mut rng = rand::thread_rng();

        // Spawn new particles based on audio energy
        let spawn_count = (features.rms * self.config.max_spawn_rate as f32) as u32 + 1;

        // Curated color palettes — each is a set of harmonious (H, S, L) values
        // Palette rotates slowly over time for variety
        const PALETTES: &[&[(f32, f32, f32)]] = &[
            // Neon Dreams: electric purple, hot pink, cyan, lime
            &[(280.0, 0.95, 0.6), (330.0, 0.95, 0.6), (185.0, 0.9, 0.55), (95.0, 0.85, 0.55)],
            // Sunset Glow: coral, amber, magenta, gold
            &[(15.0, 0.9, 0.6), (35.0, 0.95, 0.55), (320.0, 0.85, 0.6), (45.0, 0.9, 0.6)],
            // Ocean Aurora: teal, aqua, violet, mint
            &[(175.0, 0.85, 0.5), (195.0, 0.9, 0.6), (265.0, 0.8, 0.6), (155.0, 0.8, 0.55)],
            // Cosmic Fire: red-orange, deep pink, electric blue, white-hot
            &[(5.0, 0.95, 0.55), (340.0, 0.9, 0.55), (220.0, 0.9, 0.6), (50.0, 0.9, 0.7)],
            // Forest Magic: emerald, chartreuse, lavender, peach
            &[(145.0, 0.8, 0.5), (80.0, 0.85, 0.55), (270.0, 0.7, 0.65), (20.0, 0.8, 0.65)],
        ];

        // Switch palette every ~4 seconds (120 frames at 30fps)
        let palette_idx = (self.frame_count / 120) as usize % PALETTES.len();
        let palette = PALETTES[palette_idx];

        for _ in 0..spawn_count {
            let angle = rng.gen::<f32>() * std::f32::consts::TAU;
            let speed_base = 0.005 + features.rms * 0.02 * self.config.speed_multiplier;
            let speed = speed_base * (0.5 + rng.gen::<f32>() * 0.5);

            // Pick a color from the palette, with slight random variation
            let color_idx = rng.gen_range(0..palette.len());
            let (base_h, base_s, base_l) = palette[color_idx];
            let hue = (base_h + rng.gen::<f32>() * 20.0 - 10.0 + 360.0) % 360.0;
            let saturation = (base_s + rng.gen::<f32>() * 0.1 - 0.05).clamp(0.7, 1.0);
            let _ = base_l; // lightness applied at render time

            // Size driven by mid energy
            let size = 3.0 + features.mid_energy * 9.0 + rng.gen::<f32>() * 3.0;

            // Spawn slightly off-center for more interesting patterns
            let spawn_radius = rng.gen::<f32>() * 0.05;

            self.particles.push(Particle {
                x: spawn_radius * angle.cos(),
                y: spawn_radius * angle.sin(),
                vx: speed * angle.cos(),
                vy: speed * angle.sin(),
                life: 1.0,
                decay: 0.008 + rng.gen::<f32>() * 0.012,
                size,
                hue,
                saturation,
                brightness: 0.8 + features.high_energy * 0.2,
            });
        }

        // Update existing particles
        for p in &mut self.particles {
            p.x += p.vx;
            p.y += p.vy;

            // Slight spiral motion
            let spiral = 0.02 + features.low_energy * 0.03;
            let new_vx = p.vx * (1.0 - spiral * 0.1) - p.vy * spiral * 0.5;
            let new_vy = p.vy * (1.0 - spiral * 0.1) + p.vx * spiral * 0.5;
            p.vx = new_vx;
            p.vy = new_vy;

            // Slow down over time
            p.vx *= 0.995;
            p.vy *= 0.995;

            p.life -= p.decay;
            p.brightness *= 0.99;
        }

        // Remove dead particles
        self.particles.retain(|p| p.life > 0.0);

        // Cap particle count to prevent memory issues
        if self.particles.len() > 3000 {
            self.particles.drain(0..self.particles.len() - 2000);
        }
    }
}

/// Convert HSL to RGB (all values 0..1 for s,l; 0..360 for h).
pub fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (u8, u8, u8) {
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
