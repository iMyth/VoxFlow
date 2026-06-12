//! Video merger module for paragraph-level video generation.
//!
//! Merges multiple section videos into a single final output with cross-fade transitions.

use std::path::Path;
use std::sync::Mutex;

use log::info;
use tauri::{Emitter, Manager};

use crate::commands::audio::ffmpeg::find_ffmpeg;
use crate::core::db::Database;
use crate::core::error::AppError;

use super::ffmpeg_utils::build_sleep_mode_audio_filter;
use super::section_types::{MergeProgress, SectionVideoFile};

/// Probe a video file's codec and resolution using ffprobe.
fn probe_video(file_path: &str) -> Result<(String, String), AppError> {
    let ffmpeg_path = find_ffmpeg();
    let ffprobe_path = ffmpeg_path.replace("ffmpeg", "ffprobe");

    let output = std::process::Command::new(&ffprobe_path)
        .args([
            "-v",
            "quiet",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=codec_name,width,height",
            "-of",
            "csv=p=0",
            file_path,
        ])
        .output()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                AppError::FFmpeg("ffprobe not found. Please install FFmpeg.".to_string())
            } else {
                AppError::FFmpeg(format!("Failed to run ffprobe: {}", e))
            }
        })?;

    if !output.status.success() {
        return Err(AppError::FFmpeg(format!(
            "ffprobe failed for '{}': {}",
            file_path,
            String::from_utf8_lossy(&output.stderr)
        )));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let parts: Vec<&str> = stdout.trim().split(',').collect();
    if parts.len() >= 3 {
        let codec = parts[0].to_string();
        let resolution = format!("{}x{}", parts[1], parts[2]);
        Ok((codec, resolution))
    } else {
        Err(AppError::FFmpeg(format!(
            "Unexpected ffprobe output for '{}': {}",
            file_path, stdout
        )))
    }
}

/// Merge section videos into a final output.
///
/// - Validates all section files exist
/// - Probes codec/resolution via ffprobe
/// - Uses concat demuxer if uniform and no audio processing needed
/// - Re-encodes if mixed formats or audio processing needed
/// - Applies sleep mode audio processing if enabled:
///   - Slight pitch reduction (0.95x)
///   - Bass warmth boost (+3dB at 150Hz)
///   - High-frequency rolloff (8kHz lowpass)
///   - Quieter target loudness (-20 LUFS instead of -16 LUFS)
/// - Single section: copies without re-encoding unless sleep mode enabled
/// - Emits progress via callback
/// - Cleans up partial output on failure
pub async fn merge_videos(
    section_videos: &[SectionVideoFile],
    output_path: &Path,
    _transition_duration_ms: u32,
    sleep_mode: bool,
    on_progress: impl Fn(f32, &str),
) -> Result<String, AppError> {
    if section_videos.is_empty() {
        return Err(AppError::FFmpeg("No section videos to merge".to_string()));
    }

    // Validate all files exist
    on_progress(5.0, "validating");
    let mut missing: Vec<String> = Vec::new();
    for sv in section_videos {
        if !Path::new(&sv.file_path).exists() {
            missing.push(format!(
                "Section '{}' (order {}): {}",
                sv.section_id, sv.section_order, sv.file_path
            ));
        }
    }
    if !missing.is_empty() {
        return Err(AppError::FFmpeg(format!(
            "Missing section video files:\n{}",
            missing.join("\n")
        )));
    }

    // Single section: just copy if no sleep mode, otherwise re-encode with audio processing
    if section_videos.len() == 1 {
        on_progress(50.0, "concatenating");

        if !sleep_mode {
            std::fs::copy(&section_videos[0].file_path, output_path).map_err(|e| {
                AppError::FFmpeg(format!("Failed to copy single section video: {}", e))
            })?;
        } else {
            // Re-encode with sleep mode audio processing
            let ffmpeg_bin = find_ffmpeg();
            let input_path = section_videos[0].file_path.clone();
            let output_str = output_path.to_string_lossy().to_string();

            let args = vec![
                "-y".to_string(),
                "-i".to_string(),
                input_path.clone(),
                "-c:v".to_string(),
                "copy".to_string(),
                "-af".to_string(),
                build_sleep_mode_audio_filter().to_string(),
                output_str.clone(),
            ];

            let result = tokio::task::spawn_blocking(move || {
                std::process::Command::new(&ffmpeg_bin)
                    .args(&args)
                    .stderr(std::process::Stdio::piped())
                    .stdout(std::process::Stdio::piped())
                    .output()
            })
            .await
            .map_err(|e| AppError::FFmpeg(format!("spawn_blocking failed: {}", e)))?;

            match result {
                Ok(output) if output.status.success() => {}
                Ok(output) => {
                    let _ = std::fs::remove_file(output_path);
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    return Err(AppError::FFmpeg(format!(
                        "Failed to process single section with sleep mode: {}",
                        stderr.chars().take(300).collect::<String>()
                    )));
                }
                Err(e) => {
                    let _ = std::fs::remove_file(output_path);
                    return Err(AppError::FFmpeg(format!(
                        "Failed to execute ffmpeg for sleep mode: {}",
                        e
                    )));
                }
            }
        }

        on_progress(100.0, "finalizing");
        return Ok(output_path.to_string_lossy().to_string());
    }

    // Probe all videos
    on_progress(10.0, "validating");
    let mut probes: Vec<(String, String)> = Vec::new();
    for sv in section_videos {
        let probe = probe_video(&sv.file_path)?;
        probes.push(probe);
    }

    // Check if all videos share the same codec and resolution
    let first_codec = &probes[0].0;
    let first_resolution = &probes[0].1;
    let is_uniform = probes
        .iter()
        .all(|(c, r)| c == first_codec && r == first_resolution);

    // If resolutions differ, normalize all to 1920x1080 before concat.
    // This happens when LLM outputs wrong data-width/data-height in some sections.
    let all_1080p = probes.iter().all(|(_, r)| r == "1920x1080");

    on_progress(20.0, "concatenating");

    let ffmpeg_bin = find_ffmpeg();
    let output_str = output_path.to_string_lossy().to_string();

    if is_uniform && all_1080p && !sleep_mode {
        // Re-encode audio during concat to avoid sample rate / AAC priming issues.
        // The concat demuxer with `-c copy` on low sample rate AAC (22050Hz) causes
        // ffmpeg to misinterpret time_base, inflating audio duration vs video duration.
        // Re-encoding audio to AAC 44100Hz fixes the mismatch while keeping video as copy.
        let concat_path = output_path
            .parent()
            .unwrap_or(Path::new("."))
            .join("_concat_list.txt");
        let concat_path_str = concat_path.to_string_lossy().to_string();

        let mut content = String::new();
        for sv in section_videos {
            content.push_str(&format!("file '{}'\n", sv.file_path));
        }
        std::fs::write(&concat_path, &content)
            .map_err(|e| AppError::FFmpeg(format!("Failed to write concat list: {}", e)))?;

        on_progress(40.0, "concatenating");

        let result = tokio::task::spawn_blocking(move || {
            std::process::Command::new(&ffmpeg_bin)
                .args([
                    "-y",
                    "-f",
                    "concat",
                    "-safe",
                    "0",
                    "-i",
                    &concat_path_str,
                    "-c:v",
                    "copy",
                    "-c:a",
                    "aac",
                    "-ar",
                    "44100",
                    "-b:a",
                    "192k",
                    &output_str,
                ])
                .stderr(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .output()
        })
        .await
        .map_err(|e| AppError::FFmpeg(format!("spawn_blocking failed: {}", e)))?;

        // Clean up concat list file
        let _ = std::fs::remove_file(&concat_path);

        match result {
            Ok(output) if output.status.success() => {
                on_progress(100.0, "finalizing");
                Ok(output_path.to_string_lossy().to_string())
            }
            Ok(output) => {
                let _ = std::fs::remove_file(output_path);
                let stderr = String::from_utf8_lossy(&output.stderr);
                Err(AppError::FFmpeg(format!(
                    "ffmpeg concat failed: {}",
                    stderr.chars().take(500).collect::<String>()
                )))
            }
            Err(e) => {
                let _ = std::fs::remove_file(output_path);
                Err(AppError::FFmpeg(format!("Failed to execute ffmpeg: {}", e)))
            }
        }
    } else if is_uniform && all_1080p && sleep_mode {
        // Format uniform + sleep mode: concat with audio re-encode, then apply sleep processing.
        // Must re-encode audio during concat to avoid 22050Hz AAC time_base issues.
        let temp_path = output_path
            .parent()
            .unwrap_or(Path::new("."))
            .join("_temp_concat.mp4");
        let temp_path_str = temp_path.to_string_lossy().to_string();

        // Step 1: Concat all sections (video copy, audio re-encode to fix sample rate)
        let concat_path = temp_path
            .parent()
            .unwrap_or(Path::new("."))
            .join("_concat_list.txt");
        let concat_path_str = concat_path.to_string_lossy().to_string();

        let mut content = String::new();
        for sv in section_videos {
            content.push_str(&format!("file '{}'\n", sv.file_path));
        }
        std::fs::write(&concat_path, &content)
            .map_err(|e| AppError::FFmpeg(format!("Failed to write concat list: {}", e)))?;

        on_progress(30.0, "concatenating");

        let ffmpeg_bin_clone = ffmpeg_bin.clone();
        let concat_result = tokio::task::spawn_blocking(move || {
            std::process::Command::new(&ffmpeg_bin_clone)
                .args([
                    "-y",
                    "-f",
                    "concat",
                    "-safe",
                    "0",
                    "-i",
                    &concat_path_str,
                    "-c:v",
                    "copy",
                    "-c:a",
                    "aac",
                    "-ar",
                    "44100",
                    "-b:a",
                    "192k",
                    &temp_path_str,
                ])
                .stderr(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .output()
        })
        .await
        .map_err(|e| AppError::FFmpeg(format!("spawn_blocking failed: {}", e)))?;

        let _ = std::fs::remove_file(&concat_path);

        match concat_result {
            Ok(output) if output.status.success() => {}
            Ok(output) => {
                let _ = std::fs::remove_file(&temp_path);
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(AppError::FFmpeg(format!(
                    "Failed to concat sections: {}",
                    stderr.chars().take(300).collect::<String>()
                )));
            }
            Err(e) => {
                let _ = std::fs::remove_file(&temp_path);
                return Err(AppError::FFmpeg(format!(
                    "Failed to execute ffmpeg concat: {}",
                    e
                )));
            }
        }

        on_progress(60.0, "processing_audio");

        // Step 2: Apply sleep mode audio processing
        let output_str = output_path.to_string_lossy().to_string();
        let temp_path_str = temp_path.to_string_lossy().to_string();

        let args = vec![
            "-y".to_string(),
            "-i".to_string(),
            temp_path_str.clone(),
            "-c:v".to_string(),
            "copy".to_string(),
            "-af".to_string(),
            build_sleep_mode_audio_filter().to_string(),
            output_str.clone(),
        ];

        let result = tokio::task::spawn_blocking(move || {
            std::process::Command::new(&ffmpeg_bin)
                .args(&args)
                .stderr(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .output()
        })
        .await
        .map_err(|e| AppError::FFmpeg(format!("spawn_blocking failed: {}", e)))?;

        let _ = std::fs::remove_file(&temp_path);

        match result {
            Ok(output) if output.status.success() => {}
            Ok(output) => {
                let _ = std::fs::remove_file(output_path);
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(AppError::FFmpeg(format!(
                    "Failed to apply sleep mode processing: {}",
                    stderr.chars().take(300).collect::<String>()
                )));
            }
            Err(e) => {
                let _ = std::fs::remove_file(output_path);
                return Err(AppError::FFmpeg(format!(
                    "Failed to execute ffmpeg sleep mode: {}",
                    e
                )));
            }
        }

        on_progress(100.0, "finalizing");
        Ok(output_path.to_string_lossy().to_string())
    } else {
        // Formats are not uniform (different resolution or codec), OR sleep mode with non-uniform.
        // Use concat filter with per-input scale to normalize all videos to 1920x1080.
        // This handles LLM rendering bugs that produce wrong resolutions.
        let mut args: Vec<String> = vec!["-y".to_string()];

        // Add all input files
        for sv in section_videos {
            args.push("-i".to_string());
            args.push(sv.file_path.clone());
        }

        // Build filter: scale each input to 1920x1080, then concat
        let n = section_videos.len();
        let mut filter_complex = String::new();

        // Scale + format each video and audio stream
        for i in 0..n {
            // Scale video to 1920x1080, pad if aspect ratio differs, force yuv420p
            filter_complex.push_str(&format!(
                "[{i}:v]scale=1920:1080:force_original_aspect_ratio=decrease,\
                 pad=1920:1080:(ow-iw)/2:(oh-ih)/2,setsar=1,format=yuv420p[v{i}];",
                i = i
            ));
            // Normalize audio to consistent format
            filter_complex.push_str(&format!(
                "[{i}:a]aresample=44100,aformat=sample_fmts=fltp:channel_layouts=stereo[a{i}];",
                i = i
            ));
        }

        // Feed all normalized streams into concat
        for i in 0..n {
            filter_complex.push_str(&format!("[v{i}][a{i}]", i = i));
        }

        if sleep_mode {
            filter_complex.push_str(&format!(
                "concat=n={}:v=1:a=1[vtmp][atmp];[atmp]{}[aout];[vtmp]copy[vout]",
                n,
                build_sleep_mode_audio_filter()
            ));
        } else {
            filter_complex.push_str(&format!("concat=n={}:v=1:a=1[vout][aout]", n));
        }

        args.push("-filter_complex".to_string());
        args.push(filter_complex);
        args.push("-map".to_string());
        args.push("[vout]".to_string());
        args.push("-map".to_string());
        args.push("[aout]".to_string());

        // Output encoding settings
        args.push("-c:v".to_string());
        args.push("libx264".to_string());
        args.push("-preset".to_string());
        args.push("medium".to_string());
        args.push("-crf".to_string());
        args.push("23".to_string());
        args.push("-c:a".to_string());
        args.push("aac".to_string());
        args.push("-ar".to_string());
        args.push("44100".to_string());
        args.push("-b:a".to_string());
        args.push("192k".to_string());

        args.push(output_str.clone());

        on_progress(80.0, "concatenating");

        info!(
            "[Video Merger] Non-uniform merge: {} sections, resolutions: {:?}",
            n,
            probes.iter().map(|(_, r)| r.as_str()).collect::<Vec<_>>()
        );

        let result = tokio::task::spawn_blocking(move || {
            std::process::Command::new(&ffmpeg_bin)
                .args(&args)
                .stderr(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .output()
        })
        .await
        .map_err(|e| AppError::FFmpeg(format!("spawn_blocking failed: {}", e)))?;

        match result {
            Ok(output) if output.status.success() => {
                on_progress(100.0, "finalizing");
                Ok(output_path.to_string_lossy().to_string())
            }
            Ok(output) => {
                let _ = std::fs::remove_file(output_path);
                let stderr = String::from_utf8_lossy(&output.stderr);
                Err(AppError::FFmpeg(format!(
                    "ffmpeg merge failed: {}",
                    stderr.chars().take(500).collect::<String>()
                )))
            }
            Err(e) => {
                let _ = std::fs::remove_file(output_path);
                Err(AppError::FFmpeg(format!("Failed to execute ffmpeg: {}", e)))
            }
        }
    }
}

/// Tauri command to merge all section videos into a final output.
#[tauri::command]
pub async fn merge_section_videos(
    app: tauri::AppHandle,
    db: tauri::State<'_, Mutex<Database>>,
    project_id: String,
    output_path: String,
    transition_duration_ms: Option<u32>,
    sleep_mode: Option<bool>,
) -> Result<String, AppError> {
    let sleep_mode_enabled = sleep_mode.unwrap_or(false);

    info!(
        "[Video Merger] Starting merge: project={}, output={}, transition={}ms, sleep_mode={}",
        project_id,
        output_path,
        transition_duration_ms.unwrap_or(500),
        sleep_mode_enabled
    );

    // Load sections from DB to get section_order
    let sections = {
        let db_guard = db.lock().map_err(|e| AppError::Database(e.to_string()))?;
        db_guard.list_sections(&project_id)?
    };

    // Resolve section video file paths
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::FileSystem(format!("Failed to resolve app data dir: {}", e)))?;

    let mut section_videos: Vec<SectionVideoFile> = Vec::new();
    for section in &sections {
        let video_path = app_data_dir
            .join("projects")
            .join(&project_id)
            .join("export")
            .join("sections")
            .join(&section.id)
            .join("output.mp4");

        if video_path.exists() {
            // Get duration from file metadata or use a default
            let duration_ms = get_video_duration_ms(&video_path.to_string_lossy()).unwrap_or(0);

            section_videos.push(SectionVideoFile {
                section_id: section.id.clone(),
                section_order: section.section_order,
                file_path: video_path.to_string_lossy().to_string(),
                duration_ms,
            });
        }
    }

    if section_videos.is_empty() {
        return Err(AppError::FFmpeg(
            "No section videos found. Generate section videos first.".to_string(),
        ));
    }

    // Sort by section_order
    section_videos.sort_by_key(|sv| sv.section_order);

    // Validate each section video has an audio stream.
    // If a section's audio merge failed silently, the video might be video-only.
    // Filter out any video-only sections to prevent ffmpeg concat from crashing.
    let ffmpeg_path_for_probe = find_ffmpeg();
    let ffprobe_path = ffmpeg_path_for_probe.replace("ffmpeg", "ffprobe");
    section_videos.retain(|sv| {
        let has_audio = std::process::Command::new(&ffprobe_path)
            .args([
                "-v",
                "quiet",
                "-select_streams",
                "a:0",
                "-show_entries",
                "stream=codec_name",
                "-of",
                "csv=p=0",
                &sv.file_path,
            ])
            .output()
            .map(|o| o.status.success() && !o.stdout.is_empty())
            .unwrap_or(false);
        if !has_audio {
            info!(
                "[Video Merger] WARNING: Section '{}' has no audio stream, skipping from merge",
                sv.section_id
            );
        }
        has_audio
    });

    if section_videos.is_empty() {
        return Err(AppError::FFmpeg(
            "No valid section videos with audio found.".to_string(),
        ));
    }

    let transition_ms = transition_duration_ms.unwrap_or(500);
    let out_path = Path::new(&output_path);

    // Create output directory if needed
    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            AppError::FileSystem(format!("Failed to create output directory: {}", e))
        })?;
    }

    let app_clone = app.clone();
    let on_progress = move |percent: f32, stage: &str| {
        let _ = app_clone.emit(
            "merge-progress",
            MergeProgress {
                percent,
                stage: stage.to_string(),
            },
        );
    };

    let result = merge_videos(
        &section_videos,
        out_path,
        transition_ms,
        sleep_mode_enabled,
        on_progress,
    )
    .await?;

    info!("[Video Merger] Merge complete: {}", result);
    Ok(result)
}

/// Get video duration in milliseconds using ffprobe.
fn get_video_duration_ms(file_path: &str) -> Result<i64, AppError> {
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
        .map_err(|e| AppError::FFmpeg(format!("Failed to run ffprobe: {}", e)))?;

    if !output.status.success() {
        return Err(AppError::FFmpeg(
            "ffprobe duration query failed".to_string(),
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let duration_secs: f64 = stdout
        .trim()
        .parse()
        .map_err(|_| AppError::FFmpeg("Failed to parse duration".to_string()))?;

    Ok((duration_secs * 1000.0) as i64)
}

// ---- Pure duration calculation functions for testing ----

/// Calculate the final merged video duration given section durations and transition duration.
///
/// Formula: final_duration = sum(section_durations) - (N-1) * transition_duration
/// where N = number of sections.
///
/// The transition duration is clamped per-pair to min(adjacent durations), and the
/// overall result is clamped to be non-negative.
#[cfg(test)]
pub fn calculate_merged_duration(section_durations_ms: &[i64], transition_duration_ms: u32) -> i64 {
    if section_durations_ms.is_empty() {
        return 0;
    }
    if section_durations_ms.len() == 1 {
        return section_durations_ms[0].max(0);
    }

    let sum: i64 = section_durations_ms.iter().sum();
    let n = section_durations_ms.len();

    // Calculate total overlap: sum of effective transitions between each pair
    let mut total_overlap_ms: i64 = 0;
    for i in 0..(n - 1) {
        let min_adjacent = section_durations_ms[i].min(section_durations_ms[i + 1]);
        let effective_transition = (transition_duration_ms as i64).min(min_adjacent);
        total_overlap_ms += effective_transition.max(0);
    }

    (sum - total_overlap_ms).max(0)
}

/// Calculate the effective transition duration between two adjacent sections,
/// clamping to the minimum of their durations.
#[cfg(test)]
pub fn clamp_transition(duration_a_ms: i64, duration_b_ms: i64, transition_ms: u32) -> u32 {
    let min_dur = duration_a_ms.min(duration_b_ms).max(0) as u32;
    transition_ms.min(min_dur)
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    // ---- Property-based tests for video merger duration calculation ----

    proptest! {
        /// **Validates: Requirements 10.3**
        /// Property: final_duration = sum(section_durations) - sum(effective_transitions)
        /// where each effective transition is clamped to min(adjacent durations)
        #[test]
        fn prop_final_duration_formula(
            durations in prop::collection::vec(1i64..=60_000, 2..10),
            transition_ms in 100u32..=2000,
        ) {
            let result = calculate_merged_duration(&durations, transition_ms);

            // Manually compute expected
            let sum: i64 = durations.iter().sum();
            let mut total_overlap: i64 = 0;
            for i in 0..(durations.len() - 1) {
                let min_adj = durations[i].min(durations[i + 1]);
                let eff = (transition_ms as i64).min(min_adj).max(0);
                total_overlap += eff;
            }
            let expected = (sum - total_overlap).max(0);

            prop_assert_eq!(result, expected);
        }

        /// **Validates: Requirements 10.4**
        /// Property: clamping ensures non-negative results
        #[test]
        fn prop_clamping_non_negative(
            durations in prop::collection::vec(0i64..=60_000, 1..15),
            transition_ms in 0u32..=5000,
        ) {
            let result = calculate_merged_duration(&durations, transition_ms);
            prop_assert!(
                result >= 0,
                "Duration should be non-negative but got {}",
                result
            );
        }

        /// **Validates: Requirements 10.5**
        /// Property: single section produces output without transitions (duration unchanged)
        #[test]
        fn prop_single_section_no_transition(
            duration in 1i64..=120_000,
            transition_ms in 100u32..=2000,
        ) {
            let result = calculate_merged_duration(&[duration], transition_ms);
            prop_assert_eq!(
                result, duration,
                "Single section duration should be unchanged: got {} expected {}",
                result, duration
            );
        }

        /// **Validates: Requirements 10.4**
        /// Property: clamp_transition never exceeds min of adjacent durations
        #[test]
        fn prop_clamp_transition_bounded(
            dur_a in 0i64..=60_000,
            dur_b in 0i64..=60_000,
            transition_ms in 0u32..=5000,
        ) {
            let result = clamp_transition(dur_a, dur_b, transition_ms);
            let min_dur = dur_a.min(dur_b).max(0) as u32;
            prop_assert!(
                result <= min_dur,
                "Clamped transition {} exceeds min adjacent duration {}",
                result, min_dur
            );
            prop_assert!(
                result <= transition_ms,
                "Clamped transition {} exceeds requested transition {}",
                result, transition_ms
            );
        }
    }
}
