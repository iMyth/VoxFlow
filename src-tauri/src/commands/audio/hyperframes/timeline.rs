//! Timeline calculation for Hyperframes video export.
//!
//! Computes start_time and duration for each ScriptLine based on
//! AudioFragment.duration_ms and ScriptLine.gap_after_ms.

use std::collections::HashMap;

use crate::core::db::ScriptLineWithMeta;
use crate::core::models::AudioFragment;

/// A single entry in the computed timeline, representing one line of dialogue/narration.
#[derive(Debug, Clone, serde::Serialize)]
pub struct TimelineEntry {
    /// Unique identifier for the script line
    pub line_id: String,
    /// The text content of this line
    pub text: String,
    /// Character name (if assigned)
    pub character_name: Option<String>,
    /// Section title this line belongs to (if any)
    pub section_title: Option<String>,
    /// Start time in seconds
    pub start_time: f64,
    /// Duration in seconds
    pub duration: f64,
}

/// Compute the timeline from pre-loaded script lines and audio fragments.
///
/// Lines are expected to be pre-sorted by line_order (as returned by `load_script_lines`).
/// Lines without a corresponding AudioFragment are skipped.
///
/// The algorithm accumulates time using a cursor:
///   cursor = 0; for each line with audio: start = cursor; cursor += duration + gap
pub fn compute_timeline(
    lines: &[ScriptLineWithMeta],
    fragments: &[AudioFragment],
) -> Vec<TimelineEntry> {
    // Build a map from line_id -> AudioFragment for O(1) lookup
    let frag_map: HashMap<&str, &AudioFragment> =
        fragments.iter().map(|f| (f.line_id.as_str(), f)).collect();

    let mut timeline = Vec::new();
    let mut cursor: f64 = 0.0;

    for line in lines {
        // Skip lines without audio
        let frag = match frag_map.get(line.id.as_str()) {
            Some(f) => f,
            None => continue,
        };

        // Skip fragments without duration info
        let duration_ms = match frag.duration_ms {
            Some(ms) => ms,
            None => continue,
        };

        let duration_secs = duration_ms as f64 / 1000.0;
        let gap_secs = line.gap_after_ms as f64 / 1000.0;

        timeline.push(TimelineEntry {
            line_id: line.id.clone(),
            text: line.text.clone(),
            character_name: line.character_name.clone(),
            section_title: line.section_title.clone(),
            start_time: cursor,
            duration: duration_secs,
        });

        cursor += duration_secs + gap_secs;
    }

    timeline
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::db::ScriptLineWithMeta;
    use crate::core::models::AudioFragment;

    fn make_line(id: &str, order: i32, text: &str, gap_ms: i32) -> ScriptLineWithMeta {
        ScriptLineWithMeta {
            id: id.to_string(),
            project_id: "p1".to_string(),
            line_order: order,
            text: text.to_string(),
            character_id: Some("c1".to_string()),
            gap_after_ms: gap_ms,
            instructions: String::new(),
            section_id: None,
            character_name: Some("旁白".to_string()),
            section_title: None,
        }
    }

    fn make_fragment(line_id: &str, duration_ms: i64) -> AudioFragment {
        AudioFragment {
            id: format!("frag_{}", line_id),
            project_id: "p1".to_string(),
            line_id: line_id.to_string(),
            file_path: format!("/tmp/{}.mp3", line_id),
            duration_ms: Some(duration_ms),
            source: "tts".to_string(),
        }
    }

    #[test]
    fn test_empty_inputs() {
        let result = compute_timeline(&[], &[]);
        assert!(result.is_empty());
    }

    #[test]
    fn test_lines_without_audio_are_skipped() {
        let lines = vec![
            make_line("l1", 1, "Hello", 500),
            make_line("l2", 2, "World", 0),
        ];
        // No audio fragments at all
        let result = compute_timeline(&lines, &[]);
        assert!(result.is_empty());
    }

    #[test]
    fn test_single_line_with_audio() {
        let lines = vec![make_line("l1", 1, "Hello world", 500)];
        let frags = vec![make_fragment("l1", 3000)]; // 3 seconds

        let result = compute_timeline(&lines, &frags);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line_id, "l1");
        assert_eq!(result[0].text, "Hello world");
        assert_eq!(result[0].character_name, Some("旁白".to_string()));
        assert!((result[0].start_time - 0.0).abs() < f64::EPSILON);
        assert!((result[0].duration - 3.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_multiple_lines_accumulate_time() {
        let lines = vec![
            make_line("l1", 1, "First line", 500),   // gap = 0.5s
            make_line("l2", 2, "Second line", 1000), // gap = 1.0s
            make_line("l3", 3, "Third line", 0),
        ];
        let frags = vec![
            make_fragment("l1", 2000), // 2s
            make_fragment("l2", 3000), // 3s
            make_fragment("l3", 1500), // 1.5s
        ];

        let result = compute_timeline(&lines, &frags);
        assert_eq!(result.len(), 3);

        // l1: start=0, duration=2.0
        assert!((result[0].start_time - 0.0).abs() < f64::EPSILON);
        assert!((result[0].duration - 2.0).abs() < f64::EPSILON);

        // l2: start = 0 + 2.0 + 0.5 = 2.5, duration=3.0
        assert!((result[1].start_time - 2.5).abs() < f64::EPSILON);
        assert!((result[1].duration - 3.0).abs() < f64::EPSILON);

        // l3: start = 2.5 + 3.0 + 1.0 = 6.5, duration=1.5
        assert!((result[2].start_time - 6.5).abs() < f64::EPSILON);
        assert!((result[2].duration - 1.5).abs() < f64::EPSILON);
    }

    #[test]
    fn test_skip_lines_without_audio() {
        let lines = vec![
            make_line("l1", 1, "Has audio", 500),
            make_line("l2", 2, "No audio", 200),
            make_line("l3", 3, "Has audio too", 0),
        ];
        let frags = vec![
            make_fragment("l1", 2000), // 2s
            // l2 has no fragment
            make_fragment("l3", 1000), // 1s
        ];

        let result = compute_timeline(&lines, &frags);
        assert_eq!(result.len(), 2);

        // l1: start=0, duration=2.0
        assert_eq!(result[0].line_id, "l1");
        assert!((result[0].start_time - 0.0).abs() < f64::EPSILON);
        assert!((result[0].duration - 2.0).abs() < f64::EPSILON);

        // l3: start = 0 + 2.0 + 0.5 = 2.5 (gap from l1), duration=1.0
        assert_eq!(result[1].line_id, "l3");
        assert!((result[1].start_time - 2.5).abs() < f64::EPSILON);
        assert!((result[1].duration - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_fragment_without_duration_is_skipped() {
        let lines = vec![
            make_line("l1", 1, "Has duration", 500),
            make_line("l2", 2, "No duration", 0),
        ];
        let frags = vec![
            make_fragment("l1", 2000),
            AudioFragment {
                id: "frag_l2".to_string(),
                project_id: "p1".to_string(),
                line_id: "l2".to_string(),
                file_path: "/tmp/l2.mp3".to_string(),
                duration_ms: None, // no duration
                source: "tts".to_string(),
            },
        ];

        let result = compute_timeline(&lines, &frags);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line_id, "l1");
    }

    #[test]
    fn test_section_title_and_character_name_propagated() {
        let lines = vec![ScriptLineWithMeta {
            id: "l1".to_string(),
            project_id: "p1".to_string(),
            line_order: 1,
            text: "Opening line".to_string(),
            character_id: Some("c1".to_string()),
            gap_after_ms: 0,
            instructions: String::new(),
            section_id: Some("s1".to_string()),
            character_name: Some("Alice".to_string()),
            section_title: Some("Chapter 1".to_string()),
        }];
        let frags = vec![make_fragment("l1", 5000)];

        let result = compute_timeline(&lines, &frags);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].character_name, Some("Alice".to_string()));
        assert_eq!(result[0].section_title, Some("Chapter 1".to_string()));
    }
}
