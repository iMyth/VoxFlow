//! Particle system with kaleidoscope symmetry.
//!
//! Particles are spawned from the center, driven by audio energy,
//! and rendered with N-fold rotational symmetry for a kaleidoscope effect.
//! Enhanced with trails, pulsing rings, and richer color dynamics.

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
    /// Max size this particle can reach (radius in pixels)
    pub max_size: f32,
    /// Hue (0..360)
    pub hue: f32,
    /// Saturation (0..1)
    pub saturation: f32,
    /// Brightness/alpha (0..1)
    pub brightness: f32,
    /// Particle type for varied rendering
    pub kind: ParticleKind,
}

impl Particle {
    /// Compute the current display size based on life phase.
    /// Life goes from 1.0 (birth) → 0.0 (death).
    /// Size curve: small at birth → peak at ~70% life remaining → shrink at death.
    #[inline]
    pub fn current_size(&self) -> f32 {
        // age goes 0 (birth) → 1 (death)
        let age = 1.0 - self.life;
        // Grow phase: first 30% of life (age 0..0.3)
        // Peak phase: 30%-70% (age 0.3..0.7)
        // Shrink phase: last 30% (age 0.7..1.0)
        let scale = if age < 0.25 {
            // Grow: ease-out from 0.2 to 1.0
            let t = age / 0.25;
            0.2 + 0.8 * t.sqrt()
        } else if age < 0.7 {
            // Peak: full size
            1.0
        } else {
            // Shrink: ease-in from 1.0 to 0.0
            let t = (age - 0.7) / 0.3;
            1.0 - t * t
        };
        self.max_size * scale
    }
}

/// Different particle types for visual variety.
#[derive(Clone, Copy, PartialEq)]
pub enum ParticleKind {
    /// Standard dot particle
    Dot,
    /// Larger, softer glow particle
    Glow,
    /// Tiny sparkle that fades fast
    Spark,
}

/// A pulsing ring that expands outward from the center.
#[derive(Clone)]
pub struct Ring {
    /// Current radius (0..1 normalized)
    pub radius: f32,
    /// Expansion speed per frame
    pub speed: f32,
    /// Remaining life (0..1)
    pub life: f32,
    /// Ring thickness in pixels
    pub thickness: f32,
    /// Hue
    pub hue: f32,
    /// Brightness
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
            max_spawn_rate: 16,
            speed_multiplier: 1.0,
            base_hue: 250.0,
            hue_range: 120.0,
        }
    }
}

/// The particle system state.
pub struct ParticleSystem {
    pub particles: Vec<Particle>,
    pub rings: Vec<Ring>,
    pub config: ParticleConfig,
    frame_count: u32,
    /// Smoothed energy for less jittery visuals
    smooth_rms: f32,
    smooth_low: f32,
    smooth_mid: f32,
    smooth_high: f32,
    /// Beat detection state
    prev_rms: f32,
    beat_cooldown: u32,
}

impl ParticleSystem {
    pub fn new(config: ParticleConfig) -> Self {
        Self {
            particles: Vec::with_capacity(4000),
            rings: Vec::with_capacity(20),
            config,
            frame_count: 0,
            smooth_rms: 0.0,
            smooth_low: 0.0,
            smooth_mid: 0.0,
            smooth_high: 0.0,
            prev_rms: 0.0,
            beat_cooldown: 0,
        }
    }

    /// Update the particle system for one frame given audio features.
    pub fn update(&mut self, features: &FrameFeatures) {
        self.frame_count += 1;
        let mut rng = rand::thread_rng();

        // Smooth energy values for less jittery animation
        let smooth_factor = 0.3;
        self.smooth_rms += (features.rms - self.smooth_rms) * smooth_factor;
        self.smooth_low += (features.low_energy - self.smooth_low) * smooth_factor;
        self.smooth_mid += (features.mid_energy - self.smooth_mid) * smooth_factor;
        self.smooth_high += (features.high_energy - self.smooth_high) * smooth_factor;

        // Beat detection: sudden energy spike
        let is_beat = self.smooth_rms > self.prev_rms + 0.15 && self.beat_cooldown == 0;
        if is_beat {
            self.beat_cooldown = 8; // ~0.27s at 30fps
        }
        if self.beat_cooldown > 0 {
            self.beat_cooldown -= 1;
        }
        self.prev_rms = self.smooth_rms;

        // Spawn ring on beat
        if is_beat {
            let palette = Self::current_palette(self.frame_count);
            let (h, _, _) = palette[0];
            self.rings.push(Ring {
                radius: 0.02,
                speed: 0.015 + self.smooth_rms * 0.01,
                life: 1.0,
                thickness: 2.0 + self.smooth_low * 4.0,
                hue: h,
                brightness: 0.9,
            });
        }

        // Spawn particles based on audio energy
        let base_spawn = (self.smooth_rms * self.config.max_spawn_rate as f32) as u32 + 2;
        // Extra burst on beat
        let spawn_count = if is_beat { base_spawn * 3 } else { base_spawn };

        let palette = Self::current_palette(self.frame_count);

        for _ in 0..spawn_count {
            let angle = rng.gen::<f32>() * std::f32::consts::TAU;
            let speed_base = 0.005 + self.smooth_rms * 0.03 * self.config.speed_multiplier;
            let speed = speed_base * (0.4 + rng.gen::<f32>() * 0.6);

            // Pick a color from the palette
            let color_idx = rng.gen_range(0..palette.len());
            let (base_h, base_s, _base_l) = palette[color_idx];
            let hue = (base_h + rng.gen::<f32>() * 15.0 - 7.5 + 360.0) % 360.0;
            let saturation = (base_s + rng.gen::<f32>() * 0.1 - 0.05).clamp(0.6, 1.0);

            // Determine particle kind
            let kind_roll = rng.gen::<f32>();
            let kind = if kind_roll < 0.1 {
                ParticleKind::Glow
            } else if kind_roll < 0.3 {
                ParticleKind::Spark
            } else {
                ParticleKind::Dot
            };

            let (size, decay) = match kind {
                ParticleKind::Dot => {
                    let size = 3.0 + self.smooth_mid * 8.0 + rng.gen::<f32>() * 3.0;
                    let decay = 0.004 + rng.gen::<f32>() * 0.007;
                    (size, decay)
                }
                ParticleKind::Glow => {
                    let size = 7.0 + self.smooth_low * 14.0 + rng.gen::<f32>() * 5.0;
                    let decay = 0.005 + rng.gen::<f32>() * 0.006;
                    (size, decay)
                }
                ParticleKind::Spark => {
                    let size = 1.5 + self.smooth_high * 3.5;
                    let decay = 0.012 + rng.gen::<f32>() * 0.018;
                    (size, decay)
                }
            };

            // Spawn slightly off-center for more interesting patterns
            let spawn_radius = rng.gen::<f32>() * 0.04;

            self.particles.push(Particle {
                x: spawn_radius * angle.cos(),
                y: spawn_radius * angle.sin(),
                vx: speed * angle.cos(),
                vy: speed * angle.sin(),
                life: 1.0,
                decay,
                max_size: size,
                hue,
                saturation,
                brightness: 0.7 + self.smooth_high * 0.3,
                kind,
            });
        }

        // Update existing particles
        let spiral_strength = 0.02 + self.smooth_low * 0.04;
        for p in &mut self.particles {
            p.x += p.vx;
            p.y += p.vy;

            // Spiral motion — stronger for glow particles
            let spiral = match p.kind {
                ParticleKind::Glow => spiral_strength * 1.5,
                _ => spiral_strength,
            };
            let new_vx = p.vx * (1.0 - spiral * 0.1) - p.vy * spiral * 0.5;
            let new_vy = p.vy * (1.0 - spiral * 0.1) + p.vx * spiral * 0.5;
            p.vx = new_vx;
            p.vy = new_vy;

            // Slow down gently — let particles travel far
            p.vx *= 0.997;
            p.vy *= 0.997;

            p.life -= p.decay;
            p.brightness *= 0.992;

            // Sparks fade faster
            if p.kind == ParticleKind::Spark {
                p.brightness *= 0.98;
            }
        }

        // Update rings
        for ring in &mut self.rings {
            ring.radius += ring.speed;
            ring.life -= 0.015;
            ring.brightness *= 0.97;
        }

        // Remove dead particles and rings
        self.particles.retain(|p| p.life > 0.0);
        self.rings.retain(|r| r.life > 0.0 && r.radius < 1.2);

        // Cap particle count — aggressive cap to prevent slowdown on long videos.
        // Keep the newest particles (at the end) which are most visible.
        if self.particles.len() > 2000 {
            self.particles.drain(0..self.particles.len() - 1500);
        }
    }

    /// Get the current color palette based on frame count.
    fn current_palette(frame_count: u32) -> &'static [(f32, f32, f32)] {
        // Curated color palettes — each is a set of harmonious (H, S, L) values
        const PALETTES: &[&[(f32, f32, f32)]] = &[
            // Neon Dreams: electric purple, hot pink, cyan, lime
            &[
                (280.0, 0.95, 0.6),
                (330.0, 0.95, 0.6),
                (185.0, 0.9, 0.55),
                (95.0, 0.85, 0.55),
            ],
            // Sunset Glow: coral, amber, magenta, gold
            &[
                (15.0, 0.9, 0.6),
                (35.0, 0.95, 0.55),
                (320.0, 0.85, 0.6),
                (45.0, 0.9, 0.6),
            ],
            // Ocean Aurora: teal, aqua, violet, mint
            &[
                (175.0, 0.85, 0.5),
                (195.0, 0.9, 0.6),
                (265.0, 0.8, 0.6),
                (155.0, 0.8, 0.55),
            ],
            // Cosmic Fire: red-orange, deep pink, electric blue, white-hot
            &[
                (5.0, 0.95, 0.55),
                (340.0, 0.9, 0.55),
                (220.0, 0.9, 0.6),
                (50.0, 0.9, 0.7),
            ],
            // Forest Magic: emerald, chartreuse, lavender, peach
            &[
                (145.0, 0.8, 0.5),
                (80.0, 0.85, 0.55),
                (270.0, 0.7, 0.65),
                (20.0, 0.8, 0.65),
            ],
            // Midnight Bloom: deep blue, rose, silver, gold
            &[
                (230.0, 0.85, 0.5),
                (350.0, 0.8, 0.6),
                (210.0, 0.3, 0.7),
                (42.0, 0.9, 0.6),
            ],
        ];

        // Switch palette every ~5 seconds (150 frames at 30fps)
        let palette_idx = (frame_count / 150) as usize % PALETTES.len();
        PALETTES[palette_idx]
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
