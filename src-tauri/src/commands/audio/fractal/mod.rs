//! Fractal zoom visualization renderer.
//!
//! Renders an infinite Mandelbrot/Julia set zoom with audio-reactive coloring
//! and zoom speed. Creates a hypnotic, never-repeating visual that pairs
//! perfectly with long-form audio content like audiobooks.

pub mod renderer;

pub use renderer::render_fractal_video;
