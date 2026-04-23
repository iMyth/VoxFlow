//! Audio module
//!
//! This module provides audio functionality for:
//! - Audio playback via rodio
//! - Audio export and mixing
//! - Audio import (BGM and recordings)
//! - FFmpeg integration

mod export;
mod ffmpeg;
mod import;
mod player;
mod utils;

// Re-export all public items from submodules (includes __cmd__ functions from #[tauri::command])
pub use export::*;
pub use import::*;
pub use player::*;
// ffmpeg module is used internally by other commands
#[allow(unused_imports)]
pub use ffmpeg::*;
