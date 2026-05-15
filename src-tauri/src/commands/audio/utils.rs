/// Detect which script lines are missing audio fragments.
///
/// Pure function: takes script line IDs and audio fragment line IDs,
/// returns the IDs of script lines that have no corresponding audio fragment.
#[allow(dead_code)]
pub fn detect_missing_audio(
    script_line_ids: &[String],
    audio_fragment_line_ids: &[String],
) -> Vec<String> {
    let audio_set: std::collections::HashSet<&str> =
        audio_fragment_line_ids.iter().map(|s| s.as_str()).collect();

    script_line_ids
        .iter()
        .filter(|id| !audio_set.contains(id.as_str()))
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- detect_missing_audio tests ----

    #[test]
    fn test_detect_missing_audio_all_present() {
        let script_ids = vec!["l1".to_string(), "l2".to_string(), "l3".to_string()];
        let audio_ids = vec!["l1".to_string(), "l2".to_string(), "l3".to_string()];
        let missing = detect_missing_audio(&script_ids, &audio_ids);
        assert!(missing.is_empty());
    }

    #[test]
    fn test_detect_missing_audio_some_missing() {
        let script_ids = vec!["l1".to_string(), "l2".to_string(), "l3".to_string()];
        let audio_ids = vec!["l1".to_string(), "l3".to_string()];
        let missing = detect_missing_audio(&script_ids, &audio_ids);
        assert_eq!(missing, vec!["l2".to_string()]);
    }

    #[test]
    fn test_detect_missing_audio_all_missing() {
        let script_ids = vec!["l1".to_string(), "l2".to_string()];
        let audio_ids: Vec<String> = vec![];
        let missing = detect_missing_audio(&script_ids, &audio_ids);
        assert_eq!(missing, vec!["l1".to_string(), "l2".to_string()]);
    }

    #[test]
    fn test_detect_missing_audio_empty_script() {
        let script_ids: Vec<String> = vec![];
        let audio_ids = vec!["l1".to_string()];
        let missing = detect_missing_audio(&script_ids, &audio_ids);
        assert!(missing.is_empty());
    }

    #[test]
    fn test_detect_missing_audio_preserves_order() {
        let script_ids = vec!["l3".to_string(), "l1".to_string(), "l2".to_string()];
        let audio_ids = vec!["l1".to_string()];
        let missing = detect_missing_audio(&script_ids, &audio_ids);
        assert_eq!(missing, vec!["l3".to_string(), "l2".to_string()]);
    }
}
