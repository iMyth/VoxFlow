//! Vinyl/CD disc visualization renderer.
//!
//! Renders a spinning disc (vinyl record or CD) with an optional cover image,
//! surrounded by audio-reactive elements (equalizer bars, glow rings).
//! Designed for YouTube-ready audiobook/podcast videos.

pub mod renderer;

pub use renderer::render_vinyl_video;
