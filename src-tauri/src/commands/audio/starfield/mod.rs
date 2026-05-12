//! Starfield tunnel visualization renderer.
//!
//! Renders a "flying through space" effect with audio-reactive speed,
//! spiral motion, nebula background, and warp speed lines.
//! Uses wgpu compute shaders for GPU-accelerated rendering.

pub mod gpu;
pub mod renderer;

pub use renderer::render_starfield_video;
