//! Section audio merger for paragraph-level video generation.
//!
//! Concatenates AudioFragments for a single ScriptSection into a single MP3 file,
//! inserting silence gaps between fragments as specified by gap_after_ms.

use std::collections::HashMap;
use std::path::Path;

use log::info;

use crate::commands::audio::ffmpeg::find_ffmpeg;
use crate::core::db::ScriptLineWithMeta;
use crate::core::error::AppError;
use crate::core::models::AudioFragment;

/// Result of merging audio for a section.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SectionAudioResult {
    /// Path to the output MP3 file.
    pub file_path: String,
    /// Total duration of the merged audio in milliseconds.
    pub total_duration_ms: i64,
}

/// Concatenate AudioFragments for a section into a single MP3.
///
/// Filters lines by `section_id`, orders by `line_order`, and builds an ffmpeg
/// filter graph that concatenates audio files with silence gaps:
/// - `gap_after_ms` silence is inserted after each fragment EXCEPT the last line.
/// - Lines without an AudioFragment get silence of their `gap_after_ms` duration inserted.
/// - Returns error if ALL lines in the section lack audio.
/// - Output: MP3 format, libmp3lame codec, 22050 Hz sample rate, 192 kbps bitrate.
pub async fn merge_section_audio(
    section_id: &str,
    lines: &[ScriptLineWithMeta],
    fragments: &[AudioFragment],
    output_path: &Path,
) -> Result<SectionAudioResult, AppError> {
    // Filter lines by section_id and sort by line_order
    let mut section_lines: Vec<&ScriptLineWithMeta> = lines
        .iter()
        .filter(|l| l.section_id.as_deref() == Some(section_id))
        .collect();
    section_lines.sort_by_key(|l| l.line_order);

    if section_lines.is_empty() {
        return Err(AppError::FFmpeg(format!(
            "No lines found for section '{}'",
            section_id
        )));
    }

    // Build a map from line_id -> AudioFragment for O(1) lookup
    let frag_map: HashMap<&str, &AudioFragment> =
        fragments.iter().map(|f| (f.line_id.as_str(), f)).collect();

    // Check if at least one line has audio
    let has_any_audio = section_lines.iter().any(|line| {
        frag_map
            .get(line.id.as_str())
            .and_then(|f| f.duration_ms)
            .is_some()
    });

    if !has_any_audio {
        return Err(AppError::FFmpeg(format!(
            "All lines in section '{}' lack audio fragments — cannot produce section audio",
            section_id
        )));
    }

    // Build the ffmpeg filter graph
    // Strategy: use anullsrc for silence segments, file inputs for audio fragments,
    // then concat them all together.
    let ffmpeg_bin = find_ffmpeg();
    let line_count = section_lines.len();

    // Collect input files and build segment descriptors
    // Each segment is either an audio file or a silence duration
    #[derive(Debug)]
    enum Segment {
        /// An audio file input at the given ffmpeg input index
        Audio { input_idx: usize, duration_ms: i64 },
        /// A silence segment of the given duration
        Silence { duration_ms: i32 },
    }

    let mut input_files: Vec<String> = Vec::new();
    let mut segments: Vec<Segment> = Vec::new();
    let mut total_duration_ms: i64 = 0;

    for (i, line) in section_lines.iter().enumerate() {
        let is_last = i == line_count - 1;

        match frag_map.get(line.id.as_str()).and_then(|f| {
            f.duration_ms.map(|d| (f.file_path.clone(), d))
        }) {
            Some((file_path, duration_ms)) => {
                // This line has audio
                let input_idx = input_files.len();
                input_files.push(file_path);
                segments.push(Segment::Audio { input_idx, duration_ms });
                total_duration_ms += duration_ms;

                // Add gap silence after this fragment (except for the last line)
                if !is_last && line.gap_after_ms > 0 {
                    segments.push(Segment::Silence {
                        duration_ms: line.gap_after_ms,
                    });
                    total_duration_ms += line.gap_after_ms as i64;
                }
            }
            None => {
                // Line without audio: insert silence of gap_after_ms duration
                if line.gap_after_ms > 0 {
                    segments.push(Segment::Silence {
                        duration_ms: line.gap_after_ms,
                    });
                    total_duration_ms += line.gap_after_ms as i64;
                }
            }
        }
    }

    // If only silence segments remain (shouldn't happen due to has_any_audio check above),
    // but handle gracefully
    if segments.is_empty() {
        return Err(AppError::FFmpeg(
            "No audio segments to merge".to_string(),
        ));
    }

    // Build ffmpeg arguments
    let mut args: Vec<String> = Vec::new();
    args.push("-y".to_string());

    // Add input files
    for file_path in &input_files {
        args.push("-i".to_string());
        args.push(file_path.clone());
    }

    // Build filter_complex string
    let mut filter = String::new();
    let mut segment_labels: Vec<String> = Vec::new();
    let mut silence_idx = 0;

    for segment in &segments {
        match segment {
            Segment::Audio { input_idx, .. } => {
                // Resample audio input to 22050 Hz mono for consistency
                let label = format!("a{}", input_idx);
                filter.push_str(&format!(
                    "[{}:a]aresample=22050,aformat=sample_fmts=fltp:channel_layouts=mono[{}];",
                    input_idx, label
                ));
                segment_labels.push(format!("[{}]", label));
            }
            Segment::Silence { duration_ms } => {
                let duration_sec = *duration_ms as f64 / 1000.0;
                let label = format!("sil{}", silence_idx);
                filter.push_str(&format!(
                    "anullsrc=r=22050:cl=mono[s{s}];[s{s}]atrim=0:{dur:.3}[{label}];",
                    s = silence_idx,
                    dur = duration_sec,
                    label = label
                ));
                segment_labels.push(format!("[{}]", label));
                silence_idx += 1;
            }
        }
    }

    // Concat all segments
    let n = segment_labels.len();
    for label in &segment_labels {
        filter.push_str(label);
    }
    filter.push_str(&format!("concat=n={}:v=0:a=1[out]", n));

    args.push("-filter_complex".to_string());
    args.push(filter);
    args.push("-map".to_string());
    args.push("[out]".to_string());

    // Output format: MP3, libmp3lame, 22050 Hz, 192 kbps
    args.push("-c:a".to_string());
    args.push("libmp3lame".to_string());
    args.push("-ar".to_string());
    args.push("22050".to_string());
    args.push("-b:a".to_string());
    args.push("192k".to_string());

    let output_str = output_path.to_string_lossy().to_string();
    args.push(output_str.clone());

    info!(
        "[Section Audio] Merging {} segments for section '{}' -> {}",
        n, section_id, output_str
    );

    // Run ffmpeg
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
            info!(
                "[Section Audio] Merge complete for section '{}': total_duration={}ms",
                section_id, total_duration_ms
            );
            Ok(SectionAudioResult {
                file_path: output_path.to_string_lossy().to_string(),
                total_duration_ms,
            })
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(AppError::FFmpeg(format!(
                "ffmpeg section audio merge failed for section '{}': {}",
                section_id,
                stderr.chars().take(500).collect::<String>()
            )))
        }
        Err(e) => {
            if e.kind() == std::io::ErrorKind::NotFound {
                Err(AppError::FFmpeg(
                    "FFmpeg not found. Please install FFmpeg to merge section audio.".to_string(),
                ))
            } else {
                Err(AppError::FFmpeg(format!(
                    "Failed to execute ffmpeg: {}",
                    e
                )))
            }
        }
    }
}


// ---- Pure duration calculation functions for testing ----

/// Represents a line's audio contribution for duration calculation.
#[derive(Debug, Clone)]
pub struct LineAudioInfo {
    /// Whether this line has an audio fragment with a valid duration.
    pub has_audio: bool,
    /// Duration of the audio fragment in ms (only meaningful if has_audio is true).
    pub duration_ms: i64,
    /// Gap after this line in ms.
    pub gap_after_ms: i32,
    /// Whether this is the last line in the section.
    pub is_last: bool,
}

/// Calculate the expected total duration of merged section audio.
///
/// Rules:
/// - Lines with audio contribute their duration_ms
/// - Gap silence (gap_after_ms) is inserted after each audio fragment EXCEPT the last line
/// - Lines without audio contribute their gap_after_ms as silence
///
/// Returns total duration in milliseconds.
pub fn calculate_section_audio_duration(line_infos: &[LineAudioInfo]) -> i64 {
    let mut total: i64 = 0;

    for info in line_infos {
        if info.has_audio {
            total += info.duration_ms;
            // Add gap after audio fragment, except for the last line
            if !info.is_last && info.gap_after_ms > 0 {
                total += info.gap_after_ms as i64;
            }
        } else {
            // Lines without audio contribute their gap_after_ms as silence
            if info.gap_after_ms > 0 {
                total += info.gap_after_ms as i64;
            }
        }
    }

    total
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    /// Strategy to generate a list of LineAudioInfo entries where at least one has audio.
    fn arb_line_audio_infos() -> impl Strategy<Value = Vec<LineAudioInfo>> {
        // Generate 1..15 lines, each with random audio presence, duration, and gap
        prop::collection::vec(
            (prop::bool::ANY, 1i64..=30_000, 0i32..=5000),
            1..15,
        )
        .prop_filter("at least one line must have audio", |v| {
            v.iter().any(|(has_audio, _, _)| *has_audio)
        })
        .prop_map(|entries| {
            let len = entries.len();
            entries
                .into_iter()
                .enumerate()
                .map(|(i, (has_audio, duration_ms, gap_after_ms))| LineAudioInfo {
                    has_audio,
                    duration_ms,
                    gap_after_ms,
                    is_last: i == len - 1,
                })
                .collect()
        })
    }

    proptest! {
        /// **Validates: Requirements 7.1, 7.2**
        /// Property: output duration = sum of fragment durations + sum of gaps (excluding last)
        /// for lines with audio, plus gap_after_ms for lines without audio.
        #[test]
        fn prop_output_duration_formula(infos in arb_line_audio_infos()) {
            let result = calculate_section_audio_duration(&infos);

            // Manually compute expected
            let mut expected: i64 = 0;
            for info in &infos {
                if info.has_audio {
                    expected += info.duration_ms;
                    if !info.is_last && info.gap_after_ms > 0 {
                        expected += info.gap_after_ms as i64;
                    }
                } else {
                    if info.gap_after_ms > 0 {
                        expected += info.gap_after_ms as i64;
                    }
                }
            }

            prop_assert_eq!(
                result, expected,
                "Duration mismatch: got {} expected {}",
                result, expected
            );
        }

        /// **Validates: Requirements 7.3**
        /// Property: lines without fragments contribute only their gap_after_ms as silence
        #[test]
        fn prop_lines_without_fragments_contribute_silence(
            gap_values in prop::collection::vec(0i32..=5000, 1..10),
            audio_duration in 1i64..=30_000,
        ) {
            // Create a scenario: first line has audio, remaining lines have no audio
            let len = gap_values.len() + 1;
            let mut infos = Vec::with_capacity(len);

            // First line: has audio
            infos.push(LineAudioInfo {
                has_audio: true,
                duration_ms: audio_duration,
                gap_after_ms: 500, // some gap
                is_last: len == 1,
            });

            // Remaining lines: no audio, only contribute gap_after_ms
            for (i, &gap) in gap_values.iter().enumerate() {
                infos.push(LineAudioInfo {
                    has_audio: false,
                    duration_ms: 0, // irrelevant since has_audio is false
                    gap_after_ms: gap,
                    is_last: i == gap_values.len() - 1,
                });
            }

            let result = calculate_section_audio_duration(&infos);

            // Expected: audio_duration + gap from first line (if not last) + sum of gap_values (for positive gaps)
            let mut expected = audio_duration;
            if len > 1 {
                expected += 500; // gap from first line
            }
            for &gap in &gap_values {
                if gap > 0 {
                    expected += gap as i64;
                }
            }

            prop_assert_eq!(
                result, expected,
                "Lines without audio should contribute only gap_after_ms: got {} expected {}",
                result, expected
            );
        }
    }
}
