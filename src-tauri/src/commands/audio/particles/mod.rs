//! Particle-based audio visualization renderer.
//!
//! Renders a kaleidoscope-style particle animation driven by audio features.
//! Uses wgpu compute shaders for GPU-accelerated rendering, with automatic
//! fallback to CPU (rayon) if GPU is unavailable.

pub mod audio_analysis;
mod draw;
pub mod gpu;
pub mod particle_system;
pub mod renderer;

pub use renderer::render_particle_video;
