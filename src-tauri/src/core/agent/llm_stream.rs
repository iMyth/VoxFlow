use futures_util::StreamExt;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use serde::Deserialize;
use serde_json::json;

use crate::core::error::AppError;
use crate::core::event_emitter::EventEmitter;

/// Configuration for a single streaming LLM request.
#[allow(dead_code)]
pub struct LlmStreamConfig<'a> {
    pub endpoint: &'a str,
    pub api_key: &'a str,
    pub model: &'a str,
    pub messages: Vec<serde_json::Value>,
    pub max_tokens: u32,
    pub enable_thinking: bool,
}

/// Result from a streamed LLM response.
#[allow(dead_code)]
pub struct LlmStreamResult {
    /// Full accumulated text content.
    pub content: String,
}

/// Stream an LLM response, emit events for tokens/reasoning, return accumulated text.
#[allow(dead_code)]
pub async fn stream_llm_response<E: EventEmitter>(
    emitter: &E,
    config: LlmStreamConfig<'_>,
) -> Result<LlmStreamResult, AppError> {
    let url = format!("{}/chat/completions", config.endpoint.trim_end_matches('/'));

    let body = json!({
        "model": config.model,
        "messages": config.messages,
        "stream": true,
        "max_tokens": config.max_tokens,
        "enable_thinking": config.enable_thinking,
    });

    let client = reqwest::Client::new();
    let response = client
        .post(&url)
        .header(CONTENT_TYPE, "application/json")
        .header(AUTHORIZATION, format!("Bearer {}", config.api_key))
        .body(body.to_string())
        .send()
        .await
        .map_err(|e| {
            let msg = format!("LLM request failed: {}", e);
            emitter.emit_json("llm-error", &json!(msg));
            AppError::LlmService(msg)
        })?;

    if !response.status().is_success() {
        let status = response.status();
        let body_text = response.text().await.unwrap_or_default();
        let msg = format!("LLM API error {}: {}", status, body_text);
        emitter.emit_json("llm-error", &json!(msg));
        return Err(AppError::LlmService(msg));
    }

    let content = collect_streaming_content(emitter, response).await?;

    emitter.emit_json("llm-complete", &json!(()));

    Ok(LlmStreamResult { content })
}

/// Parse SSE stream and accumulate text content, emitting events along the way.
pub async fn collect_streaming_content<E: EventEmitter>(
    emitter: &E,
    response: reqwest::Response,
) -> Result<String, AppError> {
    let mut accumulated = String::new();
    let mut stream = response.bytes_stream();

    while let Some(chunk_result) = stream.next().await {
        let chunk = chunk_result.map_err(|e| {
            let msg = format!("Failed to read LLM response: {}", e);
            emitter.emit_json("llm-error", &json!(msg));
            AppError::LlmService(msg)
        })?;

        let body_str = String::from_utf8_lossy(&chunk);
        for line in body_str.lines() {
            let line = line.trim();
            if line.is_empty() || line == "data: [DONE]" {
                continue;
            }
            if let Some(data) = line.strip_prefix("data: ") {
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(data) {
                    if let Some(reasoning) =
                        parsed["choices"][0]["delta"]["reasoning_content"].as_str()
                    {
                        emitter.emit_json("llm-thinking", &json!(reasoning));
                    }
                    if let Some(content) = parsed["choices"][0]["delta"]["content"].as_str() {
                        accumulated.push_str(content);
                        emitter.emit_json("llm-token", &json!(content));
                    }
                }
            }
        }
    }

    Ok(accumulated)
}

/// Extract JSON from text that may contain surrounding prose or markdown fences.
pub fn extract_json(text: &str) -> Option<&str> {
    let trimmed = text.trim();

    // Handle markdown code fences: ```json, ```JSON, ```\n, etc.
    let stripped = if trimmed.starts_with("```") {
        if let Some(first_newline) = trimmed.find('\n') {
            let after_fence = &trimmed[first_newline + 1..];
            after_fence
                .trim()
                .strip_suffix("```")
                .unwrap_or(after_fence.trim())
        } else {
            // ``` without newline — just strip the fences
            trimmed.trim_start_matches('`').trim_end_matches('`').trim()
        }
    } else {
        trimmed
    };

    // Try parsing the whole thing first
    if serde_json::from_str::<serde_json::Value>(stripped).is_ok() {
        return Some(stripped);
    }

    // Fall back to finding balanced braces
    let start = stripped.find('{')?;
    let end = find_closing_brace(&stripped[start..])?;
    Some(&stripped[start..=end])
}

/// Find the index of the closing '}' accounting for strings and nesting.
fn find_closing_brace(s: &str) -> Option<usize> {
    let mut depth = 0;
    let mut in_string = false;
    let mut escape_next = false;

    for (i, ch) in s.chars().enumerate() {
        if escape_next {
            escape_next = false;
            continue;
        }
        if ch == '\\' && in_string {
            escape_next = true;
            continue;
        }
        if ch == '"' {
            in_string = !in_string;
            continue;
        }
        if !in_string {
            match ch {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(i);
                    }
                }
                _ => {}
            }
        }
    }
    None
}

/// Attempt to auto-complete truncated JSON by closing open strings and brackets.
pub fn auto_complete_json(json: &str) -> String {
    let mut result = json.to_string();
    let mut in_string = false;
    let mut escape_next = false;
    let mut bracket_depth: usize = 0;
    let mut array_depth: usize = 0;

    for ch in result.chars() {
        if escape_next {
            escape_next = false;
            continue;
        }
        if ch == '\\' && in_string {
            escape_next = true;
            continue;
        }
        if ch == '"' {
            in_string = !in_string;
            continue;
        }
        if !in_string {
            match ch {
                '{' => bracket_depth += 1,
                '}' => bracket_depth = bracket_depth.saturating_sub(1),
                '[' => array_depth += 1,
                ']' => array_depth = array_depth.saturating_sub(1),
                _ => {}
            }
        }
    }

    if in_string {
        result.push('"');
    }
    for _ in 0..array_depth {
        result.push(']');
    }
    for _ in 0..bracket_depth {
        result.push('}');
    }

    result
}

/// Convenience: extract JSON, auto-complete if needed, and deserialize.
#[allow(dead_code)]
pub fn parse_json_with_fallback<T: for<'a> Deserialize<'a>>(text: &str) -> Result<T, String> {
    let json_str = extract_json(text).ok_or("No JSON found in response")?;
    serde_json::from_str::<T>(json_str)
        .map_err(|e| format!("JSON parse error: {}", e))
        .or_else(|_| {
            let completed = auto_complete_json(json_str);
            serde_json::from_str::<T>(&completed)
                .map_err(|e| format!("JSON parse error (auto-completed): {}", e))
        })
}

/// Stream + parse a typed response from the LLM.
#[allow(dead_code)]
pub async fn stream_and_parse<T>(
    emitter: &impl EventEmitter,
    config: LlmStreamConfig<'_>,
) -> Result<T, AppError>
where
    for<'a> T: Deserialize<'a>,
{
    let result = stream_llm_response(emitter, config).await?;
    parse_json_with_fallback::<T>(&result.content)
        .map_err(|e| AppError::LlmService(format!("Failed to parse response: {}", e)))
}
