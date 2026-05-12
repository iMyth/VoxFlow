//! Starfield tunnel visualization renderer.
//!
//! Renders a classic "flying through space" starfield effect driven by audio.
//! Stars fly from the center outward with perspective projection, leaving
//! gradient trails. Speed and intensity are modulated by audio energy.

pub mod renderer;

pub use renderer::render_starfield_video;
