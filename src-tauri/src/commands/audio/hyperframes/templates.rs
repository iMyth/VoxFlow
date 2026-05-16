//! Fixed template HTML generation for Hyperframes video export.
//!
//! Provides 3 preset visual templates that generate complete, self-contained
//! HTML compositions conforming to the Hyperframes spec:
//! - minimal-subtitle: Cinematic dark background + centered text with glow
//! - dialogue-cards: Colored dialogue bubbles per character with ambient particles
//! - chapter-sections: Segmented by section with title card transitions and atmosphere

use serde_json::json;

use super::timeline::TimelineEntry;

/// Generate a complete Hyperframes HTML composition using the specified template.
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

// ─── Utilities ───────────────────────────────────────────────────────────────

fn total_duration(entries: &[TimelineEntry]) -> f64 {
    entries
        .iter()
        .map(|e| e.start_time + e.duration)
        .fold(0.0_f64, f64::max)
}

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn format_time(t: f64) -> String {
    if (t - t.round()).abs() < 0.001 {
        format!("{:.0}", t)
    } else {
        let s = format!("{:.2}", t);
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

/// Determine font size based on text character count.
/// Long text gets smaller font to avoid overflow.
fn font_size_for_text(text: &str, base_size: u32) -> u32 {
    let char_count = text.chars().count();
    if char_count <= 15 {
        base_size
    } else if char_count <= 30 {
        (base_size as f32 * 0.85) as u32
    } else if char_count <= 60 {
        (base_size as f32 * 0.7) as u32
    } else if char_count <= 100 {
        (base_size as f32 * 0.55) as u32
    } else {
        (base_size as f32 * 0.42) as u32
    }
}

fn tween_from(selector: &str, props: &str, time: f64) -> String {
    format!(
        "      tl.from(\"{}\", {{ {} }}, {});\n",
        selector, props, format_time(time)
    )
}

fn tween_to(selector: &str, props: &str, time: f64) -> String {
    format!(
        "      tl.to(\"{}\", {{ {} }}, {});\n",
        selector, props, format_time(time)
    )
}

fn tween_fromto(selector: &str, from: &str, to: &str, time: f64) -> String {
    format!(
        "      tl.fromTo(\"{}\", {{ {} }}, {{ {} }}, {});\n",
        selector, from, to, format_time(time)
    )
}

/// Generate deterministic dust particle divs (no Math.random).
fn generate_dust_particles(count: usize) -> String {
    let positions: Vec<(u32, u32)> = (0..count)
        .map(|i| {
            // Deterministic spread using golden ratio
            let y = ((i as f64 * 61.8) % 100.0) as u32;
            let x = ((i as f64 * 37.3 + 13.7) % 100.0) as u32;
            (y, x)
        })
        .collect();

    positions
        .iter()
        .map(|(y, x)| format!("      <div class=\"dust\" style=\"top:{}%;left:{}%\"></div>", y, x))
        .collect::<Vec<_>>()
        .join("\n")
}


// ─────────────────────────────────────────────────────────────────────────────
// Template 1: minimal-subtitle (cinematic)
// ─────────────────────────────────────────────────────────────────────────────

fn generate_minimal_subtitle(entries: &[TimelineEntry]) -> String {
    let duration = total_duration(entries);
    let mut clips = String::new();
    let mut tweens = String::new();

    // Track 1: Ambient background with particles (full duration)
    let dust = generate_dust_particles(20);
    clips.push_str(&format!(
        "    <div class=\"clip ambient-layer\" data-start=\"0\" data-duration=\"{dur}\" data-track-index=\"1\">\n\
         \x20     <div class=\"bg-gradient\"></div>\n\
         \x20     <div class=\"vignette\"></div>\n\
         {dust}\n\
         \x20   </div>\n",
        dur = format_time(duration),
        dust = dust,
    ));

    // Dust drift animation
    tweens.push_str(&tween_fromto(
        ".dust",
        "opacity: 0",
        "opacity: 0.4, y: -40, duration: 60, stagger: 0.3, ease: \"none\"",
        1.0,
    ));

    // Track 2: Text clips (each line in its own clip)
    for (i, entry) in entries.iter().enumerate() {
        let clip_id = format!("sub-{}", i);
        let text = escape_html(&entry.text);
        let font_size = font_size_for_text(&entry.text, 56);

        clips.push_str(&format!(
            "    <div id=\"{id}\" class=\"clip text-clip\" \
             data-start=\"{start}\" data-duration=\"{dur}\" data-track-index=\"2\">\n\
             \x20     <p class=\"line-text\" style=\"font-size: {fs}px\">{text}</p>\n\
             \x20   </div>\n",
            id = clip_id,
            start = format_time(entry.start_time),
            dur = format_time(entry.duration),
            fs = font_size,
            text = text,
        ));

        let selector = format!("#{} .line-text", clip_id);
        tweens.push_str(&tween_from(
            &selector,
            "y: 25, opacity: 0, duration: 0.6, ease: \"power2.out\"",
            entry.start_time + 0.15,
        ));
    }

    format!(
        "<!DOCTYPE html>\n<html>\n<head>\n  <meta charset=\"UTF-8\">\n  <style>\n{style}\
         \n  </style>\n</head>\n<body>\n\
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
         \x20 </div>\n</body>\n</html>",
        style = MINIMAL_SUBTITLE_STYLE,
        duration = format_time(duration),
        clips = clips,
        tweens = tweens,
    )
}

const MINIMAL_SUBTITLE_STYLE: &str = "\
    [data-composition-id=\"minimal-subtitle\"] {\n\
      background: #030308;\n\
      overflow: hidden;\n\
      position: relative;\n\
      font-family: 'Georgia', 'PingFang SC', serif;\n\
    }\n\
    .clip { position: absolute; inset: 0; overflow: hidden; }\n\
    .ambient-layer { z-index: 1; }\n\
    .bg-gradient {\n\
      position: absolute; inset: 0;\n\
      background: radial-gradient(ellipse at 50% 45%, #0c1225 0%, #050510 55%, #000 100%);\n\
    }\n\
    .vignette {\n\
      position: absolute; inset: 0;\n\
      background: radial-gradient(ellipse at center, transparent 35%, rgba(0,0,0,0.7) 100%);\n\
    }\n\
    .dust {\n\
      position: absolute; width: 2px; height: 2px;\n\
      background: rgba(180,200,255,0.5); border-radius: 50%;\n\
      box-shadow: 0 0 4px rgba(140,170,255,0.3);\n\
    }\n\
    .text-clip {\n\
      z-index: 2;\n\
      display: flex; align-items: center; justify-content: center;\n\
      padding: 100px 180px; box-sizing: border-box;\n\
    }\n\
    .line-text {\n\
      color: rgba(230,235,245,0.92);\n\
      font-weight: 300;\n\
      text-align: center;\n\
      line-height: 1.6;\n\
      max-width: 1400px;\n\
      letter-spacing: 1px;\n\
      text-shadow: 0 0 20px rgba(100,140,220,0.3), 0 2px 10px rgba(0,0,0,0.8);\n\
    }\n";


// ─────────────────────────────────────────────────────────────────────────────
// Template 2: dialogue-cards
// ─────────────────────────────────────────────────────────────────────────────

fn character_color_index(name: &str, total_colors: usize) -> usize {
    let hash: u32 = name
        .bytes()
        .fold(0u32, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u32));
    (hash as usize) % total_colors
}

fn generate_dialogue_cards(entries: &[TimelineEntry]) -> String {
    let duration = total_duration(entries);
    let mut clips = String::new();
    let mut tweens = String::new();

    let colors = [
        ("#6366f1", "rgba(99,102,241,0.08)"),   // indigo
        ("#ec4899", "rgba(236,72,153,0.08)"),   // pink
        ("#14b8a6", "rgba(20,184,166,0.08)"),   // teal
        ("#f59e0b", "rgba(245,158,11,0.08)"),   // amber
        ("#8b5cf6", "rgba(139,92,246,0.08)"),   // violet
        ("#06b6d4", "rgba(6,182,212,0.08)"),    // cyan
    ];

    // Track 1: Ambient background
    let dust = generate_dust_particles(16);
    clips.push_str(&format!(
        "    <div class=\"clip ambient-layer\" data-start=\"0\" data-duration=\"{dur}\" data-track-index=\"1\">\n\
         \x20     <div class=\"bg-base\"></div>\n\
         {dust}\n\
         \x20   </div>\n",
        dur = format_time(duration),
        dust = dust,
    ));
    tweens.push_str(&tween_fromto(
        ".dust",
        "opacity: 0",
        "opacity: 0.3, y: -30, duration: 80, stagger: 0.4, ease: \"none\"",
        0.5,
    ));

    // Track 2: Dialogue cards
    for (i, entry) in entries.iter().enumerate() {
        let clip_id = format!("card-{}", i);
        let text = escape_html(&entry.text);
        let char_name = entry.character_name.as_deref().unwrap_or("旁白");
        let char_name_escaped = escape_html(char_name);
        let color_idx = character_color_index(char_name, colors.len());
        let (accent, bg_tint) = colors[color_idx];
        let font_size = font_size_for_text(&entry.text, 44);

        let align_class = if color_idx % 2 == 0 { "align-left" } else { "align-right" };

        clips.push_str(&format!(
            "    <div id=\"{id}\" class=\"clip card-clip {align}\" \
             data-start=\"{start}\" data-duration=\"{dur}\" data-track-index=\"2\" \
             style=\"--accent: {accent}; --bg-tint: {bg}\">\n\
             \x20     <div class=\"card-bubble\">\n\
             \x20       <span class=\"char-name\">{name}</span>\n\
             \x20       <p class=\"card-text\" style=\"font-size: {fs}px\">{text}</p>\n\
             \x20     </div>\n\
             \x20   </div>\n",
            id = clip_id,
            align = align_class,
            start = format_time(entry.start_time),
            dur = format_time(entry.duration),
            accent = accent,
            bg = bg_tint,
            name = char_name_escaped,
            fs = font_size,
            text = text,
        ));

        let x_from = if color_idx % 2 == 0 { -50 } else { 50 };
        let selector = format!("#{} .card-bubble", clip_id);
        tweens.push_str(&tween_from(
            &selector,
            &format!("x: {}, opacity: 0, scale: 0.96, duration: 0.5, ease: \"power3.out\"", x_from),
            entry.start_time + 0.1,
        ));
    }

    format!(
        "<!DOCTYPE html>\n<html>\n<head>\n  <meta charset=\"UTF-8\">\n  <style>\n{style}\
         \n  </style>\n</head>\n<body>\n\
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
         \x20 </div>\n</body>\n</html>",
        style = DIALOGUE_CARDS_STYLE,
        duration = format_time(duration),
        clips = clips,
        tweens = tweens,
    )
}

const DIALOGUE_CARDS_STYLE: &str = "\
    [data-composition-id=\"dialogue-cards\"] {\n\
      background: #080810;\n\
      overflow: hidden;\n\
      position: relative;\n\
      font-family: 'PingFang SC', 'Noto Sans SC', sans-serif;\n\
    }\n\
    .clip { position: absolute; inset: 0; overflow: hidden; }\n\
    .ambient-layer { z-index: 1; }\n\
    .bg-base {\n\
      position: absolute; inset: 0;\n\
      background: linear-gradient(160deg, #0a0a18 0%, #0d1020 50%, #080812 100%);\n\
    }\n\
    .dust {\n\
      position: absolute; width: 2px; height: 2px;\n\
      background: rgba(200,210,240,0.4); border-radius: 50%;\n\
    }\n\
    .card-clip {\n\
      z-index: 2;\n\
      display: flex; align-items: center;\n\
      padding: 80px 140px; box-sizing: border-box;\n\
    }\n\
    .card-clip.align-left { justify-content: flex-start; }\n\
    .card-clip.align-right { justify-content: flex-end; }\n\
    .card-bubble {\n\
      background: var(--bg-tint);\n\
      border: 1px solid color-mix(in srgb, var(--accent) 30%, transparent);\n\
      border-radius: 20px;\n\
      padding: 40px 56px;\n\
      max-width: 1200px;\n\
      backdrop-filter: blur(8px);\n\
      box-shadow: 0 4px 40px rgba(0,0,0,0.4), inset 0 1px 0 rgba(255,255,255,0.03);\n\
    }\n\
    .char-name {\n\
      display: block;\n\
      font-size: 22px;\n\
      font-weight: 600;\n\
      color: var(--accent);\n\
      margin-bottom: 14px;\n\
      letter-spacing: 0.08em;\n\
      text-shadow: 0 0 12px color-mix(in srgb, var(--accent) 40%, transparent);\n\
    }\n\
    .card-text {\n\
      color: rgba(240,242,248,0.9);\n\
      font-weight: 300;\n\
      line-height: 1.7;\n\
      margin: 0;\n\
    }\n";


// ─────────────────────────────────────────────────────────────────────────────
// Template 3: chapter-sections
// ─────────────────────────────────────────────────────────────────────────────

fn generate_chapter_sections(entries: &[TimelineEntry]) -> String {
    let duration = total_duration(entries);
    let mut clips = String::new();
    let mut tweens = String::new();

    // Track 1: Ambient background (full duration)
    let dust = generate_dust_particles(24);
    clips.push_str(&format!(
        "    <div class=\"clip ambient-layer\" data-start=\"0\" data-duration=\"{dur}\" data-track-index=\"1\">\n\
         \x20     <div class=\"bg-deep\"></div>\n\
         \x20     <div class=\"bg-glow\"></div>\n\
         {dust}\n\
         \x20   </div>\n",
        dur = format_time(duration),
        dust = dust,
    ));
    tweens.push_str(&tween_fromto(
        ".dust",
        "opacity: 0",
        "opacity: 0.35, y: -50, duration: 90, stagger: 0.25, ease: \"none\"",
        1.0,
    ));
    tweens.push_str(&tween_fromto(
        ".bg-glow",
        "opacity: 0.3, scale: 0.9",
        "opacity: 0.6, scale: 1.1, duration: 30, yoyo: true, repeat: 5, ease: \"power1.inOut\"",
        0.0,
    ));

    // Detect section boundaries
    let mut current_section: Option<String> = None;
    let mut section_starts: Vec<(usize, String)> = Vec::new();

    for (i, entry) in entries.iter().enumerate() {
        let section = entry
            .section_title
            .clone()
            .unwrap_or_else(|| "正文".to_string());
        if current_section.as_deref() != Some(&section) {
            section_starts.push((i, section.clone()));
            current_section = Some(section);
        }
    }

    // Track 2: Section title cards
    for (sec_idx, (entry_idx, section_title)) in section_starts.iter().enumerate() {
        let entry = &entries[*entry_idx];
        let title_id = format!("title-{}", sec_idx);
        let title_escaped = escape_html(section_title);

        let title_start = if entry.start_time >= 2.5 {
            entry.start_time - 2.5
        } else {
            0.0
        };
        let title_duration = 3.0_f64.min(entry.start_time + 0.5);

        clips.push_str(&format!(
            "    <div id=\"{id}\" class=\"clip title-card\" \
             data-start=\"{start}\" data-duration=\"{dur}\" data-track-index=\"2\">\n\
             \x20     <div class=\"title-inner\">\n\
             \x20       <span class=\"chapter-label\">— {num} —</span>\n\
             \x20       <h1 class=\"chapter-title\">{title}</h1>\n\
             \x20     </div>\n\
             \x20   </div>\n",
            id = title_id,
            start = format_time(title_start),
            dur = format_time(title_duration),
            num = sec_idx + 1,
            title = title_escaped,
        ));

        let label_sel = format!("#{} .chapter-label", title_id);
        let title_sel = format!("#{} .chapter-title", title_id);
        tweens.push_str(&tween_from(
            &label_sel,
            "opacity: 0, letterSpacing: \"0.1em\", duration: 0.6, ease: \"power2.out\"",
            title_start + 0.2,
        ));
        tweens.push_str(&tween_from(
            &title_sel,
            "opacity: 0, y: 30, duration: 0.8, ease: \"power3.out\"",
            title_start + 0.5,
        ));
    }

    // Track 3: Text lines
    for (i, entry) in entries.iter().enumerate() {
        let clip_id = format!("line-{}", i);
        let text = escape_html(&entry.text);
        let font_size = font_size_for_text(&entry.text, 48);
        let char_html = entry
            .character_name
            .as_deref()
            .map(|n| format!("<span class=\"speaker\">{}</span>", escape_html(n)))
            .unwrap_or_default();

        clips.push_str(&format!(
            "    <div id=\"{id}\" class=\"clip line-clip\" \
             data-start=\"{start}\" data-duration=\"{dur}\" data-track-index=\"3\">\n\
             \x20     <div class=\"line-inner\">\n\
             \x20       {char}\n\
             \x20       <p class=\"line-text\" style=\"font-size: {fs}px\">{text}</p>\n\
             \x20     </div>\n\
             \x20   </div>\n",
            id = clip_id,
            start = format_time(entry.start_time),
            dur = format_time(entry.duration),
            char = char_html,
            fs = font_size,
            text = text,
        ));

        let selector = format!("#{} .line-inner", clip_id);
        tweens.push_str(&tween_from(
            &selector,
            "y: 20, opacity: 0, duration: 0.5, ease: \"power2.out\"",
            entry.start_time + 0.1,
        ));
    }

    format!(
        "<!DOCTYPE html>\n<html>\n<head>\n  <meta charset=\"UTF-8\">\n  <style>\n{style}\
         \n  </style>\n</head>\n<body>\n\
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
         \x20 </div>\n</body>\n</html>",
        style = CHAPTER_SECTIONS_STYLE,
        duration = format_time(duration),
        clips = clips,
        tweens = tweens,
    )
}

const CHAPTER_SECTIONS_STYLE: &str = "\
    [data-composition-id=\"chapter-sections\"] {\n\
      background: #040608;\n\
      overflow: hidden;\n\
      position: relative;\n\
      font-family: 'Georgia', 'PingFang SC', serif;\n\
    }\n\
    .clip { position: absolute; inset: 0; overflow: hidden; }\n\
    .ambient-layer { z-index: 1; }\n\
    .bg-deep {\n\
      position: absolute; inset: 0;\n\
      background: radial-gradient(ellipse at 50% 40%, #0a1020 0%, #040810 50%, #000 100%);\n\
    }\n\
    .bg-glow {\n\
      position: absolute; top: 30%; left: 40%; width: 600px; height: 600px;\n\
      background: radial-gradient(circle, rgba(80,120,200,0.08) 0%, transparent 70%);\n\
      border-radius: 50%; filter: blur(40px);\n\
    }\n\
    .dust {\n\
      position: absolute; width: 2px; height: 2px;\n\
      background: rgba(160,190,240,0.4); border-radius: 50%;\n\
      box-shadow: 0 0 3px rgba(120,160,220,0.2);\n\
    }\n\
    .title-card {\n\
      z-index: 2;\n\
      display: flex; align-items: center; justify-content: center;\n\
    }\n\
    .title-inner { text-align: center; }\n\
    .chapter-label {\n\
      display: block;\n\
      font-size: 20px; font-weight: 300;\n\
      color: rgba(160,140,200,0.8);\n\
      letter-spacing: 0.3em;\n\
      margin-bottom: 20px;\n\
    }\n\
    .chapter-title {\n\
      font-size: 72px; font-weight: 300;\n\
      color: rgba(200,190,240,0.9);\n\
      margin: 0; line-height: 1.3;\n\
      letter-spacing: 3px;\n\
      text-shadow: 0 0 30px rgba(140,120,220,0.3);\n\
    }\n\
    .line-clip {\n\
      z-index: 3;\n\
      display: flex; align-items: center; justify-content: center;\n\
      padding: 100px 180px; box-sizing: border-box;\n\
    }\n\
    .line-inner { text-align: center; max-width: 1400px; }\n\
    .speaker {\n\
      display: block;\n\
      font-size: 20px; font-weight: 400;\n\
      color: rgba(160,140,200,0.7);\n\
      letter-spacing: 0.1em;\n\
      margin-bottom: 14px;\n\
    }\n\
    .line-text {\n\
      color: rgba(225,230,240,0.9);\n\
      font-weight: 300;\n\
      line-height: 1.7;\n\
      margin: 0;\n\
      text-shadow: 0 0 15px rgba(80,120,200,0.2), 0 2px 8px rgba(0,0,0,0.6);\n\
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
        assert!(html.contains("data-composition-id=\"minimal-subtitle\""));
        assert!(html.contains("data-width=\"1920\""));
        assert!(html.contains("data-height=\"1080\""));
        assert!(html.contains("class=\"clip"));
        assert!(html.contains("data-start="));
        assert!(html.contains("data-duration="));
        assert!(html.contains("data-track-index="));
        assert!(html.contains("window.__timelines"));
        assert!(html.contains("gsap.timeline({ paused: true })"));
        assert!(html.contains("window.__timelines[\"minimal-subtitle\"] = tl"));
        assert!(!html.contains("Math.random()"));
        assert!(!html.contains("Date.now()"));
        assert!(!html.contains("repeat: -1"));
        assert!(html.contains("在一个风雨交加的夜晚"));
        // Has ambient layer with dust
        assert!(html.contains("dust"));
        assert!(html.contains("bg-gradient"));
    }

    #[test]
    fn test_dialogue_cards_structure() {
        let entries = sample_entries();
        let html = generate_dialogue_cards(&entries);
        assert!(html.contains("data-composition-id=\"dialogue-cards\""));
        assert!(html.contains("data-width=\"1920\""));
        assert!(html.contains("data-height=\"1080\""));
        assert!(html.contains("class=\"clip"));
        assert!(html.contains("data-start="));
        assert!(html.contains("data-duration="));
        assert!(html.contains("data-track-index="));
        assert!(html.contains("window.__timelines"));
        assert!(html.contains("window.__timelines[\"dialogue-cards\"] = tl"));
        assert!(html.contains("旁白"));
        assert!(html.contains("旅人"));
        assert!(!html.contains("Math.random()"));
        assert!(!html.contains("Date.now()"));
        assert!(!html.contains("repeat: -1"));
    }

    #[test]
    fn test_chapter_sections_structure() {
        let entries = sample_entries();
        let html = generate_chapter_sections(&entries);
        assert!(html.contains("data-composition-id=\"chapter-sections\""));
        assert!(html.contains("data-width=\"1920\""));
        assert!(html.contains("data-height=\"1080\""));
        assert!(html.contains("第一章"));
        assert!(html.contains("第二章"));
        assert!(html.contains("class=\"clip title-card\""));
        assert!(html.contains("class=\"clip line-clip\""));
        assert!(html.contains("data-start="));
        assert!(html.contains("data-duration="));
        assert!(html.contains("data-track-index="));
        assert!(html.contains("window.__timelines"));
        assert!(html.contains("window.__timelines[\"chapter-sections\"] = tl"));
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
        assert!((dur - 8.5).abs() < f64::EPSILON);
    }

    #[test]
    fn test_character_color_consistency() {
        let idx1 = character_color_index("Alice", 6);
        let idx2 = character_color_index("Alice", 6);
        assert_eq!(idx1, idx2);
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

    #[test]
    fn test_font_size_scaling() {
        // Short text gets full size
        assert_eq!(font_size_for_text("短文本", 56), 56);
        // Medium text scales down
        let medium = "这是一段中等长度的文本，大约三十个字左右吧";
        assert!(font_size_for_text(medium, 56) < 56);
        // Long text scales down more
        let long = "这是一段非常非常长的文本，它包含了很多很多的字符，用来测试当文本过长时字体大小是否会自动缩小以避免溢出画面边界的情况";
        assert!(font_size_for_text(long, 56) < font_size_for_text(medium, 56));
    }
}
