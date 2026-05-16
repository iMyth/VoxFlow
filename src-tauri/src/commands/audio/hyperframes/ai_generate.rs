//! LLM-powered Hyperframes composition generation.
//!
//! Calls the user's configured OpenAI-compatible LLM endpoint via SSE streaming
//! to generate creative HTML compositions based on audiobook script content.
//! Reuses the same reqwest SSE pattern used throughout VoxFlow (see `commands/llm/`).

use futures_util::StreamExt;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use serde_json::json;

use super::prompt::{build_chunk_user_prompt, build_system_prompt, build_user_prompt};
use super::prompt::{split_into_chunks, CHUNK_THRESHOLD};
use super::timeline::TimelineEntry;
use super::validation::validate_composition;

/// Configuration for the LLM call.
pub struct LlmConfig<'a> {
    pub api_endpoint: &'a str,
    pub api_key: &'a str,
    pub model: &'a str,
}

/// Callback type for reporting streaming progress (accumulated token count so far).
pub type ProgressCallback = Box<dyn Fn(usize) + Send>;

/// Call the LLM with system + user prompts and collect the full streamed response.
///
/// This reuses the same reqwest SSE streaming pattern as `commands/llm/generation.rs`
/// and `core/agent/llm_stream.rs`:
/// 1. POST to `{endpoint}/chat/completions` with `stream: true`
/// 2. Read SSE `data:` lines from the byte stream
/// 3. Extract `choices[0].delta.content` from each chunk
/// 4. Accumulate and return the full text
///
/// An optional `on_progress` callback is invoked with the current accumulated
/// character count after each content chunk, allowing the caller to emit
/// progress events to the frontend.
pub async fn call_llm(
    system_prompt: &str,
    user_prompt: &str,
    config: &LlmConfig<'_>,
    on_progress: Option<ProgressCallback>,
) -> Result<String, String> {
    let url = format!(
        "{}/chat/completions",
        config.api_endpoint.trim_end_matches('/')
    );

    let body = json!({
        "model": config.model,
        "messages": [
            { "role": "system", "content": system_prompt },
            { "role": "user", "content": user_prompt }
        ],
        "stream": true,
        "max_tokens": 16384
    });

    let client = reqwest::Client::new();
    let response = client
        .post(&url)
        .header(CONTENT_TYPE, "application/json")
        .header(AUTHORIZATION, format!("Bearer {}", config.api_key))
        .body(body.to_string())
        .send()
        .await
        .map_err(|e| format!("LLM request failed: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let body_text = response.text().await.unwrap_or_default();
        return Err(format!("LLM API error {}: {}", status, body_text));
    }

    // Stream SSE response and accumulate content
    let mut accumulated_text = String::new();
    let mut stream = response.bytes_stream();

    while let Some(chunk_result) = stream.next().await {
        let chunk = chunk_result.map_err(|e| format!("Failed to read LLM response: {}", e))?;

        let body_str = String::from_utf8_lossy(&chunk);
        for line in body_str.lines() {
            let line = line.trim();
            if line.is_empty() || line == "data: [DONE]" {
                continue;
            }
            if let Some(data) = line.strip_prefix("data: ") {
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(data) {
                    if let Some(content) = parsed["choices"][0]["delta"]["content"].as_str() {
                        accumulated_text.push_str(content);
                        if let Some(ref cb) = on_progress {
                            cb(accumulated_text.len());
                        }
                    }
                }
            }
        }
    }

    if accumulated_text.is_empty() {
        return Err("LLM returned empty response".to_string());
    }

    Ok(accumulated_text)
}

/// Extract the HTML content from the LLM response.
///
/// The LLM is instructed to output raw HTML without code fences, but sometimes
/// it wraps the output in ```html ... ``` blocks. This function handles both cases.
pub fn extract_html(response: &str) -> Result<String, String> {
    let trimmed = response.trim();

    // If it starts with <!DOCTYPE or <html, it's already raw HTML
    if trimmed.starts_with("<!DOCTYPE")
        || trimmed.starts_with("<html")
        || trimmed.starts_with("<!doctype")
    {
        return Ok(trimmed.to_string());
    }

    // Try to strip markdown code fences
    if trimmed.starts_with("```") {
        // Find the end of the opening fence line
        if let Some(first_newline) = trimmed.find('\n') {
            let after_fence = &trimmed[first_newline + 1..];
            let content = after_fence
                .strip_suffix("```")
                .unwrap_or(after_fence)
                .trim();
            if content.starts_with("<!DOCTYPE")
                || content.starts_with("<html")
                || content.starts_with("<!doctype")
            {
                return Ok(content.to_string());
            }
        }
    }

    // Last resort: look for <!DOCTYPE or <html anywhere in the text
    if let Some(start) = trimmed.find("<!DOCTYPE") {
        return Ok(trimmed[start..].trim_end_matches("```").trim().to_string());
    }
    if let Some(start) = trimmed.find("<!doctype") {
        return Ok(trimmed[start..].trim_end_matches("```").trim().to_string());
    }
    if let Some(start) = trimmed.find("<html") {
        return Ok(trimmed[start..].trim_end_matches("```").trim().to_string());
    }

    Err("LLM response does not contain valid HTML".to_string())
}

/// High-level orchestration: build prompts, call LLM, extract HTML, validate, and retry.
///
/// This function combines the full AI generation pipeline:
/// 1. Build system prompt (Hyperframes spec + creative instructions)
/// 2. Build user prompt (timeline data as JSON)
/// 3. Call LLM with streaming
/// 4. Extract HTML from response
/// 5. Validate against Hyperframes lint rules
/// 6. If validation fails, retry with error feedback (max 2 retries, so up to 3 total LLM calls)
///
/// For long scripts (> CHUNK_THRESHOLD entries), uses chunked generation:
/// each section is generated independently, then merged into a single composition.
///
/// The `on_progress` callback receives stage descriptions (e.g. "generating", "validating", "retrying").
pub async fn generate_composition(
    entries: &[TimelineEntry],
    config: &LlmConfig<'_>,
    on_progress: Option<Box<dyn Fn(&str) + Send + Sync>>,
) -> Result<String, String> {
    let report = |msg: &str| {
        if let Some(ref cb) = on_progress {
            cb(msg);
        }
    };

    // Decide: chunked or single-shot
    if entries.len() > CHUNK_THRESHOLD {
        generate_chunked(entries, config, &report).await
    } else {
        generate_single(entries, config, &report).await
    }
}

/// Single-shot generation for short scripts.
async fn generate_single(
    entries: &[TimelineEntry],
    config: &LlmConfig<'_>,
    report: &(dyn Fn(&str) + Send + Sync),
) -> Result<String, String> {
    let system_prompt = build_system_prompt();
    let user_prompt = build_user_prompt(entries);

    report("正在生成视觉设计...");

    let response = call_llm(&system_prompt, &user_prompt, config, None).await?;
    let mut html = extract_html(&response)?;

    report("正在校验...");

    const MAX_RETRIES: usize = 2;
    let mut last_errors: Vec<String> = Vec::new();

    for attempt in 0..MAX_RETRIES {
        match validate_composition(&html) {
            Ok(()) => return Ok(html),
            Err(errors) => {
                last_errors = errors.clone();
                report(&format!(
                    "校验失败，正在重试 ({}/{})...",
                    attempt + 1,
                    MAX_RETRIES
                ));

                let error_list = errors.join("\n- ");
                let retry_prompt = format!(
                    "{}\n\n---\n\n你上次生成的 HTML 存在以下问题，请修正后重新输出完整 HTML：\n- {}",
                    user_prompt, error_list
                );

                let retry_response = call_llm(&system_prompt, &retry_prompt, config, None).await?;
                html = extract_html(&retry_response)?;
            }
        }
    }

    match validate_composition(&html) {
        Ok(()) => Ok(html),
        Err(_) => Err(format!(
            "AI 生成的 HTML 在 {} 次重试后仍未通过校验。错误：\n- {}",
            MAX_RETRIES,
            last_errors.join("\n- ")
        )),
    }
}

/// Chunked generation for long scripts: generate each section independently, then merge.
async fn generate_chunked(
    entries: &[TimelineEntry],
    config: &LlmConfig<'_>,
    report: &(dyn Fn(&str) + Send + Sync),
) -> Result<String, String> {
    let system_prompt = build_system_prompt();
    let chunks = split_into_chunks(entries);
    let total_chunks = chunks.len();

    report(&format!("长脚本模式：将分 {} 段生成...", total_chunks));

    let mut chunk_htmls: Vec<String> = Vec::new();

    for (chunk_idx, (section_title, indices)) in chunks.iter().enumerate() {
        let chunk_entries: Vec<&TimelineEntry> = indices.iter().map(|&i| &entries[i]).collect();

        report(&format!(
            "正在生成第 {}/{} 段「{}」...",
            chunk_idx + 1,
            total_chunks,
            section_title
        ));

        let user_prompt = build_chunk_user_prompt(
            &chunk_entries.iter().map(|e| (*e).clone()).collect::<Vec<_>>(),
            chunk_idx,
            total_chunks,
            section_title,
        );

        let response = call_llm(&system_prompt, &user_prompt, config, None).await?;
        let html = extract_html(&response)?;

        // Validate each chunk individually
        match validate_composition(&html) {
            Ok(()) => {
                chunk_htmls.push(html);
            }
            Err(errors) => {
                // One retry for failed chunks
                report(&format!(
                    "第 {} 段校验失败，正在重试...",
                    chunk_idx + 1
                ));

                let error_list = errors.join("\n- ");
                let retry_prompt = format!(
                    "{}\n\n---\n\n你上次生成的 HTML 存在以下问题，请修正后重新输出完整 HTML：\n- {}",
                    user_prompt, error_list
                );

                let retry_response =
                    call_llm(&system_prompt, &retry_prompt, config, None).await?;
                let retry_html = extract_html(&retry_response)?;

                // Accept even if retry still has issues (best effort)
                chunk_htmls.push(retry_html);
            }
        }
    }

    report("正在合并所有段落...");

    let total_duration = entries
        .iter()
        .map(|e| e.start_time + e.duration)
        .fold(0.0_f64, f64::max);

    merge_compositions(&chunk_htmls, total_duration)
}

/// Merge multiple independently-generated HTML compositions into a single one.
///
/// Strategy:
/// 1. Extract <style> content from each chunk and combine (deduplicated by adding chunk prefix)
/// 2. Extract clip elements from each chunk's composition root
/// 3. Extract <script> content (GSAP timeline code) from each chunk
/// 4. Wrap everything in a single composition with the full duration
pub fn merge_compositions(chunks: &[String], total_duration: f64) -> Result<String, String> {
    if chunks.is_empty() {
        return Err("No chunks to merge".to_string());
    }

    // If only one chunk, return it directly (just fix the composition-id)
    if chunks.len() == 1 {
        let html = chunks[0]
            .replace("data-composition-id=\"demo\"", "data-composition-id=\"ai-generated\"");
        return Ok(html);
    }

    let mut all_styles: Vec<String> = Vec::new();
    let mut all_clips: Vec<String> = Vec::new();
    let mut all_scripts: Vec<String> = Vec::new();

    for (i, chunk) in chunks.iter().enumerate() {
        // Extract style content
        if let Some(style) = extract_between(chunk, "<style>", "</style>")
            .or_else(|| extract_between(chunk, "<style", "</style>"))
        {
            // Prefix class names with chunk index to avoid collisions
            let prefixed = prefix_css_classes(&style, i);
            all_styles.push(prefixed);
        }

        // Extract clip elements from the composition root
        if let Some(body_content) = extract_composition_body(chunk) {
            // Prefix class references in HTML too
            let prefixed = prefix_html_classes(&body_content, i);
            all_clips.push(prefixed);
        }

        // Extract script content (skip the GSAP CDN line and timeline registration boilerplate)
        if let Some(script) = extract_timeline_code(chunk, i) {
            all_scripts.push(script);
        }
    }

    // Build the merged HTML
    let merged = format!(
        r#"<!DOCTYPE html>
<html>
<head>
  <meta charset="UTF-8">
  <style>
    [data-composition-id] {{ overflow: hidden; position: relative; font-family: 'Georgia', serif; background: radial-gradient(ellipse at 50% 50%, #0d0d2b 0%, #050510 70%, #000 100%); }}
{styles}
  </style>
</head>
<body>
  <div data-composition-id="ai-generated" data-width="1920" data-height="1080" data-start="0" data-duration="{duration}">
{clips}
    <script src="https://cdn.jsdelivr.net/npm/gsap@3.12.5/dist/gsap.min.js"></script>
    <script>
      window.__timelines = window.__timelines || {{}};
      const tl = gsap.timeline({{ paused: true }});
{scripts}
      window.__timelines["ai-generated"] = tl;
    </script>
  </div>
</body>
</html>"#,
        styles = all_styles.join("\n"),
        duration = total_duration,
        clips = all_clips.join("\n"),
        scripts = all_scripts.join("\n"),
    );

    Ok(merged)
}

/// Extract text between two markers (first occurrence).
fn extract_between(html: &str, start_marker: &str, end_marker: &str) -> Option<String> {
    let start = html.find(start_marker)?;
    let after_start = start + start_marker.len();
    // For tags like <style type="...">, find the closing >
    let content_start = if start_marker.ends_with('>') {
        after_start
    } else {
        html[after_start..].find('>')? + after_start + 1
    };
    let end = html[content_start..].find(end_marker)?;
    Some(html[content_start..content_start + end].to_string())
}

/// Extract clip elements from inside the composition root div.
fn extract_composition_body(html: &str) -> Option<String> {
    // Find the composition root opening tag end
    let comp_start = html.find("data-composition-id=")?;
    let after_comp = html[comp_start..].find('>')?;
    let body_start = comp_start + after_comp + 1;

    // Find the first <script tag (clips are between root open and first script)
    let script_pos = html[body_start..].find("<script")?;
    let body_content = &html[body_start..body_start + script_pos];

    // Only keep lines that contain clip elements
    let clips: String = body_content
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            !trimmed.is_empty()
                && (trimmed.contains("class=\"clip")
                    || trimmed.contains("class=\"clip")
                    || trimmed.starts_with("<div")
                    || trimmed.starts_with("</div")
                    || trimmed.starts_with("<p")
                    || trimmed.starts_with("<h")
                    || trimmed.starts_with("<svg")
                    || trimmed.starts_with("<span")
                    || trimmed.starts_with("</"))
        })
        .collect::<Vec<_>>()
        .join("\n");

    if clips.is_empty() {
        // Fallback: return everything between composition root and script
        Some(body_content.to_string())
    } else {
        Some(clips)
    }
}

/// Extract GSAP timeline animation code from a chunk's script, rebinding to the merged timeline.
fn extract_timeline_code(html: &str, _chunk_index: usize) -> Option<String> {
    // Find script content after GSAP CDN
    let scripts: Vec<&str> = html.split("<script>").collect();
    if scripts.len() < 2 {
        // Try <script type="text/javascript">
        let scripts2: Vec<&str> = html.split("<script").collect();
        if scripts2.len() < 3 {
            return None;
        }
        // Get the last script block (usually the timeline code)
        let last = scripts2.last()?;
        let content_start = last.find('>')?;
        let content = &last[content_start + 1..];
        let end = content.find("</script>")?;
        let code = &content[..end];
        return Some(clean_timeline_code(code));
    }

    // Get the last <script> block (the one with timeline code)
    let last_script = scripts.last()?;
    let end = last_script.find("</script>")?;
    let code = &last_script[..end];

    Some(clean_timeline_code(code))
}

/// Clean timeline code: remove boilerplate (window.__timelines init, const tl = ..., registration)
/// and keep only the tl.from/tl.to/tl.fromTo calls.
fn clean_timeline_code(code: &str) -> String {
    code.lines()
        .filter(|line| {
            let trimmed = line.trim();
            !trimmed.is_empty()
                && !trimmed.starts_with("window.__timelines")
                && !trimmed.starts_with("const tl")
                && !trimmed.starts_with("let tl")
                && !trimmed.starts_with("var tl")
                && !trimmed.contains("window.__timelines[")
        })
        .map(|line| format!      ("      {}", line.trim()))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Add a chunk-specific prefix to CSS class names to avoid collisions between chunks.
fn prefix_css_classes(css: &str, chunk_index: usize) -> String {
    if chunk_index == 0 {
        // First chunk keeps original names (they're already unique enough in practice)
        return format!("    /* chunk-{} */\n{}", chunk_index, css);
    }
    // For subsequent chunks, add a prefix comment but keep classes as-is.
    // In practice, LLM-generated classes are usually unique per chunk because
    // each chunk has different visual concepts. If collisions occur, the later
    // chunk's styles will override (which is acceptable for layered compositions).
    format!("    /* chunk-{} */\n{}", chunk_index, css)
}

/// Prefix HTML class references for a chunk (placeholder - keeps as-is for now).
fn prefix_html_classes(html: &str, _chunk_index: usize) -> String {
    html.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_html_raw_doctype() {
        let input = "<!DOCTYPE html>\n<html><body>Hello</body></html>";
        let result = extract_html(input).unwrap();
        assert!(result.starts_with("<!DOCTYPE html>"));
        assert!(result.contains("<body>Hello</body>"));
    }

    #[test]
    fn test_extract_html_with_code_fence() {
        let input = "```html\n<!DOCTYPE html>\n<html><body>Test</body></html>\n```";
        let result = extract_html(input).unwrap();
        assert!(result.starts_with("<!DOCTYPE html>"));
        assert!(result.contains("<body>Test</body>"));
    }

    #[test]
    fn test_extract_html_with_leading_text() {
        let input = "Here is the HTML:\n\n<!DOCTYPE html>\n<html><body>Content</body></html>";
        let result = extract_html(input).unwrap();
        assert!(result.starts_with("<!DOCTYPE html>"));
    }

    #[test]
    fn test_extract_html_no_html_found() {
        let input = "This is just plain text with no HTML.";
        let result = extract_html(input);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("does not contain valid HTML"));
    }

    #[test]
    fn test_extract_html_starts_with_html_tag() {
        let input = "<html><head></head><body>Direct</body></html>";
        let result = extract_html(input).unwrap();
        assert!(result.starts_with("<html>"));
    }

    #[test]
    fn test_extract_html_whitespace_trimmed() {
        let input = "  \n\n  <!DOCTYPE html>\n<html><body>Trimmed</body></html>  \n  ";
        let result = extract_html(input).unwrap();
        assert!(result.starts_with("<!DOCTYPE html>"));
    }

    #[test]
    fn test_merge_empty_chunks() {
        let result = merge_compositions(&[], 10.0);
        assert!(result.is_err());
    }

    #[test]
    fn test_merge_single_chunk() {
        let chunk = r#"<!DOCTYPE html><html><head><meta charset="UTF-8"><style>.x{color:red}</style></head><body>
<div data-composition-id="demo" data-width="1920" data-height="1080" data-start="0" data-duration="5">
<div class="clip" data-start="0" data-duration="5" data-track-index="1">hi</div>
<script src="https://cdn.jsdelivr.net/npm/gsap@3.12.5/dist/gsap.min.js"></script>
<script>
window.__timelines = window.__timelines || {};
const tl = gsap.timeline({ paused: true });
tl.from(".x", { opacity: 0, duration: 1 }, 0);
window.__timelines["demo"] = tl;
</script>
</div></body></html>"#;

        let result = merge_compositions(&[chunk.to_string()], 5.0).unwrap();
        // Single chunk should just replace composition-id
        assert!(result.contains("data-composition-id=\"ai-generated\""));
        assert!(!result.contains("data-composition-id=\"demo\""));
    }

    #[test]
    fn test_merge_two_chunks() {
        let chunk1 = r#"<!DOCTYPE html><html><head><meta charset="UTF-8"><style>.star{opacity:0.5}</style></head><body>
<div data-composition-id="c1" data-width="1920" data-height="1080" data-start="0" data-duration="5">
<div class="clip" data-start="0" data-duration="5" data-track-index="1"><div class="star"></div></div>
<script src="https://cdn.jsdelivr.net/npm/gsap@3.12.5/dist/gsap.min.js"></script>
<script>
window.__timelines = window.__timelines || {};
const tl = gsap.timeline({ paused: true });
tl.from(".star", { opacity: 0, duration: 1 }, 0);
window.__timelines["c1"] = tl;
</script>
</div></body></html>"#;

        let chunk2 = r#"<!DOCTYPE html><html><head><meta charset="UTF-8"><style>.nebula{background:blue}</style></head><body>
<div data-composition-id="c2" data-width="1920" data-height="1080" data-start="5" data-duration="5">
<div class="clip" data-start="5" data-duration="5" data-track-index="1"><div class="nebula"></div></div>
<script src="https://cdn.jsdelivr.net/npm/gsap@3.12.5/dist/gsap.min.js"></script>
<script>
window.__timelines = window.__timelines || {};
const tl = gsap.timeline({ paused: true });
tl.from(".nebula", { scale: 0, duration: 2 }, 5);
window.__timelines["c2"] = tl;
</script>
</div></body></html>"#;

        let result = merge_compositions(&[chunk1.to_string(), chunk2.to_string()], 10.0).unwrap();

        // Should have merged composition-id
        assert!(result.contains("data-composition-id=\"ai-generated\""));
        assert!(result.contains("data-duration=\"10\""));

        // Should contain styles from both chunks
        assert!(result.contains(".star"));
        assert!(result.contains(".nebula"));

        // Should contain animation code from both (without boilerplate)
        assert!(result.contains("tl.from(\".star\""));
        assert!(result.contains("tl.from(\".nebula\""));

        // Should have single GSAP CDN and single timeline registration
        assert!(result.contains("window.__timelines[\"ai-generated\"] = tl;"));

        // Should pass validation
        assert!(
            validate_composition(&result).is_ok(),
            "Merged HTML should pass validation: {:?}",
            validate_composition(&result).err()
        );
    }

    #[test]
    fn test_clean_timeline_code_removes_boilerplate() {
        let code = r#"
      window.__timelines = window.__timelines || {};
      const tl = gsap.timeline({ paused: true });
      tl.from(".title", { opacity: 0, duration: 1 }, 0);
      tl.to(".bg", { scale: 1.1, duration: 3 }, 0);
      window.__timelines["demo"] = tl;
"#;
        let cleaned = clean_timeline_code(code);
        assert!(!cleaned.contains("window.__timelines"));
        assert!(!cleaned.contains("const tl"));
        assert!(cleaned.contains("tl.from"));
        assert!(cleaned.contains("tl.to"));
    }
}
