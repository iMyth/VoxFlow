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

/// Compute a local timeline for a single ScriptSection.
///
/// Filters lines by `section_id`, orders by `line_order` ascending, then delegates
/// to the same cursor accumulation logic used by `compute_timeline`, starting at 0.0s.
///
/// Lines without a corresponding AudioFragment (or with `duration_ms == None`) are
/// skipped without accumulating their `gap_after_ms`.
pub fn compute_section_timeline(
    section_id: &str,
    lines: &[ScriptLineWithMeta],
    fragments: &[AudioFragment],
) -> Vec<TimelineEntry> {
    // Filter lines belonging to this section and sort by line_order
    let mut section_lines: Vec<&ScriptLineWithMeta> = lines
        .iter()
        .filter(|l| l.section_id.as_deref() == Some(section_id))
        .collect();
    section_lines.sort_by_key(|l| l.line_order);

    // Build a map from line_id -> AudioFragment for O(1) lookup
    let frag_map: HashMap<&str, &AudioFragment> =
        fragments.iter().map(|f| (f.line_id.as_str(), f)).collect();

    let mut timeline = Vec::new();
    let mut cursor: f64 = 0.0;

    for line in section_lines {
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
    use proptest::prelude::*;

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

    // ---- Tests for compute_section_timeline ----

    fn make_section_line(
        id: &str,
        order: i32,
        text: &str,
        gap_ms: i32,
        section_id: Option<&str>,
    ) -> ScriptLineWithMeta {
        ScriptLineWithMeta {
            id: id.to_string(),
            project_id: "p1".to_string(),
            line_order: order,
            text: text.to_string(),
            character_id: Some("c1".to_string()),
            gap_after_ms: gap_ms,
            instructions: String::new(),
            section_id: section_id.map(|s| s.to_string()),
            character_name: Some("旁白".to_string()),
            section_title: section_id.map(|_| "Test Section".to_string()),
        }
    }

    #[test]
    fn test_section_timeline_excludes_other_sections() {
        let lines = vec![
            make_section_line("l1", 1, "Section A line", 500, Some("sA")),
            make_section_line("l2", 2, "Section B line", 500, Some("sB")),
            make_section_line("l3", 3, "Section A line 2", 0, Some("sA")),
        ];
        let frags = vec![
            make_fragment("l1", 2000),
            make_fragment("l2", 3000),
            make_fragment("l3", 1000),
        ];

        let result = compute_section_timeline("sA", &lines, &frags);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].line_id, "l1");
        assert_eq!(result[1].line_id, "l3");
    }

    #[test]
    fn test_section_timeline_first_entry_starts_at_zero() {
        let lines = vec![
            make_section_line("l1", 1, "First line", 500, Some("s1")),
            make_section_line("l2", 2, "Second line", 0, Some("s1")),
        ];
        let frags = vec![
            make_fragment("l1", 2000),
            make_fragment("l2", 1500),
        ];

        let result = compute_section_timeline("s1", &lines, &frags);
        assert!(!result.is_empty());
        assert!((result[0].start_time - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_section_timeline_empty_section_returns_empty() {
        let lines = vec![
            make_section_line("l1", 1, "Other section", 500, Some("sOther")),
        ];
        let frags = vec![make_fragment("l1", 2000)];

        let result = compute_section_timeline("sEmpty", &lines, &frags);
        assert!(result.is_empty());
    }

    #[test]
    fn test_section_timeline_lines_without_audio_skipped_no_gap() {
        // l1 has audio, l2 has no audio (should be skipped without gap), l3 has audio
        let lines = vec![
            make_section_line("l1", 1, "Has audio", 500, Some("s1")),
            make_section_line("l2", 2, "No audio", 9999, Some("s1")), // large gap should NOT accumulate
            make_section_line("l3", 3, "Has audio too", 0, Some("s1")),
        ];
        let frags = vec![
            make_fragment("l1", 2000), // 2s
            // l2 has no fragment
            make_fragment("l3", 1000), // 1s
        ];

        let result = compute_section_timeline("s1", &lines, &frags);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].line_id, "l1");
        assert_eq!(result[1].line_id, "l3");

        // l3 start = l1.duration + l1.gap = 2.0 + 0.5 = 2.5
        // l2's gap_after_ms (9999) is NOT accumulated
        assert!((result[1].start_time - 2.5).abs() < f64::EPSILON);
    }

    #[test]
    fn test_section_timeline_lines_with_null_duration_skipped_no_gap() {
        // l2 has a fragment but duration_ms is None
        let lines = vec![
            make_section_line("l1", 1, "Has audio", 500, Some("s1")),
            make_section_line("l2", 2, "Null duration", 8000, Some("s1")),
            make_section_line("l3", 3, "Has audio too", 0, Some("s1")),
        ];
        let frags = vec![
            make_fragment("l1", 2000),
            AudioFragment {
                id: "frag_l2".to_string(),
                project_id: "p1".to_string(),
                line_id: "l2".to_string(),
                file_path: "/tmp/l2.mp3".to_string(),
                duration_ms: None,
                source: "tts".to_string(),
            },
            make_fragment("l3", 1000),
        ];

        let result = compute_section_timeline("s1", &lines, &frags);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].line_id, "l1");
        assert_eq!(result[1].line_id, "l3");

        // l3 start = l1.duration + l1.gap = 2.0 + 0.5 = 2.5
        // l2's gap (8000ms) is NOT accumulated
        assert!((result[1].start_time - 2.5).abs() < f64::EPSILON);
    }

    #[test]
    fn test_section_timeline_multiple_lines_accumulate_correctly() {
        let lines = vec![
            make_section_line("l1", 1, "First", 500, Some("s1")),   // gap = 0.5s
            make_section_line("l2", 2, "Second", 1000, Some("s1")), // gap = 1.0s
            make_section_line("l3", 3, "Third", 0, Some("s1")),
        ];
        let frags = vec![
            make_fragment("l1", 2000), // 2s
            make_fragment("l2", 3000), // 3s
            make_fragment("l3", 1500), // 1.5s
        ];

        let result = compute_section_timeline("s1", &lines, &frags);
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
    fn test_section_timeline_orders_by_line_order() {
        // Lines provided out of order
        let lines = vec![
            make_section_line("l3", 3, "Third", 0, Some("s1")),
            make_section_line("l1", 1, "First", 500, Some("s1")),
            make_section_line("l2", 2, "Second", 0, Some("s1")),
        ];
        let frags = vec![
            make_fragment("l1", 1000),
            make_fragment("l2", 2000),
            make_fragment("l3", 1500),
        ];

        let result = compute_section_timeline("s1", &lines, &frags);
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].line_id, "l1");
        assert_eq!(result[1].line_id, "l2");
        assert_eq!(result[2].line_id, "l3");
    }

    // ---- Property-based tests for compute_section_timeline ----

    /// Strategy to generate a vector of ScriptLineWithMeta entries for a target section,
    /// along with matching AudioFragments.
    fn arb_section_data(
        target_section: &'static str,
    ) -> impl Strategy<Value = (Vec<ScriptLineWithMeta>, Vec<AudioFragment>)> {
        // Generate 1..20 lines for the target section, plus 0..5 lines for other sections
        let target_lines = prop::collection::vec(
            (1i64..=30_000, 0i32..=5000), // (duration_ms, gap_after_ms)
            1..20,
        );
        let other_line_count = 0usize..5;

        (target_lines, other_line_count).prop_map(move |(target_data, other_count)| {
            let mut lines = Vec::new();
            let mut frags = Vec::new();
            let mut order = 1;

            for (i, (duration_ms, gap_ms)) in target_data.iter().enumerate() {
                let line_id = format!("line_{}", i);
                lines.push(ScriptLineWithMeta {
                    id: line_id.clone(),
                    project_id: "p1".to_string(),
                    line_order: order,
                    text: format!("Text {}", i),
                    character_id: Some("c1".to_string()),
                    gap_after_ms: *gap_ms,
                    instructions: String::new(),
                    section_id: Some(target_section.to_string()),
                    character_name: Some("Speaker".to_string()),
                    section_title: Some("Section".to_string()),
                });
                frags.push(AudioFragment {
                    id: format!("frag_{}", i),
                    project_id: "p1".to_string(),
                    line_id,
                    file_path: format!("/tmp/{}.mp3", i),
                    duration_ms: Some(*duration_ms),
                    source: "tts".to_string(),
                });
                order += 1;
            }

            // Add some lines from other sections
            for j in 0..other_count {
                let line_id = format!("other_line_{}", j);
                lines.push(ScriptLineWithMeta {
                    id: line_id.clone(),
                    project_id: "p1".to_string(),
                    line_order: order,
                    text: format!("Other {}", j),
                    character_id: None,
                    gap_after_ms: 500,
                    instructions: String::new(),
                    section_id: Some("other_section".to_string()),
                    character_name: None,
                    section_title: Some("Other".to_string()),
                });
                frags.push(AudioFragment {
                    id: format!("other_frag_{}", j),
                    project_id: "p1".to_string(),
                    line_id,
                    file_path: format!("/tmp/other_{}.mp3", j),
                    duration_ms: Some(1000),
                    source: "tts".to_string(),
                });
                order += 1;
            }

            (lines, frags)
        })
    }

    proptest! {
        /// **Validates: Requirements 1.2**
        /// Property: output entries are ordered by start_time ascending
        #[test]
        fn prop_entries_ordered_by_start_time((lines, frags) in arb_section_data("target")) {
            let result = compute_section_timeline("target", &lines, &frags);
            for window in result.windows(2) {
                prop_assert!(
                    window[0].start_time <= window[1].start_time,
                    "Entries not ordered: {} > {}",
                    window[0].start_time,
                    window[1].start_time
                );
            }
        }

        /// **Validates: Requirements 1.1**
        /// Property: first entry always starts at 0.0
        #[test]
        fn prop_first_entry_starts_at_zero((lines, frags) in arb_section_data("target")) {
            let result = compute_section_timeline("target", &lines, &frags);
            if !result.is_empty() {
                prop_assert!(
                    (result[0].start_time - 0.0).abs() < 1e-10,
                    "First entry start_time is {} instead of 0.0",
                    result[0].start_time
                );
            }
        }

        /// **Validates: Requirements 1.4**
        /// Property: start_time[n] == start_time[n-1] + duration[n-1] + gap[n-1]
        #[test]
        fn prop_start_time_formula_holds((lines, frags) in arb_section_data("target")) {
            let result = compute_section_timeline("target", &lines, &frags);

            // Reconstruct expected gaps from the lines (in order)
            let mut section_lines: Vec<&ScriptLineWithMeta> = lines
                .iter()
                .filter(|l| l.section_id.as_deref() == Some("target"))
                .collect();
            section_lines.sort_by_key(|l| l.line_order);

            // Build a map of line_id -> gap_after_ms for included lines
            let frag_map: std::collections::HashMap<&str, i64> = frags
                .iter()
                .filter_map(|f| f.duration_ms.map(|d| (f.line_id.as_str(), d)))
                .collect();

            // Get the ordered list of (duration_ms, gap_ms) for lines that made it into the timeline
            let included: Vec<(f64, f64)> = section_lines
                .iter()
                .filter(|l| frag_map.contains_key(l.id.as_str()))
                .map(|l| {
                    let dur = *frag_map.get(l.id.as_str()).unwrap() as f64 / 1000.0;
                    let gap = l.gap_after_ms as f64 / 1000.0;
                    (dur, gap)
                })
                .collect();

            for i in 1..result.len() {
                let expected = result[i - 1].start_time + included[i - 1].0 + included[i - 1].1;
                let actual = result[i].start_time;
                prop_assert!(
                    (actual - expected).abs() < 1e-9,
                    "start_time[{}] = {} but expected {} (prev_start={} + dur={} + gap={})",
                    i, actual, expected,
                    result[i - 1].start_time, included[i - 1].0, included[i - 1].1
                );
            }
        }

        /// **Validates: Requirements 1.2**
        /// Property: only lines with matching section_id appear in output
        #[test]
        fn prop_only_matching_section_lines((lines, frags) in arb_section_data("target")) {
            let result = compute_section_timeline("target", &lines, &frags);

            // Collect line_ids that belong to the target section
            let target_line_ids: std::collections::HashSet<&str> = lines
                .iter()
                .filter(|l| l.section_id.as_deref() == Some("target"))
                .map(|l| l.id.as_str())
                .collect();

            for entry in &result {
                prop_assert!(
                    target_line_ids.contains(entry.line_id.as_str()),
                    "Entry with line_id '{}' does not belong to target section",
                    entry.line_id
                );
            }
        }

        /// **Validates: Requirements 1.1, 1.4**
        /// Property: total duration (last entry start + last entry duration) is correct
        #[test]
        fn prop_total_duration_correct((lines, frags) in arb_section_data("target")) {
            let result = compute_section_timeline("target", &lines, &frags);

            if result.is_empty() {
                return Ok(());
            }

            // Compute expected total from source data
            let mut section_lines: Vec<&ScriptLineWithMeta> = lines
                .iter()
                .filter(|l| l.section_id.as_deref() == Some("target"))
                .collect();
            section_lines.sort_by_key(|l| l.line_order);

            let frag_map: std::collections::HashMap<&str, i64> = frags
                .iter()
                .filter_map(|f| f.duration_ms.map(|d| (f.line_id.as_str(), d)))
                .collect();

            let included: Vec<(&ScriptLineWithMeta, i64)> = section_lines
                .iter()
                .filter(|l| frag_map.contains_key(l.id.as_str()))
                .map(|l| (*l, *frag_map.get(l.id.as_str()).unwrap()))
                .collect();

            // Expected total = sum of all durations + sum of all gaps except last
            let mut expected_total: f64 = 0.0;
            for (i, (line, dur_ms)) in included.iter().enumerate() {
                expected_total += *dur_ms as f64 / 1000.0;
                if i < included.len() - 1 {
                    expected_total += line.gap_after_ms as f64 / 1000.0;
                }
            }

            // Actual total from timeline: last entry start + last entry duration
            let last = result.last().unwrap();
            let actual_total = last.start_time + last.duration;

            prop_assert!(
                (actual_total - expected_total).abs() < 1e-9,
                "Total duration mismatch: actual={} expected={}",
                actual_total, expected_total
            );
        }
    }
}
