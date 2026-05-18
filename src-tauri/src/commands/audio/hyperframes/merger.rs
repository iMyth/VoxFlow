//! Deterministic Merger module for the LLM orchestration pipeline.
//!
//! Combines Worker HTML outputs into a single valid Composition using pure Rust
//! string/regex operations. No LLM calls are made in this module.

use regex::Regex;

use super::pipeline_types::{ClipElement, MergerError, OrchestrationPlan, ParsedChunk};
use super::validation::validate_composition;

/// Parse a Worker's HTML output into structured components.
///
/// Extracts:
/// - CSS content (from `<style>` tags)
/// - Clip elements (with `data-start`, `data-duration`, `data-track-index`)
/// - GSAP timeline code (from `<script>` tags)
///
/// Returns `MergerError::ParseFailed` if the HTML cannot be parsed.
pub fn parse_worker_html(html: &str, chunk_index: usize) -> Result<ParsedChunk, MergerError> {
    // Extract CSS
    let css = extract_style_content(html).unwrap_or_default();
    let namespaced_css = namespace_css(&css, chunk_index);

    // Extract clip elements
    let mut clips = extract_clips(html, chunk_index)?;

    // Namespace HTML class references inside clip content
    for clip in &mut clips {
        if !clip.html.is_empty() {
            clip.html = namespace_html(&clip.html, chunk_index);
        }
    }

    // Extract GSAP timeline code
    let gsap_code = extract_gsap_code(html);
    let namespaced_gsap = namespace_gsap(&gsap_code, chunk_index);

    Ok(ParsedChunk {
        chunk_index,
        css: namespaced_css,
        clips,
        gsap_code: namespaced_gsap,
    })
}

/// Apply namespace prefix to CSS class names.
///
/// Prefixes all class selectors with `_c{chunk_index}_` to prevent collisions.
/// E.g., `.star` in chunk 0 becomes `._c0_star`
///
/// Skips class-like patterns inside url(), @import, and content strings.
pub fn namespace_css(css: &str, chunk_index: usize) -> String {
    let prefix = format!("_c{}_", chunk_index);
    // Pre-compile regexes once
    let class_re = Regex::new(r"\.([a-zA-Z_][a-zA-Z0-9_-]*)").unwrap();
    let id_re = Regex::new(r"#([a-zA-Z_][a-zA-Z0-9_-]*)").unwrap();

    // Process line by line to skip @import and url() lines
    css.lines()
        .map(|line| {
            let trimmed = line.trim();
            // Skip @import lines entirely (they contain URLs with dots)
            if trimmed.starts_with("@import") {
                return line.to_string();
            }
            // Skip lines containing url() to avoid breaking URLs
            if line.contains("url(") {
                return line.to_string();
            }
            // Skip lines that are inside filter definitions (contain url(#...))
            if line.contains("url(#") {
                return line.to_string();
            }

            // Namespace class selectors
            let result = class_re
                .replace_all(line, |caps: &regex::Captures| {
                    let class_name = &caps[1];
                    if is_reserved_class(class_name) {
                        format!(".{}", class_name)
                    } else {
                        format!(".{}{}", prefix, class_name)
                    }
                })
                .to_string();

            // Namespace id selectors
            id_re
                .replace_all(&result, |caps: &regex::Captures| {
                    let id_name = &caps[1];
                    format!("#{}{}", prefix, id_name)
                })
                .to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Apply namespace prefix to HTML class and id references.
///
/// Updates `class="..."` and `id="..."` attributes to prevent collisions between chunks.
pub fn namespace_html(html: &str, chunk_index: usize) -> String {
    let prefix = format!("_c{}_", chunk_index);
    let class_attr_re = Regex::new(r#"class="([^"]*)""#).unwrap();
    let id_attr_re = Regex::new(r#"id="([^"]*)""#).unwrap();

    // First namespace classes
    let result = class_attr_re
        .replace_all(html, |caps: &regex::Captures| {
            let classes = &caps[1];
            let namespaced_classes: Vec<String> = classes
                .split_whitespace()
                .map(|cls| {
                    if is_reserved_class(cls) {
                        cls.to_string()
                    } else {
                        format!("{}{}", prefix, cls)
                    }
                })
                .collect();
            format!("class=\"{}\"", namespaced_classes.join(" "))
        })
        .to_string();

    // Then namespace ids
    id_attr_re
        .replace_all(&result, |caps: &regex::Captures| {
            let id = &caps[1];
            format!("id=\"{}{}\"", prefix, id)
        })
        .to_string()
}

/// Namespace GSAP code selectors to match namespaced CSS classes.
///
/// Handles multiple patterns:
/// - Simple: ".classname"
/// - Multi-selector: ".a, .b, .c" or ".a,.b,.c"
/// - Compound: ".parent .child"
/// - With pseudo-classes: ".el:nth-child(2)"
///
/// Skips strings that are clearly not CSS selectors (URLs, CSS values, SVG paths, etc.)
fn namespace_gsap(code: &str, chunk_index: usize) -> String {
    let prefix = format!("_c{}_", chunk_index);

    // Only match CSS class selectors that appear as GSAP targets:
    // Patterns like tl.to('.class', ...) or tl.from('#id .class', ...)
    // Match: quote + optional #id + space + dot + classname, or quote + dot + classname
    // This is much safer than trying to match all quoted strings.
    let selector_re = Regex::new(r#"(['"])([^'"]*\.[a-zA-Z_][a-zA-Z0-9_-]*[^'"]*)(['"])"#).unwrap();
    let class_in_selector_re = Regex::new(r"\.([a-zA-Z_][a-zA-Z0-9_-]*)").unwrap();

    selector_re
        .replace_all(code, |caps: &regex::Captures| {
            let quote_start = &caps[1];
            let content = &caps[2];
            let quote_end = &caps[3];

            // Quotes must match (both single or both double)
            if quote_start != quote_end {
                return caps[0].to_string();
            }

            // Skip strings that are clearly not CSS selectors
            if content.contains("px")
                || content.contains("deg")
                || content.contains("rgb")
                || content.contains("hsl")
                || content.contains("blur(")
                || content.contains("rotate(")
                || content.contains("polygon(")
                || content.contains("gradient")
                || content.contains("inOut")
                || content.contains("ease")
                || content.contains("http")
                || content.contains("data:")
                || content.starts_with('+')
                || content.starts_with('-')
                || content.starts_with('=')
                || content.contains("none")
                || content.contains("auto")
                || content.contains("hidden")
                || content.contains("visible")
                || content.contains("block")
                || content.contains("flex")
                || content.contains("absolute")
                || content.contains("relative")
                || content.contains("center")
                || content.contains("left")
                || content.contains("right")
                || content.contains("top")
                || content.contains("bottom")
                || content.contains("solid")
                || content.contains("screen")
                || content.contains("overlay")
                || content.contains("multiply")
            {
                return caps[0].to_string();
            }

            // Must start with a selector-like pattern: . or # or element name
            let trimmed = content.trim();
            if !trimmed.starts_with('.') && !trimmed.starts_with('#') {
                return caps[0].to_string();
            }

            // Namespace all class references and id references
            let namespaced_content = class_in_selector_re
                .replace_all(content, |inner_caps: &regex::Captures| {
                    let class_name = &inner_caps[1];
                    if is_reserved_class(class_name) {
                        format!(".{}", class_name)
                    } else {
                        format!(".{}{}", prefix, class_name)
                    }
                })
                .to_string();

            // Also namespace #id references
            let id_in_selector_re = Regex::new(r"#([a-zA-Z_][a-zA-Z0-9_-]*)").unwrap();
            let namespaced_content = id_in_selector_re
                .replace_all(&namespaced_content, |inner_caps: &regex::Captures| {
                    let id_name = &inner_caps[1];
                    format!("#{}{}", prefix, id_name)
                })
                .to_string();

            format!("{}{}{}", quote_start, namespaced_content, quote_end)
        })
        .to_string()
}

/// Check if a class name is reserved and should not be namespaced.
fn is_reserved_class(name: &str) -> bool {
    matches!(name, "clip" | "layer" | "clip-path" | "mix-blend-mode")
}

/// Extract content from `<style>` tags.
fn extract_style_content(html: &str) -> Option<String> {
    let style_re = Regex::new(r"(?s)<style[^>]*>(.*?)</style>").unwrap();
    let mut all_styles = String::new();

    for cap in style_re.captures_iter(html) {
        if !all_styles.is_empty() {
            all_styles.push('\n');
        }
        all_styles.push_str(&cap[1]);
    }

    if all_styles.is_empty() {
        None
    } else {
        Some(all_styles)
    }
}

/// Extract clip elements from the HTML.
///
/// Uses a two-pass approach:
/// 1. Find the composition body (between root element and first script tag)
/// 2. Extract clip metadata from opening tags
/// 3. Use the full body content as clip HTML (preserving nested structure)
fn extract_clips(html: &str, chunk_index: usize) -> Result<Vec<ClipElement>, MergerError> {
    // First, find the composition body content
    let body_content = extract_composition_body_content(html);

    if body_content.is_empty() {
        return Err(MergerError::ParseFailed {
            chunk_index,
            reason: "Could not find composition body content".to_string(),
        });
    }

    // Extract clip metadata using lenient approach (just the attributes)
    let clips = extract_clips_from_body(&body_content, chunk_index);

    if clips.is_empty() {
        return Err(MergerError::ParseFailed {
            chunk_index,
            reason: "No clip elements found in HTML".to_string(),
        });
    }

    Ok(clips)
}

/// Extract the body content between composition root and first script tag.
fn extract_composition_body_content(html: &str) -> String {
    // Find the composition root opening tag end
    let comp_start = match html.find("data-composition-id=") {
        Some(pos) => pos,
        None => return String::new(),
    };
    let after_comp = match html[comp_start..].find('>') {
        Some(pos) => pos,
        None => return String::new(),
    };
    let body_start = comp_start + after_comp + 1;

    // Find the first <script tag
    let script_pos = match html[body_start..].find("<script") {
        Some(pos) => pos,
        None => return String::new(),
    };

    html[body_start..body_start + script_pos].to_string()
}

/// Extract clips from the body content, splitting by clip div boundaries.
fn extract_clips_from_body(body: &str, _chunk_index: usize) -> Vec<ClipElement> {
    let mut clips = Vec::new();

    // Find each top-level clip div by looking for opening tags with data-start
    // We need to handle nested divs properly by counting open/close tags
    let tag_re = Regex::new(r#"<div[^>]*data-start="([^"]*)"[^>]*>"#).unwrap();
    let duration_re = Regex::new(r#"data-duration="([^"]*)""#).unwrap();
    let track_re = Regex::new(r#"data-track-index="([^"]*)""#).unwrap();

    for cap in tag_re.captures_iter(body) {
        let full_match = cap.get(0).unwrap();
        let tag_str = full_match.as_str();
        let start_pos = full_match.start();

        // Skip the composition root element itself
        if tag_str.contains("data-composition-id") {
            continue;
        }

        let start_str = &cap[1];
        let data_start = match start_str.parse::<f64>() {
            Ok(v) => v,
            Err(_) => continue,
        };

        let data_duration = duration_re
            .captures(tag_str)
            .and_then(|c| c[1].parse::<f64>().ok())
            .unwrap_or(0.0);

        let data_track_index = track_re
            .captures(tag_str)
            .and_then(|c| c[1].parse::<u32>().ok())
            .unwrap_or(1);

        // Find the matching closing </div> by counting nesting
        let after_tag = full_match.end();
        let inner_html = find_matching_close_div(&body[after_tag..]);

        clips.push(ClipElement {
            html: inner_html,
            data_start,
            data_duration,
            data_track_index,
        });

        // Skip past this clip for the next iteration
        let _ = start_pos; // tag_re.captures_iter handles iteration
    }

    clips
}

/// Find content up to the matching </div>, handling nested divs.
/// Properly handles UTF-8 multi-byte characters by only checking at '<' boundaries.
fn find_matching_close_div(content: &str) -> String {
    let mut depth = 1;
    let chars = content.char_indices().peekable();

    for (pos, ch) in chars {
        if ch == '<' {
            // Check if this is <div or </div>
            let remaining = &content[pos..];
            if remaining.starts_with("<div") {
                let after_div = remaining.as_bytes().get(4).copied().unwrap_or(b'>');
                if after_div == b' ' || after_div == b'>' || after_div == b'/' {
                    depth += 1;
                }
            } else if remaining.starts_with("</div>") {
                depth -= 1;
                if depth == 0 {
                    return content[..pos].to_string();
                }
            }
        }
    }

    // If we didn't find matching close, return everything
    content.to_string()
}

/// Extract GSAP timeline code from script tags.
///
/// Removes boilerplate (window.__timelines init, const tl = ..., registration)
/// and keeps only the animation calls.
/// Also renames local variable declarations to avoid conflicts between chunks.
fn extract_gsap_code(html: &str) -> String {
    // Find script blocks (not the CDN script tag)
    let script_re = Regex::new(r"(?s)<script>(.+?)</script>").unwrap();

    let mut timeline_code = String::new();

    for cap in script_re.captures_iter(html) {
        let code = &cap[1];
        // Skip if this is just a CDN import
        if code.contains("cdn.jsdelivr.net") {
            continue;
        }

        // Extract all lines, filtering boilerplate
        for line in code.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty()
                || trimmed.starts_with("window.__timelines")
                || trimmed.starts_with("const tl")
                || trimmed.starts_with("let tl")
                || trimmed.starts_with("var tl")
                || trimmed.contains("window.__timelines[")
            {
                continue;
            }
            if !timeline_code.is_empty() {
                timeline_code.push('\n');
            }
            timeline_code.push_str(trimmed);
        }
    }

    timeline_code
}

/// Resolve track-index conflicts between chunks.
///
/// Offsets each chunk's track indices to prevent overlap on the same track.
/// Strategy: offset each chunk's track indices by the max used in previous chunks.
pub fn resolve_track_indices(chunks: &mut [ParsedChunk]) {
    let mut offset = 0u32;
    for chunk in chunks.iter_mut() {
        let max_in_chunk = chunk
            .clips
            .iter()
            .map(|c| c.data_track_index)
            .max()
            .unwrap_or(0);

        // Offset all track indices in this chunk
        for clip in &mut chunk.clips {
            clip.data_track_index += offset;
        }
        offset += max_in_chunk;
    }
}

/// Sort clips by ascending `data-start` with chunk index as tiebreaker.
pub fn sort_clips_by_start(clips: &mut [(usize, &ClipElement)]) {
    clips.sort_by(|a, b| {
        a.1.data_start
            .partial_cmp(&b.1.data_start)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.0.cmp(&b.0))
    });
}

/// Merge multiple parsed chunks into a final Composition HTML.
///
/// Combines:
/// - Namespaced CSS from all chunks (deduplicating @font-face and CDN scripts)
/// - Clip elements ordered by ascending data-start
/// - GSAP timeline code merged into a single timeline instance
///
/// Validates the final output with `validate_composition()`.
pub fn merge_chunks(
    chunks: &[ParsedChunk],
    total_duration: f64,
    plan: &OrchestrationPlan,
) -> Result<String, MergerError> {
    if chunks.is_empty() {
        return Err(MergerError::ValidationFailed(vec![
            "No chunks to merge".to_string()
        ]));
    }

    // Verify transitions (log warnings but don't fail)
    if let Err(warnings) = verify_transitions(plan) {
        log::warn!("Transition mismatches detected: {:?}", warnings);
    }

    // Collect and deduplicate CSS
    let mut all_css = String::new();
    let mut seen_font_faces: Vec<String> = Vec::new();

    for chunk in chunks {
        // Deduplicate @font-face rules
        let css = deduplicate_font_faces(&chunk.css, &mut seen_font_faces);
        if !all_css.is_empty() {
            all_css.push('\n');
        }
        all_css.push_str(&format!("    /* chunk-{} */\n", chunk.chunk_index));
        all_css.push_str(&css);
    }

    // Collect all clips, sorted by data-start with chunk_index as tiebreaker
    let mut all_clips_indexed: Vec<(usize, &ClipElement)> = Vec::new();
    for chunk in chunks {
        for clip in &chunk.clips {
            all_clips_indexed.push((chunk.chunk_index, clip));
        }
    }
    sort_clips_by_start(&mut all_clips_indexed);

    // Build clip HTML elements
    let clips_html: String = all_clips_indexed
        .iter()
        .map(|(chunk_idx, clip)| {
            format!(
                "    <div class=\"clip layer\" data-start=\"{}\" data-duration=\"{}\" data-track-index=\"{}\">\n      {}\n    </div>",
                clip.data_start,
                clip.data_duration,
                clip.data_track_index,
                if clip.html.is_empty() {
                    format!("<!-- chunk {} clip -->", chunk_idx)
                } else {
                    clip.html.clone()
                }
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    // Collect GSAP code from all chunks, each wrapped in a block scope to avoid variable conflicts
    let all_gsap: String = chunks
        .iter()
        .filter(|c| !c.gsap_code.is_empty())
        .map(|c| {
            format!(
                "      // chunk-{}\n      {{\n      {}\n      }}",
                c.chunk_index, c.gsap_code
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    // Assemble final HTML
    let merged_html = format!(
        r#"<!DOCTYPE html>
<html>
<head>
  <meta charset="UTF-8">
  <style>
    [data-composition-id] {{ overflow: hidden; position: relative; }}
    .layer {{ position: absolute; width: 100%; height: 100%; }}
{css}
  </style>
</head>
<body>
  <div data-composition-id="ai-generated" data-width="1920" data-height="1080" data-start="0" data-duration="{duration}">
{clips}
    <script src="https://cdn.jsdelivr.net/npm/gsap@3.12.5/dist/gsap.min.js"></script>
    <script>
      window.__timelines = window.__timelines || {{}};
      const tl = gsap.timeline({{ paused: true }});
{gsap}
      window.__timelines["ai-generated"] = tl;
    </script>
  </div>
</body>
</html>"#,
        css = all_css,
        duration = total_duration,
        clips = clips_html,
        gsap = all_gsap,
    );

    // Validate final output
    if let Err(errors) = validate_composition(&merged_html) {
        return Err(MergerError::ValidationFailed(errors));
    }

    Ok(merged_html)
}

/// Deduplicate @font-face rules from CSS content.
///
/// Tracks seen font-face declarations and removes duplicates.
fn deduplicate_font_faces(css: &str, seen: &mut Vec<String>) -> String {
    let font_face_re = Regex::new(r"(?s)@font-face\s*\{[^}]*\}").unwrap();
    let mut result = css.to_string();

    for cap in font_face_re.find_iter(css) {
        let font_face = cap.as_str().to_string();
        if seen.contains(&font_face) {
            // Remove duplicate
            result = result.replace(&font_face, "");
        } else {
            seen.push(font_face);
        }
    }

    result
}

/// Verify that adjacent chunks have compatible transition specifications.
///
/// Checks that the transition-out type of chunk N matches the transition-in type of chunk N+1.
pub fn verify_transitions(plan: &OrchestrationPlan) -> Result<(), Vec<String>> {
    let mut mismatches = Vec::new();

    for i in 0..plan.chunks.len().saturating_sub(1) {
        let out_type = &plan.chunks[i].transition_out.transition_type;
        let in_type = &plan.chunks[i + 1].transition_in.transition_type;

        if out_type != in_type {
            mismatches.push(format!(
                "Chunk {} transition_out '{}' does not match chunk {} transition_in '{}'",
                i,
                out_type,
                i + 1,
                in_type
            ));
        }
    }

    if mismatches.is_empty() {
        Ok(())
    } else {
        Err(mismatches)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_namespace_css_prefixes_classes() {
        let css = ".star { opacity: 0.5; }\n.nebula { background: blue; }";
        let result = namespace_css(css, 0);
        assert!(result.contains("._c0_star"));
        assert!(result.contains("._c0_nebula"));
        assert!(!result.contains(".star"));
        assert!(!result.contains(".nebula"));
    }

    #[test]
    fn test_namespace_css_different_chunks() {
        let css = ".element { color: red; }";
        let result0 = namespace_css(css, 0);
        let result1 = namespace_css(css, 1);
        assert!(result0.contains("._c0_element"));
        assert!(result1.contains("._c1_element"));
    }

    #[test]
    fn test_namespace_css_preserves_reserved_classes() {
        let css = ".clip { display: block; }\n.layer { position: absolute; }";
        let result = namespace_css(css, 2);
        assert!(result.contains(".clip"));
        assert!(result.contains(".layer"));
        assert!(!result.contains("._c2_clip"));
        assert!(!result.contains("._c2_layer"));
    }

    #[test]
    fn test_namespace_html_updates_class_references() {
        let html = r#"<div class="star bright">content</div>"#;
        let result = namespace_html(html, 1);
        assert!(result.contains("_c1_star"));
        assert!(result.contains("_c1_bright"));
    }

    #[test]
    fn test_namespace_html_preserves_reserved_classes() {
        let html = r#"<div class="clip layer custom">content</div>"#;
        let result = namespace_html(html, 0);
        assert!(result.contains("clip"));
        assert!(result.contains("layer"));
        assert!(result.contains("_c0_custom"));
    }

    #[test]
    fn test_namespace_isolation_between_chunks() {
        let css = ".element { color: red; }";
        let result0 = namespace_css(css, 0);
        let result1 = namespace_css(css, 1);

        // Extract class names from each result
        let class_re = Regex::new(r"\.([a-zA-Z_][a-zA-Z0-9_-]*)").unwrap();
        let classes0: Vec<String> = class_re
            .captures_iter(&result0)
            .map(|c| c[1].to_string())
            .collect();
        let classes1: Vec<String> = class_re
            .captures_iter(&result1)
            .map(|c| c[1].to_string())
            .collect();

        // No class from chunk 0 should appear in chunk 1
        for cls in &classes0 {
            assert!(
                !classes1.contains(cls),
                "Class '{}' from chunk 0 collides with chunk 1",
                cls
            );
        }
    }

    #[test]
    fn test_extract_style_content() {
        let html = r#"<html><head><style>.x { color: red; }</style></head><body></body></html>"#;
        let result = extract_style_content(html);
        assert!(result.is_some());
        assert!(result.unwrap().contains(".x { color: red; }"));
    }

    #[test]
    fn test_extract_gsap_code_removes_boilerplate() {
        let html = r#"<script>
window.__timelines = window.__timelines || {};
const tl = gsap.timeline({ paused: true });
tl.from(".star", { opacity: 0, duration: 1 }, 0);
tl.to(".bg", { scale: 1.1, duration: 3 }, 2);
window.__timelines["ai-generated"] = tl;
</script>"#;
        let result = extract_gsap_code(html);
        assert!(result.contains("tl.from"));
        assert!(result.contains("tl.to"));
        assert!(!result.contains("window.__timelines"));
        assert!(!result.contains("const tl"));
    }

    #[test]
    fn test_verify_transitions_compatible() {
        use super::super::pipeline_types::*;

        let plan = OrchestrationPlan {
            global_theme: GlobalTheme {
                mood: vec!["epic".to_string()],
                shared_motifs: vec![],
                color_progression: ColorProgression {
                    start_palette: vec![],
                    end_palette: vec![],
                },
            },
            chunks: vec![
                ChunkPlan {
                    index: 0,
                    entry_start: 0,
                    entry_end: 5,
                    visual_directive: VisualDirective {
                        palette: vec!["#aaa".to_string(), "#bbb".to_string(), "#ccc".to_string()],
                        style_keywords: vec![],
                        rhythm: "moderate".to_string(),
                        concept: "test".to_string(),
                    },
                    transition_in: TransitionSpec {
                        transition_type: "fade".to_string(),
                        colors: vec![],
                    },
                    transition_out: TransitionSpec {
                        transition_type: "dissolve".to_string(),
                        colors: vec![],
                    },
                },
                ChunkPlan {
                    index: 1,
                    entry_start: 5,
                    entry_end: 10,
                    visual_directive: VisualDirective {
                        palette: vec!["#aaa".to_string(), "#bbb".to_string(), "#ddd".to_string()],
                        style_keywords: vec![],
                        rhythm: "fast".to_string(),
                        concept: "test2".to_string(),
                    },
                    transition_in: TransitionSpec {
                        transition_type: "dissolve".to_string(),
                        colors: vec![],
                    },
                    transition_out: TransitionSpec {
                        transition_type: "fade".to_string(),
                        colors: vec![],
                    },
                },
            ],
        };

        assert!(verify_transitions(&plan).is_ok());
    }

    #[test]
    fn test_verify_transitions_incompatible() {
        use super::super::pipeline_types::*;

        let plan = OrchestrationPlan {
            global_theme: GlobalTheme {
                mood: vec![],
                shared_motifs: vec![],
                color_progression: ColorProgression {
                    start_palette: vec![],
                    end_palette: vec![],
                },
            },
            chunks: vec![
                ChunkPlan {
                    index: 0,
                    entry_start: 0,
                    entry_end: 5,
                    visual_directive: VisualDirective {
                        palette: vec!["#a".to_string(), "#b".to_string(), "#c".to_string()],
                        style_keywords: vec![],
                        rhythm: "slow".to_string(),
                        concept: "".to_string(),
                    },
                    transition_in: TransitionSpec {
                        transition_type: "fade".to_string(),
                        colors: vec![],
                    },
                    transition_out: TransitionSpec {
                        transition_type: "wipe-left".to_string(),
                        colors: vec![],
                    },
                },
                ChunkPlan {
                    index: 1,
                    entry_start: 5,
                    entry_end: 10,
                    visual_directive: VisualDirective {
                        palette: vec!["#a".to_string(), "#b".to_string(), "#d".to_string()],
                        style_keywords: vec![],
                        rhythm: "fast".to_string(),
                        concept: "".to_string(),
                    },
                    transition_in: TransitionSpec {
                        transition_type: "dissolve".to_string(),
                        colors: vec![],
                    },
                    transition_out: TransitionSpec {
                        transition_type: "fade".to_string(),
                        colors: vec![],
                    },
                },
            ],
        };

        let result = verify_transitions(&plan);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors[0].contains("wipe-left"));
        assert!(errors[0].contains("dissolve"));
    }

    #[test]
    fn test_parse_worker_html_returns_error_for_unparseable() {
        let html = "<html><body>No clips here</body></html>";
        let result = parse_worker_html(html, 0);
        assert!(result.is_err());
        match result.unwrap_err() {
            MergerError::ParseFailed { chunk_index, .. } => {
                assert_eq!(chunk_index, 0);
            }
            _ => panic!("Expected ParseFailed"),
        }
    }

    #[test]
    fn test_find_matching_close_div_with_chinese() {
        // Regression test: UTF-8 multi-byte characters must not cause panics
        let content = r#"<div id="inner">看！在那里！那颗发光的果子！</div></div>"#;
        let result = find_matching_close_div(content);
        // depth starts at 1, <div> makes it 2, first </div> makes it 1, second </div> makes it 0
        assert!(result.contains("看！在那里"));
        assert!(result.contains("那颗发光的果子"));
    }

    #[test]
    fn test_resolve_track_indices_no_overlap() {
        let mut chunks = vec![
            ParsedChunk {
                chunk_index: 0,
                css: String::new(),
                clips: vec![
                    ClipElement {
                        html: String::new(),
                        data_start: 0.0,
                        data_duration: 5.0,
                        data_track_index: 1,
                    },
                    ClipElement {
                        html: String::new(),
                        data_start: 0.0,
                        data_duration: 5.0,
                        data_track_index: 2,
                    },
                ],
                gsap_code: String::new(),
            },
            ParsedChunk {
                chunk_index: 1,
                css: String::new(),
                clips: vec![
                    ClipElement {
                        html: String::new(),
                        data_start: 5.0,
                        data_duration: 5.0,
                        data_track_index: 1,
                    },
                    ClipElement {
                        html: String::new(),
                        data_start: 5.0,
                        data_duration: 5.0,
                        data_track_index: 2,
                    },
                ],
                gsap_code: String::new(),
            },
        ];

        resolve_track_indices(&mut chunks);

        // Chunk 0 tracks should remain 1, 2
        assert_eq!(chunks[0].clips[0].data_track_index, 1);
        assert_eq!(chunks[0].clips[1].data_track_index, 2);

        // Chunk 1 tracks should be offset by max of chunk 0 (2)
        assert_eq!(chunks[1].clips[0].data_track_index, 3);
        assert_eq!(chunks[1].clips[1].data_track_index, 4);
    }

    #[test]
    fn test_resolve_track_indices_single_chunk() {
        let mut chunks = vec![ParsedChunk {
            chunk_index: 0,
            css: String::new(),
            clips: vec![ClipElement {
                html: String::new(),
                data_start: 0.0,
                data_duration: 10.0,
                data_track_index: 1,
            }],
            gsap_code: String::new(),
        }];

        resolve_track_indices(&mut chunks);
        assert_eq!(chunks[0].clips[0].data_track_index, 1);
    }

    #[test]
    fn test_sort_clips_by_start() {
        let clips = vec![
            ClipElement {
                html: "c".to_string(),
                data_start: 10.0,
                data_duration: 5.0,
                data_track_index: 1,
            },
            ClipElement {
                html: "a".to_string(),
                data_start: 0.0,
                data_duration: 5.0,
                data_track_index: 1,
            },
            ClipElement {
                html: "b".to_string(),
                data_start: 5.0,
                data_duration: 5.0,
                data_track_index: 1,
            },
        ];

        let mut indexed: Vec<(usize, &ClipElement)> =
            clips.iter().enumerate().map(|(i, c)| (i, c)).collect();
        sort_clips_by_start(&mut indexed);

        assert_eq!(indexed[0].1.data_start, 0.0);
        assert_eq!(indexed[1].1.data_start, 5.0);
        assert_eq!(indexed[2].1.data_start, 10.0);
    }

    #[test]
    fn test_merge_chunks_two_valid_chunks() {
        use super::super::pipeline_types::*;

        let chunks = vec![
            ParsedChunk {
                chunk_index: 0,
                css: "._c0_star { opacity: 0.5; }".to_string(),
                clips: vec![ClipElement {
                    html: "<div class=\"_c0_star\"></div>".to_string(),
                    data_start: 0.0,
                    data_duration: 5.0,
                    data_track_index: 1,
                }],
                gsap_code: "tl.from(\"._c0_star\", { opacity: 0, duration: 1 }, 0);".to_string(),
            },
            ParsedChunk {
                chunk_index: 1,
                css: "._c1_nebula { background: blue; }".to_string(),
                clips: vec![ClipElement {
                    html: "<div class=\"_c1_nebula\"></div>".to_string(),
                    data_start: 5.0,
                    data_duration: 5.0,
                    data_track_index: 2,
                }],
                gsap_code: "tl.from(\"._c1_nebula\", { scale: 0, duration: 2 }, 5);".to_string(),
            },
        ];

        let plan = OrchestrationPlan {
            global_theme: GlobalTheme {
                mood: vec!["epic".to_string()],
                shared_motifs: vec![],
                color_progression: ColorProgression {
                    start_palette: vec![],
                    end_palette: vec![],
                },
            },
            chunks: vec![
                ChunkPlan {
                    index: 0,
                    entry_start: 0,
                    entry_end: 3,
                    visual_directive: VisualDirective {
                        palette: vec!["#a".to_string(), "#b".to_string(), "#c".to_string()],
                        style_keywords: vec![],
                        rhythm: "moderate".to_string(),
                        concept: "".to_string(),
                    },
                    transition_in: TransitionSpec {
                        transition_type: "fade".to_string(),
                        colors: vec![],
                    },
                    transition_out: TransitionSpec {
                        transition_type: "dissolve".to_string(),
                        colors: vec![],
                    },
                },
                ChunkPlan {
                    index: 1,
                    entry_start: 3,
                    entry_end: 6,
                    visual_directive: VisualDirective {
                        palette: vec!["#a".to_string(), "#b".to_string(), "#d".to_string()],
                        style_keywords: vec![],
                        rhythm: "fast".to_string(),
                        concept: "".to_string(),
                    },
                    transition_in: TransitionSpec {
                        transition_type: "dissolve".to_string(),
                        colors: vec![],
                    },
                    transition_out: TransitionSpec {
                        transition_type: "fade".to_string(),
                        colors: vec![],
                    },
                },
            ],
        };

        let result = merge_chunks(&chunks, 10.0, &plan);
        assert!(result.is_ok(), "merge_chunks failed: {:?}", result.err());

        let html = result.unwrap();
        assert!(html.contains("data-composition-id=\"ai-generated\""));
        assert!(html.contains("data-duration=\"10\""));
        assert!(html.contains("._c0_star"));
        assert!(html.contains("._c1_nebula"));
        assert!(html.contains("window.__timelines[\"ai-generated\"] = tl;"));
    }

    #[test]
    fn test_deduplicate_font_faces() {
        let css1 =
            "@font-face { font-family: 'Test'; src: url('test.woff2'); }\n.a { color: red; }";
        let css2 =
            "@font-face { font-family: 'Test'; src: url('test.woff2'); }\n.b { color: blue; }";

        let mut seen = Vec::new();
        let result1 = deduplicate_font_faces(css1, &mut seen);
        let result2 = deduplicate_font_faces(css2, &mut seen);

        // First occurrence should be kept
        assert!(result1.contains("@font-face"));
        // Second occurrence should be removed
        assert!(!result2.contains("@font-face"));
        // Non-font-face content should be preserved
        assert!(result1.contains(".a { color: red; }"));
        assert!(result2.contains(".b { color: blue; }"));
    }
}
