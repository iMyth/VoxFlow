//! Ink diffusion visualization renderer.
//!
//! Uses a Gray-Scott reaction-diffusion system to simulate ink spreading
//! in water. Audio drives the injection of new "ink drops" and controls
//! the diffusion/reaction parameters for ever-changing organic patterns.
//! Creates mesmerizing, never-repeating visuals perfect for audiobooks.

pub mod renderer;

pub use renderer::render_ink_video;
