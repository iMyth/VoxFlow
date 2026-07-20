//! HTML post-processing fixes for Hyperframes compositions.
//!
//! Each function takes LLM-generated HTML and applies a specific safety-net fix.
//! Together they form the post-processing pipeline that ensures render compatibility.

use log::info;
use regex::Regex;

use super::html_utils::{
    extract_attr_value, find_composition_tag_range, replace_or_add_attr, set_data_duration_in_tag,
};
use super::timeline::TimelineEntry;

// ─── Font Fixes ──────────────────────────────────────────────────────────────

/// Fix CSS font variables by replacing var(--font-*) with actual font names.
/// Hyperframes cannot map CSS variables to fonts, so we inline them.
pub fn fix_css_font_variables(html: &str) -> String {
    let mut result = html.to_string();

    // Standard --font-* naming
    result = result
        .replace("var(--font-body)", "'DM Sans', sans-serif")
        .replace("var(--font-heading)", "'Space Grotesk', sans-serif")
        .replace("var(--font-mono)", "'JetBrains Mono', monospace")
        .replace("var(--font-main)", "'DM Sans', sans-serif")
        .replace("var(--font-display)", "'Space Grotesk', sans-serif")
        .replace("var(--font-serif)", "'Libre Baskerville', serif")
        .replace("var(--font-sans)", "'DM Sans', sans-serif");

    // Short naming (--mono, --sans, --serif)
    result = result
        .replace("var(--mono)", "'JetBrains Mono', monospace")
        .replace("var(--sans)", "'DM Sans', sans-serif")
        .replace("var(--serif)", "'Libre Baskerville', serif")
        .replace("var(--body)", "'DM Sans', sans-serif")
        .replace("var(--heading)", "'Space Grotesk', sans-serif");

    // Suffix naming (--body-font, --heading-font)
    result = result
        .replace("var(--body-font)", "'DM Sans', sans-serif")
        .replace("var(--heading-font)", "'Space Grotesk', sans-serif")
        .replace("var(--mono-font)", "'JetBrains Mono', monospace");

    // Primary/secondary naming
    result = result
        .replace("var(--font-primary)", "'DM Sans', sans-serif")
        .replace("var(--font-secondary)", "'Space Grotesk', sans-serif");

    // Fix kebab-case internal IDs → display names
    for (kebab, display) in &[
        ("jetbrains-mono", "JetBrains Mono"),
        ("dm-sans", "DM Sans"),
        ("space-grotesk", "Space Grotesk"),
        ("libre-baskerville", "Libre Baskerville"),
        ("archivo-black", "Archivo Black"),
        ("comic-neue", "Comic Neue"),
        ("playfair-display", "Playfair Display"),
        ("fira-code", "Fira Code"),
    ] {
        result = result
            .replace(&format!("'{kebab}'"), &format!("'{display}'"))
            .replace(&format!("\"{kebab}\""), &format!("\"{display}\""));
    }

    result
}

/// Strip all specific font names from font-family declarations, keeping only
/// generic CSS families. Hyperframes treats any unmapped font name as fatal.
pub fn sanitize_unsupported_fonts(html: &str) -> String {
    sanitize_fonts_impl(html)
}

/// Public variant for use by the render retry path.
#[allow(dead_code)]
pub fn sanitize_fonts_for_retry(html: &str) -> String {
    sanitize_fonts_impl(html)
}

fn sanitize_fonts_impl(html: &str) -> String {
    let mut result = html.to_string();

    // Pattern 1: Explicit font-family declarations
    let font_family_re = Regex::new(r#"font-family\s*:\s*([^;}"]+)"#).unwrap();
    let matches: Vec<_> = font_family_re
        .find_iter(&result)
        .map(|m| (m.start(), m.end(), m.as_str().to_string()))
        .collect();

    for (start, end, matched) in matches.into_iter().rev() {
        let colon_pos = matched.find(':').unwrap_or(0);
        let value_part = matched[colon_pos + 1..].trim().to_lowercase();

        if is_all_generic(&value_part) {
            continue;
        }

        let replacement = infer_generic_family(&value_part);
        result.replace_range(start..end, &format!("font-family: {replacement}"));
    }

    // Pattern 2: CSS font shorthand
    let font_shorthand_re = Regex::new(r#"(?i)\bfont\s*:\s*([^;}"]*)"#).unwrap();
    let size_re = Regex::new(r#"[\d.]+(?:px|em|rem|pt|vw|vh|%|ex|ch)(?:\s*/\s*[\d.]+)?"#).unwrap();

    let matches: Vec<_> = font_shorthand_re
        .find_iter(&result)
        .map(|m| (m.start(), m.end(), m.as_str().to_string()))
        .collect();

    for (start, end, matched) in matches.into_iter().rev() {
        let prop_name = matched
            .split(':')
            .next()
            .unwrap_or("")
            .trim()
            .to_lowercase();
        if prop_name != "font" {
            continue;
        }

        let colon_pos = matched.find(':').unwrap_or(0);
        let value_part = matched[colon_pos + 1..].trim();
        let lower_value = value_part.to_lowercase();

        let has_specific_font = value_part.contains('"')
            || value_part.contains('\'')
            || lower_value.contains("noto")
            || lower_value.contains("song")
            || lower_value.contains("hei")
            || lower_value.contains("kai")
            || lower_value.contains("ming")
            || lower_value.contains("gothic");

        if !has_specific_font {
            continue;
        }

        if let Some(size_match) = size_re.find(value_part) {
            let family_start = size_match.end();
            let family_part = value_part[family_start..].trim();

            if !is_all_generic(&family_part.to_lowercase()) {
                let replacement = infer_generic_family(&family_part.to_lowercase());
                result.replace_range(
                    start..end,
                    &format!("font: {} {replacement}", value_part[..family_start].trim()),
                );
            }
        }
    }

    // Pattern 3: Remove @font-face blocks
    let fontface_re = Regex::new(r#"(?is)@font-face\s*\{[^}]*\}"#).unwrap();
    result = fontface_re.replace_all(&result, "").to_string();

    result
}

fn is_all_generic(value: &str) -> bool {
    value.split(',').all(|f| {
        let f = f.trim().trim_matches('\'').trim_matches('"').trim();
        matches!(
            f,
            "sans-serif"
                | "serif"
                | "monospace"
                | "cursive"
                | "fantasy"
                | "inherit"
                | "initial"
                | "unset"
                | ""
        )
    })
}

fn infer_generic_family(value: &str) -> &'static str {
    if value.contains("mono") || value.contains("code") || value.contains("courier") {
        "monospace"
    } else if (value.contains("serif")
        || value.contains("song")
        || value.contains("ming")
        || value.contains("baskerville")
        || value.contains("georgia")
        || value.contains("times"))
        && !value.contains("sans")
    {
        "serif"
    } else {
        "sans-serif"
    }
}

// ─── Hyperframes Interface Injection ────────────────────────────────────────

/// Ensure required Hyperframes interfaces exist (window.__hf and window.__timelines).
/// This safety-net prevents render failures when the LLM omits required JS setup.
pub fn ensure_hyperframes_interfaces(html: &str, duration: f64) -> String {
    let fallback_script = format!(
        r#"<script>
// === Hyperframes safety-net (always injected) ===
(function() {{
  window.__timelines = window.__timelines || {{}};
  if (!window.__timelines['ai-generated']) {{
    if (typeof gsap !== 'undefined') {{
      window.__timelines['ai-generated'] = gsap.timeline({{ paused: true }});
    }} else {{
      window.__timelines['ai-generated'] = {{ seek: function() {{}}, duration: function() {{ return {dur:.2}; }} }};
    }}
  }}
  window.__hf = window.__hf || {{}};
  window.__hf.duration = {dur:.2};
  if (!window.__hf.seek) {{
    window.__hf.seek = function(time) {{
      if (window.__timelines) {{
        Object.values(window.__timelines).forEach(function(tl) {{
          if (tl && typeof tl.seek === 'function') tl.seek(time);
        }});
      }}
    }};
  }}
}})();
</script>"#,
        dur = duration,
    );

    let mut result = html.to_string();
    if let Some(pos) = result.rfind("</body>") {
        result.insert_str(pos, &fallback_script);
    } else if let Some(pos) = result.rfind("</html>") {
        result.insert_str(pos, &fallback_script);
    } else {
        result.push_str(&fallback_script);
    }

    info!(
        "[PostProcess] Injected safety-net interfaces (duration={:.2})",
        duration
    );
    result
}

// ─── Duration Fixes ──────────────────────────────────────────────────────────

/// Ensure the root composition element has the correct `data-duration` attribute.
/// Hyperframes uses this to determine frame count — wrong values produce videos
/// that are too long (trailing blank frames) or too short (cut off).
pub fn ensure_root_duration(html: &str, duration: f64) -> String {
    let mut result = html.to_string();
    let mut corrections = 0;
    let mut comp_is_on_html_tag = false;

    if let Some((tag_start, tag_end)) = find_composition_tag_range(&result) {
        let tag_name_end = result[tag_start + 1..]
            .find(|c: char| c.is_whitespace() || c == '>')
            .map(|i| i + tag_start + 1)
            .unwrap_or(tag_start + 1);
        let tag_name = result[tag_start + 1..tag_name_end].to_string();
        comp_is_on_html_tag = tag_name.eq_ignore_ascii_case("html");

        let (new_html, changed) = set_data_duration_in_tag(&result, tag_start, tag_end, duration);
        if changed {
            result = new_html;
            corrections += 1;
            info!(
                "[PostProcess] Set composition data-duration to {:.3} on <{}>",
                duration, tag_name
            );
        }
    } else {
        info!("[PostProcess] No composition element with data-composition-id found");
    }

    if !comp_is_on_html_tag {
        if let Some(html_start) = result.find("<html") {
            if let Some(html_end_rel) = result[html_start..].find('>') {
                let html_end = html_start + html_end_rel;
                if result[html_start..=html_end].contains("data-duration=") {
                    let (new_html, changed) =
                        set_data_duration_in_tag(&result, html_start, html_end, duration);
                    if changed {
                        result = new_html;
                        corrections += 1;
                        info!(
                            "[PostProcess] Fixed <html> data-duration to {:.3}",
                            duration
                        );
                    }
                }
            }
        }
    }

    if corrections == 0 {
        info!(
            "[PostProcess] data-duration already correct ({:.3}), no changes needed",
            duration
        );
    }

    result
}

// ─── Clip Timing Fixes ───────────────────────────────────────────────────────

/// Find all clip element tag ranges in the HTML.
fn find_clip_tag_ranges(html: &str) -> Vec<(usize, usize)> {
    let mut positions = Vec::new();
    let lower = html.to_ascii_lowercase();
    let mut search_from = 0;

    while let Some(rel) = lower[search_from..].find("class=\"clip") {
        let abs_pos = search_from + rel;
        if let Some(tag_start) = html[..abs_pos].rfind('<') {
            if let Some(end_rel) = html[tag_start..].find('>') {
                positions.push((tag_start, tag_start + end_rel));
            }
        }
        search_from = abs_pos + 1;
    }
    positions
}

/// Ensure clip elements have correct data-start and data-duration matching the timeline.
/// Uses proximity matching: each clip is matched to the timeline entry with the closest
/// existing data-start value, with a fallback to order-based matching.
pub fn ensure_clip_timing(html: &str, entries: &[TimelineEntry]) -> String {
    if entries.is_empty() {
        return html.to_string();
    }

    let clip_positions = find_clip_tag_ranges(html);
    if clip_positions.is_empty() {
        return html.to_string();
    }

    let mut result = html.to_string();

    // Extract current data-start values for proximity matching
    let clips_with_start: Vec<(usize, f64)> = clip_positions
        .iter()
        .enumerate()
        .filter_map(|(i, &(start, end))| {
            extract_attr_value(&result[start..=end], "data-start")
                .and_then(|v| v.parse::<f64>().ok())
                .map(|t| (i, t))
        })
        .collect();

    // Match clips to timeline entries by proximity
    let mut entry_matched = vec![false; entries.len()];
    let mut match_pairs: Vec<(usize, usize)> = Vec::new();

    for &(clip_idx, clip_start) in &clips_with_start {
        let (best_entry, best_dist) = entries
            .iter()
            .enumerate()
            .filter(|&(i, _)| !entry_matched[i])
            .map(|(i, e)| (i, (clip_start - e.start_time).abs()))
            .fold(None::<(usize, f64)>, |acc, (i, d)| {
                acc.filter(|&(_, best)| best < d)
                    .map_or(Some((i, d)), |_| acc)
            })
            .unwrap_or((0, f64::MAX));

        if best_dist < 5.0 {
            entry_matched[best_entry] = true;
            match_pairs.push((clip_idx, best_entry));
        }
    }

    // Fallback: order-based matching for unmatched entries
    if match_pairs.len() < entries.len() && clip_positions.len() >= entries.len() {
        let matched_clips: std::collections::HashSet<usize> =
            match_pairs.iter().map(|(c, _)| *c).collect();
        let unmatched_clips: Vec<usize> = (0..clip_positions.len())
            .filter(|i| !matched_clips.contains(i))
            .collect();
        let unmatched_entries: Vec<usize> =
            (0..entries.len()).filter(|i| !entry_matched[*i]).collect();

        for i in 0..unmatched_entries.len().min(unmatched_clips.len()) {
            match_pairs.push((unmatched_clips[i], unmatched_entries[i]));
        }
    }

    // Apply corrections (reverse order to preserve byte positions)
    match_pairs.sort_by_key(|b| std::cmp::Reverse(b.0));
    let mut corrections = 0;

    for (clip_idx, entry_idx) in &match_pairs {
        let (tag_start, tag_end) = clip_positions[*clip_idx];
        let entry = &entries[*entry_idx];
        let tag_content = result[tag_start..=tag_end].to_string();

        let new_tag = replace_or_add_attr(
            &replace_or_add_attr(
                &tag_content,
                "data-start",
                &format!("{:.3}", entry.start_time),
            ),
            "data-duration",
            &format!("{:.3}", entry.duration),
        );

        if new_tag != tag_content {
            corrections += 1;
            result.replace_range(tag_start..=tag_end, &new_tag);
        }
    }

    if corrections > 0 {
        info!(
            "[PostProcess] Corrected timing on {}/{} clip elements",
            corrections,
            clip_positions.len()
        );
    }

    result
}

/// Clamp any clip whose `data-start + data-duration` extends beyond `total_duration`.
pub fn clamp_overflow_clips(html: &str, total_duration: f64) -> String {
    if total_duration <= 0.0 {
        return html.to_string();
    }

    let clip_positions = find_clip_tag_ranges(html);
    let mut result = html.to_string();
    let mut clamped = 0;

    for &(tag_start, tag_end) in clip_positions.iter().rev() {
        let tag_content = result[tag_start..=tag_end].to_string();
        let start =
            extract_attr_value(&tag_content, "data-start").and_then(|v| v.parse::<f64>().ok());
        let dur =
            extract_attr_value(&tag_content, "data-duration").and_then(|v| v.parse::<f64>().ok());

        if let (Some(start), Some(dur)) = (start, dur) {
            if start + dur > total_duration + 0.05 {
                let clamped_dur = (total_duration - start).max(0.0);
                let new_tag = replace_or_add_attr(
                    &tag_content,
                    "data-duration",
                    &format!("{:.3}", clamped_dur),
                );
                if new_tag != tag_content {
                    result.replace_range(tag_start..=tag_end, &new_tag);
                    clamped += 1;
                }
            }
        }
    }

    if clamped > 0 {
        info!(
            "[PostProcess] Clamped {} clip(s) beyond total duration {:.3}s",
            clamped, total_duration
        );
    }

    result
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fix_css_font_variables() {
        let html = r#"<style>
            body { font-family: var(--font-body); }
            h1 { font-family: var(--font-heading); }
            code { font-family: var(--font-mono); }
        </style>"#;

        let fixed = fix_css_font_variables(html);
        assert!(!fixed.contains("var(--font-"));
        assert!(fixed.contains("'DM Sans', sans-serif"));
        assert!(fixed.contains("'Space Grotesk', sans-serif"));
        assert!(fixed.contains("'JetBrains Mono', monospace"));
    }

    #[test]
    fn test_fix_css_font_variables_extended_patterns() {
        let html = r#"<style>
            .mono { font-family: var(--mono); }
            .sans { font-family: var(--sans); }
            .serif { font-family: var(--serif); }
            .body { font-family: var(--body); }
            .heading { font-family: var(--heading); }
            .body-font { font-family: var(--body-font); }
            .heading-font { font-family: var(--heading-font); }
            .primary { font-family: var(--font-primary); }
            .secondary { font-family: var(--font-secondary); }
        </style>"#;

        let fixed = fix_css_font_variables(html);
        assert!(!fixed.contains("var(--mono)"));
        assert!(!fixed.contains("var(--sans)"));
        assert!(!fixed.contains("var(--serif)"));
        assert!(!fixed.contains("var(--body)"));
        assert!(!fixed.contains("var(--heading)"));
        assert!(!fixed.contains("var(--body-font)"));
        assert!(!fixed.contains("var(--heading-font)"));
        assert!(!fixed.contains("var(--font-primary)"));
        assert!(!fixed.contains("var(--font-secondary)"));

        assert!(fixed.contains("'JetBrains Mono', monospace"));
        assert!(fixed.contains("'DM Sans', sans-serif"));
        assert!(fixed.contains("'Libre Baskerville', serif"));
        assert!(fixed.contains("'Space Grotesk', sans-serif"));
    }

    #[test]
    fn test_sanitize_unsupported_fonts() {
        let html = r#"<style>
            body { font-family: Noto Serif CJK SC, serif; }
            .title { font-family: PingFang SC, sans-serif; }
        </style>"#;

        let fixed = sanitize_unsupported_fonts(html);
        assert!(!fixed.contains("Noto Serif"));
        assert!(!fixed.contains("PingFang"));
        assert!(fixed.contains("font-family: serif"));
        assert!(fixed.contains("font-family: sans-serif"));
    }

    #[test]
    fn test_sanitize_preserves_generic_only() {
        let html = r#"<style>
            body { font-family: sans-serif; }
            code { font-family: monospace; }
        </style>"#;

        let fixed = sanitize_unsupported_fonts(html);
        assert!(fixed.contains("font-family: sans-serif"));
        assert!(fixed.contains("font-family: monospace"));
    }

    #[test]
    fn test_sanitize_removes_fontface() {
        let html = r#"<style>@font-face { font-family: "Custom"; src: url("x.woff2"); }
        body { font-family: "Custom", sans-serif; }</style>"#;

        let fixed = sanitize_unsupported_fonts(html);
        assert!(!fixed.contains("@font-face"));
    }

    #[test]
    fn test_ensure_hyperframes_interfaces_missing_both() {
        let html = r#"<!DOCTYPE html>
<html>
<head><title>Test</title></head>
<body>
<div class="clip">Content</div>
</body>
</html>"#;

        let fixed = ensure_hyperframes_interfaces(html, 10.5);
        assert!(fixed.contains("window.__timelines"));
        assert!(fixed.contains("window.__hf"));
        assert!(fixed.contains("window.__hf.duration = 10.50"));
        assert!(fixed.contains("seek"));
    }

    #[test]
    fn test_ensure_hyperframes_interfaces_already_present() {
        let html = r#"<!DOCTYPE html>
<html>
<head><title>Test</title></head>
<body>
<div class="clip">Content</div>
<script>
window.__timelines = window.__timelines || {};
window.__hf = { duration: 10, seek: function() {} };
</script>
</body>
</html>"#;

        let fixed = ensure_hyperframes_interfaces(html, 10.5);
        assert!(
            fixed.contains("window.__hf.duration = 10.50"),
            "Should always force correct duration"
        );
        assert!(
            fixed.contains("if (!window.__timelines['ai-generated'])"),
            "Safety-net should check timeline before creating"
        );
    }

    #[test]
    fn test_ensure_root_duration_adds_missing() {
        let html = r#"<html data-composition-id="test" data-width="1920">
            <body><div class="clip" data-start="0" data-duration="3">Hi</div></body>
        </html>"#;
        let result = ensure_root_duration(html, 8.5);
        assert!(result.contains(r#"data-duration="8.500""#), "Got: {result}");
    }

    #[test]
    fn test_ensure_root_duration_updates_wrong_value() {
        let html = r#"<html data-composition-id="test" data-duration="100" data-width="1920">
            <body><div class="clip" data-start="0" data-duration="3">Hi</div></body>
        </html>"#;
        let result = ensure_root_duration(html, 8.5);
        assert!(result.contains(r#"data-duration="8.500""#), "Got: {result}");
        assert!(
            !result.contains(r#"data-duration="100""#),
            "Should have replaced 100"
        );
    }

    #[test]
    fn test_ensure_root_duration_preserves_correct_value() {
        let html = r#"<html data-composition-id="test" data-duration="8.5" data-width="1920">
            <body><div class="clip" data-start="0" data-duration="3">Hi</div></body>
        </html>"#;
        let result = ensure_root_duration(html, 8.5);
        assert!(result.contains(r#"data-duration="8.5""#), "Got: {result}");
    }

    #[test]
    fn test_ensure_root_duration_no_composition_id() {
        let html = r#"<html data-width="1920"><body>No composition id</body></html>"#;
        let result = ensure_root_duration(html, 8.5);
        assert_eq!(result, html);
    }

    #[test]
    fn test_ensure_root_duration_single_quotes() {
        let html = r#"<html data-composition-id='test' data-duration='100'>
            <body>Content</body>
        </html>"#;
        let result = ensure_root_duration(html, 8.5);
        assert!(result.contains(r#"data-duration="8.500""#), "Got: {result}");
    }

    #[test]
    fn test_ensure_root_duration_ignores_css_selector() {
        let html = r#"<!DOCTYPE html><html>
<head><style>[data-composition-id="ai-generated"] { background: black; }</style></head>
<body>
<div data-composition-id="ai-generated" data-width="1920" data-height="1080" data-duration="480">
<div class="clip" data-start="0" data-duration="3">Hi</div>
</div>
</body></html>"#;
        let result = ensure_root_duration(html, 116.273);
        assert!(
            result.contains(r#"data-duration="116.273""#),
            "Got: {result}"
        );
        assert!(
            !result.contains(r#"data-duration="480""#),
            "Should have replaced 480"
        );
        assert!(
            result.contains(r#"[data-composition-id="ai-generated"]"#),
            "CSS selector should be intact"
        );
    }

    #[test]
    fn test_clamp_overflow_clips() {
        let html = r#"<html><body>
<div class="clip" data-start="0" data-duration="50">A</div>
<div class="clip" data-start="50" data-duration="200">B overflows</div>
</body></html>"#;
        let result = clamp_overflow_clips(html, 100.0);
        assert!(
            result.contains(r#"data-duration="50.000""#),
            "Got: {result}"
        );
        assert!(
            result.contains(r#"data-start="0" data-duration="50""#),
            "Got: {result}"
        );
    }
}
