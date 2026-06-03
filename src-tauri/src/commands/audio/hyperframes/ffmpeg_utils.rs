//! Shared FFmpeg utilities for the hyperframes module.
//!
//! Contains common functions used across multiple modules to avoid duplication:
//! - Sleep mode audio filter chain
//! - Video + audio merge operations

use std::process::Stdio;

use log::info;
use tokio::process::Command;

use crate::commands::audio::ffmpeg::find_ffmpeg;
use crate::core::error::AppError;

/// Build the FFmpeg audio filter chain for sleep mode.
///
/// Sleep mode applies:
/// - Slight pitch reduction (0.95x)
/// - Bass warmth boost (+3dB at 150Hz)
/// - High-frequency rolloff (8kHz lowpass)
/// - Quieter target loudness (-20 LUFS)
///
/// This is a single source of truth for the sleep mode filter chain,
/// used by section_audio.rs, video_merger.rs, and render.rs.
pub fn build_sleep_mode_audio_filter() -> &'static str {
    "asetrate=22050*0.95,aresample=22050,bass=g=3:f=150,lowpass=f=8000:p=1,loudnorm=I=-20:TP=-2:LRA=7"
}

/// Merge a silent video with an audio file using FFmpeg.
///
/// - Video is copied without re-encoding (`-c:v copy`)
/// - Audio is encoded to AAC (`-c:a aac`)
/// - Uses `-shortest` to match the shorter stream
///
/// If the merge fails, falls back to copying the silent video to output.
///
/// Returns Ok(()) on success, or the original error if fallback also fails.
pub async fn merge_video_with_audio(
    silent_video_path: &str,
    audio_path: &str,
    output_path: &str,
) -> Result<(), AppError> {
    let ffmpeg_bin = find_ffmpeg();

    info!(
        "[FFmpeg Utils] Merging video + audio: {} + {} -> {}",
        silent_video_path, audio_path, output_path
    );

    let ffmpeg_status = Command::new(&ffmpeg_bin)
        .args([
            "-y",
            "-i",
            silent_video_path,
            "-i",
            audio_path,
            "-c:v",
            "copy",
            "-c:a",
            "aac",
            "-shortest",
            output_path,
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await
        .map_err(|e| {
            info!("[FFmpeg Utils] ffmpeg execution failed: {}", e);
            AppError::FFmpeg(format!("Failed to run ffmpeg: {}", e))
        })?;

    if !ffmpeg_status.success() {
        info!("[FFmpeg Utils] ffmpeg merge failed, falling back to silent video");
        // Fallback: copy silent video to output
        let silent_path = std::path::Path::new(silent_video_path);
        let output = std::path::Path::new(output_path);
        std::fs::copy(silent_path, output).map_err(|e| {
            AppError::FFmpeg(format!(
                "ffmpeg merge failed and fallback copy also failed: {}",
                e
            ))
        })?;
    } else {
        // Clean up the silent video on success
        let _ = std::fs::remove_file(silent_video_path);
        info!("[FFmpeg Utils] Audio merge successful, removed silent video");
    }

    Ok(())
}

/// Copy a video file to a new location, removing the source on success.
///
/// Used when no audio merge is needed but we want to move the file to the final location.
pub fn copy_video_file(source: &str, destination: &str) -> Result<(), AppError> {
    let src_path = std::path::Path::new(source);
    let dest_path = std::path::Path::new(destination);

    std::fs::copy(src_path, dest_path)
        .map_err(|e| AppError::FFmpeg(format!("Failed to copy video file: {}", e)))?;

    // Try to remove source (may fail if cross-device, ignore)
    let _ = std::fs::remove_file(src_path);

    info!("[FFmpeg Utils] Copied video: {} -> {}", source, destination);
    Ok(())
}
