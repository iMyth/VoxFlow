//! Fractal zoom visualization renderer.
//!
//! Renders an infinite Mandelbrot/Julia set zoom with audio-reactive coloring
//! and zoom speed. Uses wgpu compute shaders for GPU-accelerated rendering,
//! with automatic fallback to CPU (rayon) if GPU is unavailable.

pub mod gpu;
pub mod renderer;

pub use renderer::render_fractal_video;
