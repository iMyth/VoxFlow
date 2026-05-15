use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::core::agent::llm_stream::extract_json;
use crate::core::error::AppError;
use crate::core::event_emitter::EventEmitter;

pub struct CharacterExtractionTool;

impl CharacterExtractionTool {
    pub fn name() -> &'static str {
        "character_extraction"
    }

    pub fn description() -> &'static str {
        "Extract all characters mentioned in a text segment. Returns a list of characters with their names, roles, and personality descriptions."
    }

    pub fn parameters_schema() -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "text": {
                    "type": "string",
                    "description": "The text to analyze for character extraction"
                }
            },
            "required": ["text"]
        })
    }
}

/// A character extracted from the text.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedCharacter {
    pub name: String,
    pub role: String,
    pub description: String,
}

/// Perform the actual character extraction HTTP call.
pub async fn do_character_extraction<E: EventEmitter>(
    emitter: &E,
    text: &str,
    api_endpoint: &str,
    api_key: &str,
    model: &str,
    enable_thinking: bool,
) -> Result<Vec<ExtractedCharacter>, AppError> {
    let system_prompt = "You are a character extraction assistant. Analyze the provided text and extract ALL characters mentioned (named or implied).\n\n\
        Return ONLY valid JSON (no markdown fences):\n\
        {\"characters\":[{\"name\":\"Character Name\",\"role\":\"protagonist/antagonist/narrator/supporting\",\"description\":\"Brief personality/role description\"}]}\n\n\
        CRITICAL: Return characters in the SAME LANGUAGE as the input text.";

    let url = format!("{}/chat/completions", api_endpoint.trim_end_matches('/'));

    let body = json!({
        "model": model,
        "messages": [
            { "role": "system", "content": system_prompt },
            { "role": "user", "content": text }
        ],
        "stream": true,
        "max_tokens": 4096,
        "enable_thinking": enable_thinking
    });

    let client = reqwest::Client::new();
    let response = client
        .post(&url)
        .header(CONTENT_TYPE, "application/json")
        .header(AUTHORIZATION, format!("Bearer {}", api_key))
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

    let content =
        crate::core::agent::llm_stream::collect_streaming_content(emitter, response).await?;

    emitter.emit_json("llm-complete", &json!(()));

    parse_characters(&content)
        .map_err(|e| AppError::LlmService(format!("Failed to parse characters: {}", e)))
}

fn parse_characters(text: &str) -> Result<Vec<ExtractedCharacter>, String> {
    #[derive(Deserialize)]
    struct CharactersResponse {
        characters: Vec<ExtractedCharacter>,
    }

    let json_str = extract_json(text).ok_or("No JSON found in response")?;
    let resp = serde_json::from_str::<CharactersResponse>(json_str)
        .map_err(|e| format!("JSON parse error: {}", e))?;
    Ok(resp.characters)
}
