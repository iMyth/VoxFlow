//! Fixed template HTML generation for Hyperframes video export.
//!
//! Provides 3 preset visual templates that generate complete, self-contained
//! HTML compositions conforming to the Hyperframes spec:
//! - minimal-subtitle: Dark background + centered white text
//! - dialogue-cards: Colored dialogue bubbles per character
//! - chapter-sections: Segmented by section with title card transitions

use serde_json::json;

use super::timeline::TimelineEntry;

/// Generate a complete Hyperframes HTML composition using the specified template.
///
/// Returns an error if the template name is not recognized.
pub fn generate_html(template: &str, entries: &[TimelineEntry]) -> Result<String, String> {
    match template {
        "minimal-subtitle" => Ok(generate_minimal_subtitle(entries)),
        "dialogue-cards" => Ok(generate_dialogue_cards(entries)),
        "chapter-sections" => Ok(generate_chapter_sections(entries)),
        _ => Err(format!(
            "Unknown template: '{}'. Available: minimal-subtitle, dialogue-cards, chapter-sections",
            template
        )),
    }
}

/// Generate the Hyperframes project metadata JSON file content.
///
/// Returns a JSON string suitable for writing directly to `meta.json`.
///
/// # Arguments
/// * `composition_id` — Template/composition name (e.g. "minimal-subtitle"), matches `data-composition-id` in HTML
/// * `title` — Human-readable project title
/// * `total_duration` — Total composition duration in seconds (from timeline)
pub fn generate_meta_json(composition_id: &str, title: &str, total_duration: f64) -> String {
    let meta = json!({
        "id": composition_id,
        "title": title,
        "width": 1920,
        "height": 1080,
        "fps": 30,
        "duration": total_duration
    });
    serde_json::to_string_pretty(&meta).expect("Failed to serialize meta.json")
}

/// Compute total composition duration from timeline entries.
fn total_duration(entries: &[TimelineEntry]) -> f64 {
    entries
        .iter()
        .map(|e| e.start_time + e.duration)
        .fold(0.0_f64, f64::max)
}

/// Escape HTML special characters in text content.
fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

/// Format a time value to a clean string representation.
/// Avoids unnecessary decimal places for whole numbers.
fn format_time(t: f64) -> String {
    if (t - t.round()).abs() < 0.001 {
        format!("{:.0}", t)
    } else {
        let s = format!("{:.2}", t);
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

/// Build a GSAP `tl.from(...)` line.
fn tween_from(selector: &str, props: &str, time: f64) -> String {
    format!(
        "      tl.from(\"{}\", {{ {} }}, {});\n",
        selector,
        props,
        format_time(time)
    )
}

/// Build a GSAP `tl.to(...)` line.
fn tween_to(selector: &str, props: &str, time: f64) -> String {
    format!(
        "      tl.to(\"{}\", {{ {} }}, {});\n",
        selector,
        props,
        format_time(time)
    )
}

// ─────────────────────────────────────────────────────────────────────────────
// Template 1: minimal-subtitle
// ─────────────────────────────────────────────────────────────────────────────

/// Generate the "minimal-subtitle" template.
///
/// Dark background with centered white text. Each line fades in/out.
/// Suitable for narration/monologue content.
fn generate_minimal_subtitle(entries: &[TimelineEntry]) -> String {
    let duration = total_duration(entries);
    let mut clips = String::new();
    let mut tweens = String::new();

    for (i, entry) in entries.iter().enumerate() {
        let clip_id = format!("clip-{}", i);
        let text = escape_html(&entry.text);

        clips.push_str(&format!(
            "    <div id=\"{id}\" class=\"clip subtitle-line\" \
             data-start=\"{start}\" data-duration=\"{dur}\" data-track-index=\"1\">\n\
             \x20     <p class=\"line-text\">{text}</p>\n\
             \x20   </div>\n",
            id = clip_id,
            start = format_time(entry.start_time),
            dur = format_time(entry.duration),
            text = text,
        ));

        // Fade in from below
        let selector = format!("#{} .line-text", clip_id);
        tweens.push_str(&tween_from(
            &selector,
            "y: 30, opacity: 0, duration: 0.4, ease: \"power2.out\"",
            entry.start_time + 0.1,
        ));

        // Fade out upward (only if there's enough duration)
        if entry.duration > 1.0 {
            let exit_time = entry.start_time + entry.duration - 0.4;
            tweens.push_str(&tween_to(
                &selector,
                "y: -20, opacity: 0, duration: 0.3, ease: \"power2.in\"",
                exit_time,
            ));
        }
    }

    format!(
        "{doctype}\n\
         <html>\n\
         <head>\n\
         \x20 <meta charset=\"UTF-8\">\n\
         \x20 <style>\n\
         {style}\
         \x20 </style>\n\
         </head>\n\
         <body>\n\
         \x20 <div data-composition-id=\"minimal-subtitle\" data-width=\"1920\" data-height=\"1080\" \
         data-start=\"0\" data-duration=\"{duration}\">\n\
         {clips}\
         \x20   <script src=\"https://cdn.jsdelivr.net/npm/gsap@3.12.5/dist/gsap.min.js\"></script>\n\
         \x20   <script>\n\
         \x20     window.__timelines = window.__timelines || {{}};\n\
         \x20     const tl = gsap.timeline({{ paused: true }});\n\
         {tweens}\
         \x20     window.__timelines[\"minimal-subtitle\"] = tl;\n\
         \x20   </script>\n\
         \x20 </div>\n\
         </body>\n\
         </html>",
        doctype = "<!DOCTYPE html>",
        style = MINIMAL_SUBTITLE_STYLE,
        duration = format_time(duration),
        clips = clips,
        tweens = tweens,
    )
}

const MINIMAL_SUBTITLE_STYLE: &str = "\
    :root {\n\
      --bg-color: #0a0a0f;\n\
      --text-color: #f0f0f5;\n\
      --accent-color: #6366f1;\n\
      --font-family: \"Inter\", \"Noto Sans SC\", sans-serif;\n\
    }\n\
    [data-composition-id=\"minimal-subtitle\"] {\n\
      background: var(--bg-color);\n\
      display: flex;\n\
      align-items: center;\n\
      justify-content: center;\n\
      overflow: hidden;\n\
      font-family: var(--font-family);\n\
    }\n\
    .subtitle-line {\n\
      position: absolute;\n\
      width: 100%;\n\
      height: 100%;\n\
      display: flex;\n\
      align-items: center;\n\
      justify-content: center;\n\
      padding: 120px 200px;\n\
      box-sizing: border-box;\n\
    }\n\
    .line-text {\n\
      color: var(--text-color);\n\
      font-size: 64px;\n\
      font-weight: 500;\n\
      text-align: center;\n\
      line-height: 1.4;\n\
      max-width: 1400px;\n\
    }\n";

// ─────────────────────────────────────────────────────────────────────────────
// Template 2: dialogue-cards
// ─────────────────────────────────────────────────────────────────────────────

/// Assign a consistent color index to a character name.
/// Uses a simple hash to map character names to color indices.
fn character_color_index(name: &str, total_colors: usize) -> usize {
    let hash: u32 = name
        .bytes()
        .fold(0u32, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u32));
    (hash as usize) % total_colors
}

/// Generate the "dialogue-cards" template.
///
/// Different colored dialogue bubbles per character.
/// Suitable for multi-character dialogue content.
fn generate_dialogue_cards(entries: &[TimelineEntry]) -> String {
    let duration = total_duration(entries);
    let mut clips = String::new();
    let mut tweens = String::new();

    let colors = [
        "#6366f1", // indigo
        "#ec4899", // pink
        "#14b8a6", // teal
        "#f59e0b", // amber
        "#8b5cf6", // violet
        "#06b6d4", // cyan
    ];

    for (i, entry) in entries.iter().enumerate() {
        let clip_id = format!("clip-{}", i);
        let text = escape_html(&entry.text);
        let char_name = entry.character_name.as_deref().unwrap_or("旁白");
        let char_name_escaped = escape_html(char_name);
        let color_idx = character_color_index(char_name, colors.len());
        let color = colors[color_idx];

        // Alternate left/right alignment based on character
        let align_class = if color_idx % 2 == 0 {
            "align-left"
        } else {
            "align-right"
        };

        clips.push_str(&format!(
            "    <div id=\"{id}\" class=\"clip dialogue-card {align}\" \
             data-start=\"{start}\" data-duration=\"{dur}\" data-track-index=\"1\" \
             style=\"--card-color: {color}\">\n\
             \x20     <div class=\"card-bubble\">\n\
             \x20       <span class=\"char-name\">{name}</span>\n\
             \x20       <p class=\"card-text\">{text}</p>\n\
             \x20     </div>\n\
             \x20   </div>\n",
            id = clip_id,
            align = align_class,
            start = format_time(entry.start_time),
            dur = format_time(entry.duration),
            color = color,
            name = char_name_escaped,
            text = text,
        ));

        // Slide in from the side
        let x_from = if color_idx % 2 == 0 { -60 } else { 60 };
        let selector = format!("#{} .card-bubble", clip_id);
        tweens.push_str(&tween_from(
            &selector,
            &format!(
                "x: {}, opacity: 0, duration: 0.5, ease: \"power3.out\"",
                x_from
            ),
            entry.start_time + 0.1,
        ));

        // Fade out
        if entry.duration > 1.2 {
            let exit_time = entry.start_time + entry.duration - 0.4;
            tweens.push_str(&tween_to(
                &selector,
                "opacity: 0, duration: 0.3, ease: \"power2.in\"",
                exit_time,
            ));
        }
    }

    format!(
        "{doctype}\n\
         <html>\n\
         <head>\n\
         \x20 <meta charset=\"UTF-8\">\n\
         \x20 <style>\n\
         {style}\
         \x20 </style>\n\
         </head>\n\
         <body>\n\
         \x20 <div data-composition-id=\"dialogue-cards\" data-width=\"1920\" data-height=\"1080\" \
         data-start=\"0\" data-duration=\"{duration}\">\n\
         {clips}\
         \x20   <script src=\"https://cdn.jsdelivr.net/npm/gsap@3.12.5/dist/gsap.min.js\"></script>\n\
         \x20   <script>\n\
         \x20     window.__timelines = window.__timelines || {{}};\n\
         \x20     const tl = gsap.timeline({{ paused: true }});\n\
         {tweens}\
         \x20     window.__timelines[\"dialogue-cards\"] = tl;\n\
         \x20   </script>\n\
         \x20 </div>\n\
         </body>\n\
         </html>",
        doctype = "<!DOCTYPE html>",
        style = DIALOGUE_CARDS_STYLE,
        duration = format_time(duration),
        clips = clips,
        tweens = tweens,
    )
}

const DIALOGUE_CARDS_STYLE: &str = "\
    :root {\n\
      --bg-color: #111118;\n\
      --text-color: #f5f5f7;\n\
      --font-family: \"Inter\", \"Noto Sans SC\", sans-serif;\n\
    }\n\
    [data-composition-id=\"dialogue-cards\"] {\n\
      background: var(--bg-color);\n\
      overflow: hidden;\n\
      font-family: var(--font-family);\n\
      position: relative;\n\
    }\n\
    .dialogue-card {\n\
      position: absolute;\n\
      width: 100%;\n\
      height: 100%;\n\
      display: flex;\n\
      align-items: center;\n\
      padding: 100px 160px;\n\
      box-sizing: border-box;\n\
    }\n\
    .dialogue-card.align-left {\n\
      justify-content: flex-start;\n\
    }\n\
    .dialogue-card.align-right {\n\
      justify-content: flex-end;\n\
    }\n\
    .card-bubble {\n\
      background: color-mix(in srgb, var(--card-color) 15%, transparent);\n\
      border: 2px solid color-mix(in srgb, var(--card-color) 40%, transparent);\n\
      border-radius: 24px;\n\
      padding: 48px 64px;\n\
      max-width: 1100px;\n\
    }\n\
    .char-name {\n\
      display: block;\n\
      font-size: 28px;\n\
      font-weight: 700;\n\
      color: var(--card-color);\n\
      margin-bottom: 16px;\n\
      text-transform: uppercase;\n\
      letter-spacing: 0.05em;\n\
    }\n\
    .card-text {\n\
      color: var(--text-color);\n\
      font-size: 52px;\n\
      font-weight: 400;\n\
      line-height: 1.5;\n\
      margin: 0;\n\
    }\n";

// ─────────────────────────────────────────────────────────────────────────────
// Template 3: chapter-sections
// ─────────────────────────────────────────────────────────────────────────────

/// Generate the "chapter-sections" template.
///
/// Segmented by ScriptSection with title card transitions.
/// Suitable for long audiobooks with distinct chapters.
fn generate_chapter_sections(entries: &[TimelineEntry]) -> String {
    let duration = total_duration(entries);
    let mut clips = String::new();
    let mut tweens = String::new();
    let mut track_index = 1;

    // Group entries by section — detect section boundaries
    let mut current_section: Option<String> = None;
    let mut section_start_indices: Vec<(usize, String)> = Vec::new();

    for (i, entry) in entries.iter().enumerate() {
        let section = entry
            .section_title
            .clone()
            .unwrap_or_else(|| "正文".to_string());

        if current_section.as_deref() != Some(&section) {
            section_start_indices.push((i, section.clone()));
            current_section = Some(section);
        }
    }

    // Generate title cards for each section
    for (sec_idx, (entry_idx, section_title)) in section_start_indices.iter().enumerate() {
        let entry = &entries[*entry_idx];
        let title_id = format!("title-{}", sec_idx);
        let title_escaped = escape_html(section_title);

        // For the very first section, show title for 2s at the start
        // For subsequent sections, show title 2s before the first line
        let (title_start, title_duration) = if *entry_idx == 0 {
            (0.0, 2.0_f64.min(entry.start_time.max(2.0)))
        } else {
            let ts = if entry.start_time >= 2.0 {
                entry.start_time - 2.0
            } else {
                0.0
            };
            let td = 2.0_f64.min(entry.start_time);
            (ts, td)
        };

        clips.push_str(&format!(
            "    <div id=\"{id}\" class=\"clip title-card\" \
             data-start=\"{start}\" data-duration=\"{dur}\" data-track-index=\"{track}\">\n\
             \x20     <div class=\"title-content\">\n\
             \x20       <span class=\"section-label\">CHAPTER {num}</span>\n\
             \x20       <h1 class=\"section-title\">{title}</h1>\n\
             \x20     </div>\n\
             \x20   </div>\n",
            id = title_id,
            start = format_time(title_start),
            dur = format_time(title_duration),
            track = track_index,
            num = sec_idx + 1,
            title = title_escaped,
        ));

        // Title card entrance animation
        let label_sel = format!("#{} .section-label", title_id);
        let title_sel = format!("#{} .section-title", title_id);
        tweens.push_str(&tween_from(
            &label_sel,
            "y: -20, opacity: 0, duration: 0.4, ease: \"power2.out\"",
            title_start + 0.2,
        ));
        tweens.push_str(&tween_from(
            &title_sel,
            "y: 40, opacity: 0, duration: 0.6, ease: \"power3.out\"",
            title_start + 0.4,
        ));

        // Title card exit
        if title_duration > 1.0 {
            let exit_t = title_start + title_duration - 0.4;
            let content_sel = format!("#{} .title-content", title_id);
            tweens.push_str(&tween_to(
                &content_sel,
                "opacity: 0, duration: 0.3, ease: \"power2.in\"",
                exit_t,
            ));
        }
    }

    track_index += 1;

    // Generate text clips for each entry
    for (i, entry) in entries.iter().enumerate() {
        let clip_id = format!("clip-{}", i);
        let text = escape_html(&entry.text);
        let char_name_html = entry
            .character_name
            .as_deref()
            .map(|n| format!("<span class=\"speaker\">{}</span>\n        ", escape_html(n)))
            .unwrap_or_default();

        clips.push_str(&format!(
            "    <div id=\"{id}\" class=\"clip chapter-line\" \
             data-start=\"{start}\" data-duration=\"{dur}\" data-track-index=\"{track}\">\n\
             \x20     <div class=\"line-content\">\n\
             \x20       {name}<p class=\"line-text\">{text}</p>\n\
             \x20     </div>\n\
             \x20   </div>\n",
            id = clip_id,
            start = format_time(entry.start_time),
            dur = format_time(entry.duration),
            track = track_index,
            name = char_name_html,
            text = text,
        ));

        // Fade in
        let selector = format!("#{} .line-content", clip_id);
        tweens.push_str(&tween_from(
            &selector,
            "y: 20, opacity: 0, duration: 0.4, ease: \"power2.out\"",
            entry.start_time + 0.1,
        ));

        // Fade out
        if entry.duration > 1.0 {
            let exit_time = entry.start_time + entry.duration - 0.4;
            tweens.push_str(&tween_to(
                &selector,
                "y: -15, opacity: 0, duration: 0.3, ease: \"power2.in\"",
                exit_time,
            ));
        }
    }

    format!(
        "{doctype}\n\
         <html>\n\
         <head>\n\
         \x20 <meta charset=\"UTF-8\">\n\
         \x20 <style>\n\
         {style}\
         \x20 </style>\n\
         </head>\n\
         <body>\n\
         \x20 <div data-composition-id=\"chapter-sections\" data-width=\"1920\" data-height=\"1080\" \
         data-start=\"0\" data-duration=\"{duration}\">\n\
         {clips}\
         \x20   <script src=\"https://cdn.jsdelivr.net/npm/gsap@3.12.5/dist/gsap.min.js\"></script>\n\
         \x20   <script>\n\
         \x20     window.__timelines = window.__timelines || {{}};\n\
         \x20     const tl = gsap.timeline({{ paused: true }});\n\
         {tweens}\
         \x20     window.__timelines[\"chapter-sections\"] = tl;\n\
         \x20   </script>\n\
         \x20 </div>\n\
         </body>\n\
         </html>",
        doctype = "<!DOCTYPE html>",
        style = CHAPTER_SECTIONS_STYLE,
        duration = format_time(duration),
        clips = clips,
        tweens = tweens,
    )
}

const CHAPTER_SECTIONS_STYLE: &str = "\
    :root {\n\
      --bg-color: #0d0d14;\n\
      --text-color: #e8e8ed;\n\
      --accent-color: #a78bfa;\n\
      --title-color: #c4b5fd;\n\
      --font-family: \"Inter\", \"Noto Sans SC\", sans-serif;\n\
    }\n\
    [data-composition-id=\"chapter-sections\"] {\n\
      background: var(--bg-color);\n\
      overflow: hidden;\n\
      font-family: var(--font-family);\n\
      position: relative;\n\
    }\n\
    .title-card {\n\
      position: absolute;\n\
      width: 100%;\n\
      height: 100%;\n\
      display: flex;\n\
      align-items: center;\n\
      justify-content: center;\n\
      box-sizing: border-box;\n\
    }\n\
    .title-content {\n\
      text-align: center;\n\
    }\n\
    .section-label {\n\
      display: block;\n\
      font-size: 24px;\n\
      font-weight: 600;\n\
      color: var(--accent-color);\n\
      letter-spacing: 0.2em;\n\
      text-transform: uppercase;\n\
      margin-bottom: 24px;\n\
    }\n\
    .section-title {\n\
      font-size: 96px;\n\
      font-weight: 700;\n\
      color: var(--title-color);\n\
      margin: 0;\n\
      line-height: 1.2;\n\
    }\n\
    .chapter-line {\n\
      position: absolute;\n\
      width: 100%;\n\
      height: 100%;\n\
      display: flex;\n\
      align-items: center;\n\
      justify-content: center;\n\
      padding: 120px 200px;\n\
      box-sizing: border-box;\n\
    }\n\
    .line-content {\n\
      text-align: center;\n\
      max-width: 1400px;\n\
    }\n\
    .speaker {\n\
      display: block;\n\
      font-size: 28px;\n\
      font-weight: 600;\n\
      color: var(--accent-color);\n\
      margin-bottom: 16px;\n\
    }\n\
    .line-text {\n\
      color: var(--text-color);\n\
      font-size: 56px;\n\
      font-weight: 400;\n\
      line-height: 1.5;\n\
      margin: 0;\n\
    }\n";

#[cfg(test)]
mod tests {
    use super::*;

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
            TimelineEntry {
                line_id: "l3".to_string(),
                text: "你好，请问这里有旅馆吗？".to_string(),
                character_name: Some("旅人".to_string()),
                section_title: Some("第二章".to_string()),
                start_time: 6.5,
                duration: 2.0,
            },
        ]
    }

    #[test]
    fn test_generate_html_dispatcher() {
        let entries = sample_entries();
        assert!(generate_html("minimal-subtitle", &entries).is_ok());
        assert!(generate_html("dialogue-cards", &entries).is_ok());
        assert!(generate_html("chapter-sections", &entries).is_ok());
        assert!(generate_html("unknown-template", &entries).is_err());
    }

    #[test]
    fn test_minimal_subtitle_structure() {
        let entries = sample_entries();
        let html = generate_minimal_subtitle(&entries);

        // Root element attributes
        assert!(html.contains("data-composition-id=\"minimal-subtitle\""));
        assert!(html.contains("data-width=\"1920\""));
        assert!(html.contains("data-height=\"1080\""));

        // Clips with required attributes
        assert!(html.contains("class=\"clip subtitle-line\""));
        assert!(html.contains("data-start="));
        assert!(html.contains("data-duration="));
        assert!(html.contains("data-track-index="));

        // GSAP timeline registration
        assert!(html.contains("window.__timelines"));
        assert!(html.contains("gsap.timeline({ paused: true })"));
        assert!(html.contains("window.__timelines[\"minimal-subtitle\"] = tl"));

        // No forbidden patterns
        assert!(!html.contains("Math.random()"));
        assert!(!html.contains("Date.now()"));
        assert!(!html.contains("repeat: -1"));

        // CSS variables
        assert!(html.contains("--bg-color"));
        assert!(html.contains("--text-color"));

        // Content
        assert!(html.contains("在一个风雨交加的夜晚"));
    }

    #[test]
    fn test_dialogue_cards_structure() {
        let entries = sample_entries();
        let html = generate_dialogue_cards(&entries);

        // Root element
        assert!(html.contains("data-composition-id=\"dialogue-cards\""));
        assert!(html.contains("data-width=\"1920\""));
        assert!(html.contains("data-height=\"1080\""));

        // Clips
        assert!(html.contains("class=\"clip dialogue-card"));
        assert!(html.contains("data-start="));
        assert!(html.contains("data-duration="));
        assert!(html.contains("data-track-index="));

        // GSAP
        assert!(html.contains("window.__timelines"));
        assert!(html.contains("window.__timelines[\"dialogue-cards\"] = tl"));

        // Character names displayed
        assert!(html.contains("旁白"));
        assert!(html.contains("旅人"));

        // CSS variables
        assert!(html.contains("--bg-color"));
        assert!(html.contains("--text-color"));
        assert!(html.contains("--card-color"));

        // No forbidden patterns
        assert!(!html.contains("Math.random()"));
        assert!(!html.contains("Date.now()"));
        assert!(!html.contains("repeat: -1"));
    }

    #[test]
    fn test_chapter_sections_structure() {
        let entries = sample_entries();
        let html = generate_chapter_sections(&entries);

        // Root element
        assert!(html.contains("data-composition-id=\"chapter-sections\""));
        assert!(html.contains("data-width=\"1920\""));
        assert!(html.contains("data-height=\"1080\""));

        // Title cards for sections
        assert!(html.contains("CHAPTER 1"));
        assert!(html.contains("CHAPTER 2"));
        assert!(html.contains("第一章"));
        assert!(html.contains("第二章"));

        // Clips
        assert!(html.contains("class=\"clip title-card\""));
        assert!(html.contains("class=\"clip chapter-line\""));
        assert!(html.contains("data-start="));
        assert!(html.contains("data-duration="));
        assert!(html.contains("data-track-index="));

        // GSAP
        assert!(html.contains("window.__timelines"));
        assert!(html.contains("window.__timelines[\"chapter-sections\"] = tl"));

        // CSS variables
        assert!(html.contains("--bg-color"));
        assert!(html.contains("--text-color"));
        assert!(html.contains("--accent-color"));

        // No forbidden patterns
        assert!(!html.contains("Math.random()"));
        assert!(!html.contains("Date.now()"));
        assert!(!html.contains("repeat: -1"));
    }

    #[test]
    fn test_empty_entries() {
        let entries: Vec<TimelineEntry> = vec![];
        let html = generate_minimal_subtitle(&entries);
        assert!(html.contains("data-composition-id=\"minimal-subtitle\""));
        assert!(html.contains("data-duration=\"0\""));
    }

    #[test]
    fn test_html_escaping() {
        let entries = vec![TimelineEntry {
            line_id: "l1".to_string(),
            text: "He said <hello> & \"goodbye\"".to_string(),
            character_name: Some("Test's Character".to_string()),
            section_title: None,
            start_time: 0.0,
            duration: 2.0,
        }];

        let html = generate_dialogue_cards(&entries);
        assert!(html.contains("&lt;hello&gt;"));
        assert!(html.contains("&amp;"));
        assert!(html.contains("&quot;goodbye&quot;"));
        assert!(html.contains("Test&#39;s Character"));
        // Should NOT contain raw special chars in content
        assert!(!html.contains("<hello>"));
    }

    #[test]
    fn test_format_time() {
        assert_eq!(format_time(0.0), "0");
        assert_eq!(format_time(3.0), "3");
        assert_eq!(format_time(2.5), "2.5");
        assert_eq!(format_time(1.25), "1.25");
        assert_eq!(format_time(10.0), "10");
    }

    #[test]
    fn test_total_duration() {
        let entries = sample_entries();
        let dur = total_duration(&entries);
        // Last entry: start=6.5, duration=2.0 → end=8.5
        assert!((dur - 8.5).abs() < f64::EPSILON);
    }

    #[test]
    fn test_character_color_consistency() {
        // Same character should always get the same color
        let idx1 = character_color_index("Alice", 6);
        let idx2 = character_color_index("Alice", 6);
        assert_eq!(idx1, idx2);

        // Different characters should (likely) get different colors
        let idx_a = character_color_index("Alice", 6);
        let idx_b = character_color_index("Bob", 6);
        assert!(idx_a < 6);
        assert!(idx_b < 6);
    }

    #[test]
    fn test_generate_meta_json() {
        let json_str = generate_meta_json("minimal-subtitle", "我的有声书项目", 120.5);
        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();

        assert_eq!(parsed["id"], "minimal-subtitle");
        assert_eq!(parsed["title"], "我的有声书项目");
        assert_eq!(parsed["width"], 1920);
        assert_eq!(parsed["height"], 1080);
        assert_eq!(parsed["fps"], 30);
        assert_eq!(parsed["duration"], 120.5);
    }

    #[test]
    fn test_generate_meta_json_zero_duration() {
        let json_str = generate_meta_json("dialogue-cards", "Empty Project", 0.0);
        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();

        assert_eq!(parsed["id"], "dialogue-cards");
        assert_eq!(parsed["title"], "Empty Project");
        assert_eq!(parsed["duration"], 0.0);
    }
}
