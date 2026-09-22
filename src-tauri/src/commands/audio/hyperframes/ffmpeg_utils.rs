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

    // Publish only after a successful merge, preserving any previous good output.
    let destination = std::path::Path::new(output_path);
    let extension = destination
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("mp4");
    let temporary =
        destination.with_file_name(format!(".voxflow-{}.{}", uuid::Uuid::new_v4(), extension));
    let temporary_str = temporary.to_string_lossy().to_string();

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
            &temporary_str,
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
        let _ = std::fs::remove_file(&temporary);
        return Err(AppError::FFmpeg(format!(
            "Audio/video merge failed (exit {:?}): {}",
            result.status.code(),
            stderr.trim()
        )));
    }

    if let Err(error) = std::fs::rename(&temporary, destination) {
        let _ = std::fs::remove_file(&temporary);
        return Err(AppError::FileSystem(format!(
            "Failed to publish merged video: {error}"
        )));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore = "requires installed FFmpeg and FFprobe"]
    async fn failed_merge_preserves_previous_output_and_reports_error() {
        let dir = tempfile::tempdir().unwrap();
        let audio = dir.path().join("audio.wav");
        let video = dir.path().join("broken.mp4");
        let output = dir.path().join("output.mp4");
        let result = std::process::Command::new(find_ffmpeg())
            .args([
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "anullsrc=r=24000:cl=mono",
                "-t",
                "0.1",
            ])
            .arg(&audio)
            .status()
            .unwrap();
        assert!(result.success());
        std::fs::write(&video, b"invalid video").unwrap();
        std::fs::write(&output, b"previous good output").unwrap();
        let error = merge_video_with_audio(
            video.to_str().unwrap(),
            audio.to_str().unwrap(),
            output.to_str().unwrap(),
        )
        .await
        .unwrap_err();
        assert!(error.to_string().contains("merge failed"));
        assert_eq!(std::fs::read(&output).unwrap(), b"previous good output");
        assert!(video.exists());
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 3);

        // A subsequent successful merge publishes a playable file with both streams.
        let status = std::process::Command::new(find_ffmpeg())
            .args([
                "-v",
                "error",
                "-y",
                "-f",
                "lavfi",
                "-i",
                "color=black:s=32x32:r=30",
                "-t",
                "0.1",
                "-c:v",
                "libx264",
                "-pix_fmt",
                "yuv420p",
            ])
            .arg(&video)
            .status()
            .unwrap();
        assert!(status.success());
        merge_video_with_audio(
            video.to_str().unwrap(),
            audio.to_str().unwrap(),
            output.to_str().unwrap(),
        )
        .await
        .unwrap();
        assert!(!video.exists());
        let probe = std::process::Command::new(find_ffmpeg().replace("ffmpeg", "ffprobe"))
            .args([
                "-v",
                "error",
                "-show_entries",
                "stream=codec_type",
                "-of",
                "json",
            ])
            .arg(&output)
            .output()
            .unwrap();
        assert!(probe.status.success());
        let metadata: serde_json::Value = serde_json::from_slice(&probe.stdout).unwrap();
        let streams = metadata["streams"].as_array().unwrap();
        assert!(streams.iter().any(|s| s["codec_type"] == "audio"));
        assert!(streams.iter().any(|s| s["codec_type"] == "video"));
    }
}
