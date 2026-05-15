//! Hyperframes composition validation.
//!
//! Validates generated HTML against the Hyperframes lint specification.
//! Used by both fixed templates (as a sanity check) and AI-generated
//! compositions (to verify LLM output before saving).

/// Validate that an HTML composition conforms to the Hyperframes lint specification.
///
/// Checks for:
/// - Presence of `data-composition-id` on root element
/// - Presence of `window.__timelines` registration
/// - At least one `class="clip"` element with required data attributes
/// - Absence of forbidden patterns (Math.random(), Date.now(), repeat: -1)
///
/// Returns `Ok(())` if valid, or `Err(Vec<String>)` with a list of all
/// validation errors found.
pub fn validate_composition(html: &str) -> Result<(), Vec<String>> {
    let mut errors: Vec<String> = Vec::new();

    // Rule 1: Root element must have data-composition-id
    if !html.contains("data-composition-id=") {
        errors.push("Missing data-composition-id attribute on root element".to_string());
    }

    // Rule 2: Root element must have data-width and data-height (1920x1080)
    if !html.contains("data-width=\"1920\"") {
        errors.push("Missing or incorrect data-width attribute (expected 1920)".to_string());
    }
    if !html.contains("data-height=\"1080\"") {
        errors.push("Missing or incorrect data-height attribute (expected 1080)".to_string());
    }

    // Rule 3: Root element must have data-start and data-duration
    if !html.contains("data-start=") {
        errors.push("Missing data-start attribute on root element".to_string());
    }
    if !html.contains("data-duration=") {
        errors.push("Missing data-duration attribute".to_string());
    }

    // Rule 4: At least one clip element with required attributes
    if !html.contains("class=\"clip") {
        errors.push(
            "No clip elements found (need at least one element with class=\"clip\")".to_string(),
        );
    } else {
        // Verify clip elements have required data attributes
        // We check that data-track-index exists somewhere (clips should have it)
        if !html.contains("data-track-index=") {
            errors.push("Clip elements missing data-track-index attribute".to_string());
        }
    }

    // Rule 5: GSAP timelines must be created with { paused: true }
    if html.contains("gsap.timeline(")
        && !html.contains("paused: true")
        && !html.contains("paused:true")
    {
        errors.push("GSAP timeline must be created with { paused: true }".to_string());
    }

    // Rule 6: GSAP timelines must be registered to window.__timelines
    if !html.contains("window.__timelines") {
        errors.push("Missing window.__timelines registration".to_string());
    }

    // Rule 7: No Math.random() or Date.now() (deterministic rendering)
    if html.contains("Math.random()") {
        errors.push("Forbidden pattern: Math.random() (non-deterministic rendering)".to_string());
    }
    if html.contains("Date.now()") {
        errors.push("Forbidden pattern: Date.now() (non-deterministic rendering)".to_string());
    }

    // Rule 8: No repeat: -1 (infinite loops)
    if html.contains("repeat: -1") || html.contains("repeat:-1") {
        errors.push("Forbidden pattern: repeat: -1 (infinite animation loop)".to_string());
    }

    // Rule 9: Valid HTML structure (basic checks)
    if !html.contains("<!DOCTYPE html>") && !html.contains("<!doctype html>") {
        errors.push("Missing DOCTYPE declaration".to_string());
    }
    if !html.contains("charset") {
        errors.push("Missing charset declaration".to_string());
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::audio::hyperframes::templates::generate_html;
    use crate::commands::audio::hyperframes::timeline::TimelineEntry;

    fn sample_entries() -> Vec<TimelineEntry> {
        vec![
            TimelineEntry {
                line_id: "l1".to_string(),
                text: "在一个风雨交加的夜晚".to_string(),
                character_name: Some("旁白".to_string()),
                section_title: Some("第一章".to_string()),
                start_time: 0.0,
                duration: 3.0,
            },
            TimelineEntry {
                line_id: "l2".to_string(),
                text: "一位旅人来到了小镇".to_string(),
                character_name: Some("旁白".to_string()),
                section_title: Some("第一章".to_string()),
                start_time: 3.5,
                duration: 2.5,
            },
        ]
    }

    #[test]
    fn test_minimal_subtitle_passes_validation() {
        let entries = sample_entries();
        let html = generate_html("minimal-subtitle", &entries).unwrap();
        assert!(
            validate_composition(&html).is_ok(),
            "minimal-subtitle template should pass validation: {:?}",
            validate_composition(&html).err()
        );
    }

    #[test]
    fn test_dialogue_cards_passes_validation() {
        let entries = sample_entries();
        let html = generate_html("dialogue-cards", &entries).unwrap();
        assert!(
            validate_composition(&html).is_ok(),
            "dialogue-cards template should pass validation: {:?}",
            validate_composition(&html).err()
        );
    }

    #[test]
    fn test_chapter_sections_passes_validation() {
        let entries = sample_entries();
        let html = generate_html("chapter-sections", &entries).unwrap();
        assert!(
            validate_composition(&html).is_ok(),
            "chapter-sections template should pass validation: {:?}",
            validate_composition(&html).err()
        );
    }

    #[test]
    fn test_missing_composition_id() {
        let html = r#"<!DOCTYPE html><html><head><meta charset="UTF-8"></head><body>
            <div data-width="1920" data-height="1080" data-start="0" data-duration="5">
                <div class="clip" data-start="0" data-duration="5" data-track-index="1">hi</div>
                <script>
                    window.__timelines = window.__timelines || {};
                    const tl = gsap.timeline({ paused: true });
                    window.__timelines["test"] = tl;
                </script>
            </div></body></html>"#;
        let result = validate_composition(html);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.contains("data-composition-id")));
    }

    #[test]
    fn test_missing_timelines_registration() {
        let html = r#"<!DOCTYPE html><html><head><meta charset="UTF-8"></head><body>
            <div data-composition-id="test" data-width="1920" data-height="1080" data-start="0" data-duration="5">
                <div class="clip" data-start="0" data-duration="5" data-track-index="1">hi</div>
                <script>
                    const tl = gsap.timeline({ paused: true });
                </script>
            </div></body></html>"#;
        let result = validate_composition(html);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.contains("window.__timelines")));
    }

    #[test]
    fn test_forbidden_math_random() {
        let html = r#"<!DOCTYPE html><html><head><meta charset="UTF-8"></head><body>
            <div data-composition-id="test" data-width="1920" data-height="1080" data-start="0" data-duration="5">
                <div class="clip" data-start="0" data-duration="5" data-track-index="1">hi</div>
                <script>
                    window.__timelines = window.__timelines || {};
                    const tl = gsap.timeline({ paused: true });
                    const x = Math.random();
                    window.__timelines["test"] = tl;
                </script>
            </div></body></html>"#;
        let result = validate_composition(html);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.contains("Math.random()")));
    }

    #[test]
    fn test_forbidden_date_now() {
        let html = r#"<!DOCTYPE html><html><head><meta charset="UTF-8"></head><body>
            <div data-composition-id="test" data-width="1920" data-height="1080" data-start="0" data-duration="5">
                <div class="clip" data-start="0" data-duration="5" data-track-index="1">hi</div>
                <script>
                    window.__timelines = window.__timelines || {};
                    const tl = gsap.timeline({ paused: true });
                    const t = Date.now();
                    window.__timelines["test"] = tl;
                </script>
            </div></body></html>"#;
        let result = validate_composition(html);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.contains("Date.now()")));
    }

    #[test]
    fn test_forbidden_infinite_repeat() {
        let html = r#"<!DOCTYPE html><html><head><meta charset="UTF-8"></head><body>
            <div data-composition-id="test" data-width="1920" data-height="1080" data-start="0" data-duration="5">
                <div class="clip" data-start="0" data-duration="5" data-track-index="1">hi</div>
                <script>
                    window.__timelines = window.__timelines || {};
                    const tl = gsap.timeline({ paused: true });
                    tl.to(".box", { x: 100, repeat: -1 });
                    window.__timelines["test"] = tl;
                </script>
            </div></body></html>"#;
        let result = validate_composition(html);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.contains("repeat: -1")));
    }

    #[test]
    fn test_missing_doctype() {
        let html = r#"<html><head><meta charset="UTF-8"></head><body>
            <div data-composition-id="test" data-width="1920" data-height="1080" data-start="0" data-duration="5">
                <div class="clip" data-start="0" data-duration="5" data-track-index="1">hi</div>
                <script>
                    window.__timelines = window.__timelines || {};
                    const tl = gsap.timeline({ paused: true });
                    window.__timelines["test"] = tl;
                </script>
            </div></body></html>"#;
        let result = validate_composition(html);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.contains("DOCTYPE")));
    }

    #[test]
    fn test_no_clip_elements() {
        let html = r#"<!DOCTYPE html><html><head><meta charset="UTF-8"></head><body>
            <div data-composition-id="test" data-width="1920" data-height="1080" data-start="0" data-duration="5">
                <div>no clips here</div>
                <script>
                    window.__timelines = window.__timelines || {};
                    const tl = gsap.timeline({ paused: true });
                    window.__timelines["test"] = tl;
                </script>
            </div></body></html>"#;
        let result = validate_composition(html);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.contains("clip")));
    }

    #[test]
    fn test_multiple_errors_reported() {
        let html = "<html><body><div>nothing valid</div></body></html>";
        let result = validate_composition(html);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        // Should report multiple issues
        assert!(errors.len() > 3);
    }

    #[test]
    fn test_valid_minimal_html() {
        let html = r#"<!DOCTYPE html><html><head><meta charset="UTF-8"></head><body>
            <div data-composition-id="test" data-width="1920" data-height="1080" data-start="0" data-duration="5">
                <div class="clip" data-start="0" data-duration="5" data-track-index="1">content</div>
                <script>
                    window.__timelines = window.__timelines || {};
                    const tl = gsap.timeline({ paused: true });
                    window.__timelines["test"] = tl;
                </script>
            </div></body></html>"#;
        assert!(validate_composition(html).is_ok());
    }
}
