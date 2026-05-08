//! Particle-based audio visualization renderer.
//!
//! Renders a kaleidoscope-style particle animation driven by audio features.
//! Each frame is rendered to a PNG image, then assembled into video via FFmpeg.

pub mod audio_analysis;
pub mod renderer;
pub mod particle_system;

pub use renderer::render_particle_video;
