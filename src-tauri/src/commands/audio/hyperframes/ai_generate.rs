//! LLM-powered Hyperframes composition generation.
//!
//! Calls the user's configured OpenAI-compatible LLM endpoint via SSE streaming
//! to generate creative HTML compositions based on audiobook script content.
//! Reuses the same reqwest SSE pattern used throughout VoxFlow (see `commands/llm/`).

use futures_util::StreamExt;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use serde_json::json;

use super::prompt::{build_system_prompt, build_user_prompt};
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
        let chunk = chunk_result
            .map_err(|e| format!("Failed to read LLM response: {}", e))?;

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
    if trimmed.starts_with("<!DOCTYPE") || trimmed.starts_with("<html") || trimmed.starts_with("<!doctype") {
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
            if content.starts_with("<!DOCTYPE") || content.starts_with("<html") || content.starts_with("<!doctype") {
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
/// The `on_progress` callback receives stage descriptions (e.g. "generating", "validating", "retrying").
pub async fn generate_composition(
    entries: &[TimelineEntry],
    config: &LlmConfig<'_>,
    on_progress: Option<Box<dyn Fn(&str) + Send + Sync>>,
) -> Result<String, String> {
    let system_prompt = build_system_prompt();
    let user_prompt = build_user_prompt(entries);

    let report = |msg: &str| {
        if let Some(ref cb) = on_progress {
            cb(msg);
        }
    };

    report("正在生成视觉设计...");

    // Initial LLM call
    let response = call_llm(&system_prompt, &user_prompt, config, None).await?;
    let mut html = extract_html(&response)?;

    report("正在校验...");

    // Validate and retry up to 2 times
    const MAX_RETRIES: usize = 2;
    let mut last_errors: Vec<String> = Vec::new();

    for attempt in 0..MAX_RETRIES {
        match validate_composition(&html) {
            Ok(()) => {
                return Ok(html);
            }
            Err(errors) => {
                last_errors = errors.clone();
                report(&format!(
                    "校验失败，正在重试 ({}/{})...",
                    attempt + 1,
                    MAX_RETRIES
                ));

                // Build retry prompt with error feedback
                let error_list = errors.join("\n- ");
                let retry_prompt = format!(
                    "{}\n\n---\n\n你上次生成的 HTML 存在以下问题，请修正后重新输出完整 HTML：\n- {}",
                    user_prompt, error_list
                );

                let retry_response =
                    call_llm(&system_prompt, &retry_prompt, config, None).await?;
                html = extract_html(&retry_response)?;
            }
        }
    }

    // Final validation after all retries
    match validate_composition(&html) {
        Ok(()) => Ok(html),
        Err(_) => Err(format!(
            "AI 生成的 HTML 在 {} 次重试后仍未通过校验。错误：\n- {}",
            MAX_RETRIES,
            last_errors.join("\n- ")
        )),
    }
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
}
