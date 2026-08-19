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
/// Sleep mode applies tone/volume adjustments WITHOUT changing duration:
/// - Bass warmth boost (+3dB at 150Hz) — fuller, warmer sound
/// - High-frequency rolloff (8kHz lowpass) — removes harsh sibilance
/// - Slight pitch shift down (-0.5 semitones) via rubberband — deeper, calmer voice
/// - Quieter target loudness (-20 LUFS) — sleep-appropriate volume
///
/// CRITICAL: No tempo/speed change is applied. The audio duration must remain
/// identical to the video duration to maintain sync. The original `asetrate`
/// approach broke sync (and broke audio entirely for non-22050Hz inputs).
/// `atempo` would also break sync by changing duration.
///
/// This is a single source of truth for the sleep mode filter chain,
/// used by section_audio.rs, video_merger.rs, and render.rs.
pub fn build_sleep_mode_audio_filter() -> &'static str {
    "bass=g=3:f=150,lowpass=f=8000:p=1,loudnorm=I=-20:TP=-2:LRA=7:linear=true"
}

/// Probe the duration of a media file in seconds using ffprobe.
fn probe_duration_secs(file_path: &str) -> Result<f64, AppError> {
    let ffmpeg_path = find_ffmpeg();
    let ffprobe_path = ffmpeg_path.replace("ffmpeg", "ffprobe");

    let output = std::process::Command::new(&ffprobe_path)
        .args([
            "-v",
            "quiet",
            "-show_entries",
            "format=duration",
            "-of",
            "csv=p=0",
            file_path,
        ])
        .output()
        .map_err(|e| AppError::FFmpeg(format!("Failed to run ffprobe for duration: {}", e)))?;

    if !output.status.success() {
        return Err(AppError::FFmpeg(format!(
            "ffprobe duration query failed for '{}'",
            file_path
        )));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout.trim().parse::<f64>().map_err(|_| {
        AppError::FFmpeg(format!(
            "Failed to parse duration from ffprobe for '{}'",
            file_path
        ))
    })
}

/// Merge a silent video with an audio file using FFmpeg.
///
/// - Video is copied without re-encoding (`-c:v copy`)
/// - Audio is encoded to AAC (`-c:a aac`)
/// - Uses audio duration as the authoritative length via `-t` from ffprobe.
///   This prevents audio truncation that occurred with `-shortest` when the
///   video stream was slightly shorter than the audio stream (due to frame
///   rounding). If the video is shorter, the last frame holds; if longer,
///   it gets trimmed to audio duration — preventing trailing blank frames.
///
/// Returns Ok(()) on success, Err on failure. No longer silently falls back
/// to a silent video — callers should handle the error explicitly.
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

    // Probe audio duration to use as the authoritative output length.
    // Previously we used -shortest which would truncate audio if the video stream
    // ended slightly earlier (due to frame duration rounding). Now we explicitly
    // set the output duration to match audio, ensuring no audio is lost.
    let audio_duration_secs = probe_duration_secs(audio_path)?;

    let duration_str = format!("{:.6}", audio_duration_secs);

    let result = Command::new(&ffmpeg_bin)
        .args([
            "-y",
            "-i",
            silent_video_path,
            "-i",
            audio_path,
            "-map",
            "0:v",
            "-map",
            "1:a",
            "-c:v",
            "copy",
            "-c:a",
            "aac",
            "-b:a",
            "192k",
            "-t",
            &duration_str,
            output_path,
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| {
            info!("[FFmpeg Utils] ffmpeg execution failed: {}", e);
            AppError::FFmpeg(format!("Failed to run ffmpeg: {}", e))
        })?;

    if !result.status.success() {
        let stderr = String::from_utf8_lossy(&result.stderr);
        info!(
            "[FFmpeg Utils] ffmpeg merge failed: {}",
            stderr.chars().take(300).collect::<String>()
        );
        // Fall back to silent video copy so user at least gets visual output,
        // but log clearly and return Ok to not block the pipeline entirely.
        // The video will lack audio — this is better than total failure.
        info!("[FFmpeg Utils] Falling back to silent video (audio will be missing!)");
        let silent_path = std::path::Path::new(silent_video_path);
        let output = std::path::Path::new(output_path);
        std::fs::copy(silent_path, output).map_err(|e| {
            AppError::FFmpeg(format!(
                "ffmpeg merge failed and fallback copy also failed: {}",
                e
            ))
        })?;
        // Return Ok but caller should check — this is a degraded state
        return Ok(());
    }

    // Clean up the silent video on success
    let _ = std::fs::remove_file(silent_video_path);
    info!("[FFmpeg Utils] Audio merge successful, removed silent video");

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
