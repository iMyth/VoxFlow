use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::core::agent::llm_stream::{collect_streaming_content, parse_json_with_fallback};
use crate::core::agent::AgentPlan;
use crate::core::error::AppError;
use crate::core::event_emitter::EventEmitter;
use crate::core::models::Character;

pub struct OutlineAnalysisTool;

impl OutlineAnalysisTool {
    pub fn name() -> &'static str {
        "outline_analysis"
    }

    pub fn description() -> &'static str {
        "Analyze the user's outline and return a structured audiobook plan with chapters, characters, mood, and style."
    }

    pub fn parameters_schema() -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "outline": {
                    "type": "string",
                    "description": "The user's outline text to analyze"
                },
                "existing_characters": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "List of existing character names in the project to match against"
                }
            },
            "required": ["outline"]
        })
    }
}

/// Suggested TTS configuration for a character.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TtsSuggestion {
    pub character_name: String,
    pub suggested_speed: f32,
    pub suggested_pitch: f32,
    pub reason: String,
}

/// Perform the actual outline analysis HTTP call with auto-retry on parse failure.
pub async fn do_outline_analysis<E: EventEmitter>(
    emitter: &E,
    outline: &str,
    characters: &[Character],
    api_endpoint: &str,
    api_key: &str,
    model: &str,
    enable_thinking: bool,
) -> Result<AgentPlan, AppError> {
    emitter.emit_json(
        "agent-tool-call",
        &json!({
            "tool": "outline_analysis",
            "outline": outline.chars().take(100).collect::<String>()
        }),
    );

    let existing_char_names: Vec<&str> = characters.iter().map(|c| c.name.as_str()).collect();
    let existing_chars_section = if existing_char_names.is_empty() {
        String::new()
    } else {
        format!("\nExisting project characters: {}\nTry to match suggested characters to these existing ones when possible.\n", existing_char_names.join(", "))
    };

    let system_prompt = format!(
        "You are an audiobook script planning assistant. Analyze the user's outline and return a structured plan.\n\n\
        CRITICAL LANGUAGE RULE: You MUST detect the language of the user's outline and respond entirely in that SAME language. \
        If the outline is in English, respond entirely in English. If in Chinese, respond entirely in Chinese. Match the user's language exactly.\n\n\
        {existing_chars}\
        Requirements:\n\
        1. Identify chapters/scenes, estimate line count per chapter (be generous — aim for 15-30+ lines per chapter for rich dialogue), list involved characters, describe mood\n\
        2. Extract all characters with their roles (protagonist, antagonist, narrator, etc.)\n\
        3. Check if characters match existing project characters\n\
        4. Summarize overall style\n\
        5. Provide character configuration notes\n\n\
        Return ONLY valid JSON (no markdown fences):\n\
        {{\"chapters\":[{{\"title\":\"...\",\"estimated_lines\":20,\"characters\":[\"...\"],\"mood\":\"...\"}}],\
        \"suggested_characters\":[{{\"name\":\"...\",\"role\":\"...\",\"matched_existing\":false,\"existing_id\":null}}],\
        \"overall_style\":\"...\",\"character_notes\":\"...\"}}",
        existing_chars = existing_chars_section
    );

    let url = format!("{}/chat/completions", api_endpoint.trim_end_matches('/'));

    let body = json!({
        "model": model,
        "messages": [
            { "role": "system", "content": system_prompt },
            { "role": "user", "content": outline }
        ],
        "stream": true,
        "max_tokens": 8192,
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

    let content = collect_streaming_content(emitter, response).await?;
    emitter.emit_json("llm-complete", &json!(()));

    // Parse with retry: if JSON fails, re-request with correction hint
    let plan = parse_outline_with_retry(
        emitter,
        &content,
        &system_prompt,
        outline,
        api_endpoint,
        api_key,
        model,
        enable_thinking,
    )
    .await?;

    // Emit TTS suggestions based on character roles
    emit_tts_suggestions(emitter, &plan.suggested_characters, &plan.character_notes);

    emitter.emit_json(
        "agent-tool-result",
        &json!({
            "tool": "outline_analysis",
            "results_count": plan.chapters.len()
        }),
    );

    Ok(plan)
}

/// Retry loop for parsing outline analysis JSON.
async fn parse_outline_with_retry<E: EventEmitter>(
    emitter: &E,
    initial_content: &str,
    system_prompt: &str,
    outline: &str,
    api_endpoint: &str,
    api_key: &str,
    model: &str,
    enable_thinking: bool,
) -> Result<AgentPlan, AppError> {
    let max_retries = 2;
    let mut attempt = 0;
    let mut parse_error = String::new();

    // Try parsing initial content
    if let Ok(plan) = parse_json_with_fallback::<AgentPlan>(initial_content) {
        return Ok(plan);
    }

    loop {
        attempt += 1;
        if attempt > max_retries {
            return Err(AppError::LlmService(format!(
                "Failed to parse outline after {} attempts: {}",
                attempt, parse_error
            )));
        }

        emitter.emit_json(
            "agent-retry",
            &json!({
                "attempt": attempt,
                "max_retries": max_retries,
                "reason": "JSON parse failed, retrying with correction"
            }),
        );

        let url = format!("{}/chat/completions", api_endpoint.trim_end_matches('/'));
        let body = json!({
            "model": model,
            "messages": [
                { "role": "system", "content": system_prompt },
                { "role": "user", "content": outline },
                { "role": "user", "content": format!(
                    "Your previous response could not be parsed as valid JSON. Error: {}\n\
                    Please return ONLY valid JSON with no markdown code fences, no surrounding text.",
                    parse_error
                )}
            ],
            "stream": true,
            "max_tokens": 8192,
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

        let content = collect_streaming_content(emitter, response).await?;
        emitter.emit_json("llm-complete", &json!(()));

        match parse_json_with_fallback::<AgentPlan>(&content) {
            Ok(plan) => return Ok(plan),
            Err(e) => parse_error = e,
        }
    }
}

/// Analyze character roles and emit TTS parameter suggestions.
fn emit_tts_suggestions<E: EventEmitter>(
    emitter: &E,
    characters: &[crate::core::agent::SuggestedCharacter],
    notes: &str,
) {
    let notes_lower = notes.to_lowercase();
    let suggestions: Vec<TtsSuggestion> = characters
        .iter()
        .map(|ch| {
            let role_lower = ch.role.to_lowercase();
            let (speed, pitch, reason) = match () {
                _ if role_lower.contains("child")
                    || role_lower.contains("young")
                    || role_lower.contains("小孩")
                    || role_lower.contains("少年") =>
                {
                    (
                        1.1,
                        1.2,
                        "Young/child character — faster pace, higher pitch",
                    )
                }
                _ if role_lower.contains("old")
                    || role_lower.contains("elder")
                    || role_lower.contains("老人")
                    || role_lower.contains("老年") =>
                {
                    (0.85, 0.8, "Elderly character — slower pace, lower pitch")
                }
                _ if role_lower.contains("antagonist")
                    || role_lower.contains("villain")
                    || role_lower.contains("反派") =>
                {
                    (
                        0.95,
                        0.85,
                        "Antagonist — slightly slower, deeper voice for dramatic effect",
                    )
                }
                _ if role_lower.contains("narrator")
                    || role_lower.contains("旁白")
                    || role_lower.contains("叙述") =>
                {
                    (1.0, 1.0, "Narrator — neutral, clear pacing")
                }
                _ if notes_lower.contains(&ch.name.to_lowercase()) => {
                    (1.0, 1.0, "Default — no specific voice traits detected")
                }
                _ => (1.0, 1.0, "Default pacing and pitch"),
            };
            TtsSuggestion {
                character_name: ch.name.clone(),
                suggested_speed: speed,
                suggested_pitch: pitch,
                reason: reason.to_string(),
            }
        })
        .collect();

    if !suggestions.is_empty() {
        emitter.emit_json(
            "agent-tts-suggestions",
            &json!({
                "suggestions": suggestions
            }),
        );
    }
}
