//! Audio module
//!
//! This module provides audio functionality for:
//! - Audio playback via rodio
//! - Audio export and mixing
//! - Audio import (BGM and recordings)
//! - FFmpeg integration
//! - Video export with audio visualization
//! - Particle-based kaleidoscope visualization
//! - Vinyl/CD disc visualization
//! - Fractal zoom visualization
//! - Ink diffusion visualization

mod export;
mod ffmpeg;
pub mod fractal;
mod import;
pub mod ink;
pub mod particles;
mod player;
mod utils;
mod video;
pub mod vinyl;
pub mod starfield;

// Re-export all public items from submodules (includes __cmd__ functions from #[tauri::command])
pub use export::*;
pub use import::*;
pub use player::*;
pub use video::*;
// ffmpeg module is used internally by other commands
#[allow(unused_imports)]
pub use ffmpeg::*;
