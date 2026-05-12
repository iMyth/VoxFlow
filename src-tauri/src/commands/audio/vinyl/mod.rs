//! Vinyl/CD disc visualization renderer.
//!
//! Renders a spinning disc (vinyl record or CD) with an optional cover image,
//! surrounded by audio-reactive elements (equalizer bars, glow rings).
//! Uses wgpu compute shaders for GPU-accelerated rendering, with automatic
//! fallback to CPU (rayon) if GPU is unavailable.

mod draw;
pub mod gpu;
pub mod renderer;
mod texture;
mod utils;

pub use renderer::render_vinyl_video;
