use std::sync::Mutex;

use futures_util::StreamExt;
use tauri::Emitter;

use crate::core::agent::{
    AgentPlan, SuggestedCharacter,
};
use crate::core::cancel_token::CancellationToken;
use crate::core::db::Database;
use crate::core::error::AppError;
use crate::core::models::{Character, LlmScriptLine, LlmScriptResponse, LlmSection, ScriptLine, ScriptSection};

/// Resolve a character name to its ID.
fn resolve_character(name: &Option<String>, characters: &[Character]) -> Option<String> {
    name.as_ref().and_then(|n| {
        characters
            .iter()
            .find(|c| c.name == *n)
            .map(|c| c.id.clone())
    })
}

/// Analyze outline and return a structured plan with chapters,
/// suggested characters, and style — WITHOUT generating script lines yet.
/// This is Phase 1 of the two-phase Agent workflow.
/// Streams tokens back to the frontend for real-time feedback.
#[tauri::command]
pub async fn analyze_outline(
    app: tauri::AppHandle,
    cancel_token: tauri::State<'_, CancellationToken>,
    outline: String,
    api_endpoint: String,
    api_key: String,
    model: String,
    characters: Vec<Character>,
    enable_thinking: bool,
) -> Result<AgentPlan, AppError> {
    use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
    use serde_json::json;

    cancel_token.reset();

    let url = format!("{}/chat/completions", api_endpoint.trim_end_matches('/'));

    let existing_char_names: Vec<&str> = characters.iter().map(|c| c.name.as_str()).collect();

    let existing_chars_section = if existing_char_names.is_empty() {
        String::new()
    } else {
        format!("\nExisting project characters: {}\nTry to match suggested characters to these existing ones when possible.\n", existing_char_names.join(", "))
    };

    let system_prompt = format!(
        "You are an audiobook script planning assistant. Analyze the user's outline and return a structured plan.\n\n\
        CRITICAL LANGUAGE RULE: You MUST detect the language of the user's outline and respond entirely in that SAME language. \
        If the outline is in English, ALL content (titles, descriptions, notes) must be in English. \
        If the outline is in Chinese, respond in Chinese. Match the user's language exactly.\n\n\
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
            let _ = app.emit("llm-error", &msg);
            AppError::LlmService(msg)
        })?;

    if !response.status().is_success() {
        let status = response.status();
        let body_text = response.text().await.unwrap_or_default();
        let msg = format!("LLM API error {}: {}", status, body_text);
        let _ = app.emit("llm-error", &msg);
        return Err(AppError::LlmService(msg));
    }

    // Read SSE stream chunk by chunk for real-time streaming
    let mut accumulated_text = String::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk_result) = stream.next().await {
        // Check cancellation
        if cancel_token.is_cancelled() {
            let _ = app.emit("llm-complete", &());
            let _ = app.emit("llm-cancel", &());
            return Err(AppError::LlmService("Cancelled".to_string()));
        }

        let chunk = chunk_result.map_err(|e| {
            let msg = format!("Failed to read LLM response: {}", e);
            let _ = app.emit("llm-error", &msg);
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
                    // Emit thinking/reasoning content
                    if let Some(reasoning) = parsed["choices"][0]["delta"]["reasoning_content"].as_str() {
                        let _ = app.emit("llm-thinking", reasoning);
                    }
                    // Emit normal content
                    if let Some(content) = parsed["choices"][0]["delta"]["content"].as_str() {
                        accumulated_text.push_str(content);
                        let _ = app.emit("llm-token", content);
                    }
                }
            }
        }
    }

    // Signal stream completion
    let _ = app.emit("llm-complete", &());

    let plan: AgentPlan = parse_agent_plan(&accumulated_text)
        .map_err(|e| {
            let msg = format!("Failed to parse plan: {}\nRaw: {}", e, accumulated_text.chars().take(300).collect::<String>());
            let _ = app.emit("llm-error", &msg);
            AppError::LlmService(msg)
        })?;

    // Enrich: match suggested characters with existing project characters
    let char_map: std::collections::HashMap<String, &Character> = characters
        .iter()
        .map(|c| (c.name.clone(), c))
        .collect();

    let enriched_chars: Vec<SuggestedCharacter> = plan
        .suggested_characters
        .into_iter()
        .map(|mut sc| {
            if let Some(existing) = char_map.get(&sc.name) {
                sc.matched_existing = true;
                sc.existing_id = Some(existing.id.clone());
            }
            sc
        })
        .collect();

    Ok(AgentPlan {
        chapters: plan.chapters,
        suggested_characters: enriched_chars,
        overall_style: plan.overall_style,
        character_notes: plan.character_notes,
    })
}

/// Parse LLM response into AgentPlan.
fn parse_agent_plan(text: &str) -> Result<AgentPlan, String> {
    let trimmed = text.trim();

    let json_str = if trimmed.starts_with("```") {
        if let Some(first_newline) = trimmed.find('\n') {
            let after_fence = &trimmed[first_newline + 1..];
            after_fence
                .trim()
                .strip_suffix("```")
                .unwrap_or(after_fence.trim())
                .to_string()
        } else {
            trimmed.trim().to_string()
        }
    } else {
        trimmed.to_string()
    };

    serde_json::from_str::<AgentPlan>(&json_str).map_err(|e| e.to_string())
}

/// Generate script from a confirmed plan. This is Phase 2.
/// Characters are now REQUIRED — the LLM must assign every line to an existing or new character.
#[tauri::command]
pub async fn generate_script(
    app: tauri::AppHandle,
    db: tauri::State<'_, Mutex<Database>>,
    cancel_token: tauri::State<'_, CancellationToken>,
    project_id: String,
    outline: String,
    api_endpoint: String,
    api_key: String,
    model: String,
    characters: Vec<Character>,
    agent_plan: Option<AgentPlan>,
    extra_instructions: Option<String>,
    enable_thinking: bool,
) -> Result<(), AppError> {
    use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
    use serde_json::json;

    cancel_token.reset();

    let url = format!("{}/chat/completions", api_endpoint.trim_end_matches('/'));

    let char_list = if characters.is_empty() {
        "(No characters defined yet — use character names freely, the system will create them automatically)".to_string()
    } else {
        let names: Vec<&str> = characters.iter().map(|c| c.name.as_str()).collect();
        format!("Available characters: {}", names.join(", "))
    };

    // Build chapter reference info from the plan (as guidance, not hard requirement)
    let chapter_info = agent_plan.as_ref().map(|p| {
        let ch_descs: Vec<String> = p.chapters.iter().map(|ch| {
            format!(
                "- \"{}\" ~{} lines, mood: {}, characters: {}",
                ch.title,
                ch.estimated_lines,
                ch.mood,
                if ch.characters.is_empty() { "unspecified".to_string() } else { ch.characters.join(", ") }
            )
        }).collect();
        let total_estimated: u32 = p.chapters.iter().map(|ch| ch.estimated_lines).sum();
        format!(
            "CHAPTER PLAN (reference — adapt freely based on story needs):\n{}\nTotal estimated lines: {}\n\
            IMPORTANT: Generate AT LEAST this many lines total. Each chapter should have rich, detailed dialogue. \
            Do NOT cut short or summarize — fully develop every scene with natural conversation flow.",
            ch_descs.join("\n"),
            total_estimated
        )
    }).unwrap_or_default();

    let extra = extra_instructions
        .filter(|s| !s.trim().is_empty())
        .map(|s| format!("ADDITIONAL USER INSTRUCTIONS: {}\n", s))
        .unwrap_or_default();

    let system_prompt = format!(
        "You are an audiobook script writer. Generate a complete, detailed audiobook script from the user's outline.\n\n\
        CRITICAL LANGUAGE RULE: You MUST detect the language of the user's outline and write ALL dialogue/content in that SAME language. \
        If the outline is in English, write English dialogue. If in Chinese, write Chinese dialogue. \
        Only the JSON keys remain in English.\n\n\
        {extra}\
        {char_list}\n\n\
        {chapter_info}\n\n\
        OUTPUT FORMAT — return ONLY valid JSON (no markdown fences, no extra text):\n\
        {{\"sections\":[\
        {{\"title\":\"Section Title\",\"lines\":[\
        {{\"text\":\"dialogue content\",\"character\":\"character name\",\"instructions\":\"emotion/pace direction or null\",\"gap_ms\":500}},\
        ...\
        ]}},\
        ...\
        ]}}\n\n\
        RULES:\n\
        1. \"character\" is REQUIRED for every line — assign a character to each line\n\
        2. \"instructions\" describes voice direction (emotion, pace, tone). Use null if unsure\n\
        3. \"gap_ms\" is pause duration in ms after the line (500-2000, default 500)\n\
        4. Each line must be a complete, meaningful sentence that advances the story\n\
        5. DO NOT use ellipsis (\"...\"/\"……\") as filler or padding\n\
        6. DO NOT use placeholders like \"(omitted)\" or \"(continues)\"\n\
        7. DO NOT summarize or abbreviate — write out every line of dialogue fully\n\
        8. Generate RICH, DETAILED scripts — aim for at least 15-30 lines per section\n\
        9. Develop each scene thoroughly: include greetings, reactions, transitions, emotional beats\n\
        10. If the outline describes a long story, generate proportionally more content\n\
        11. Organize into 3-5 sections (e.g. \"Intro\", \"Act 1\", \"Act 2\", \"Climax\", \"Outro\" or localized equivalents)",
        extra = extra,
        char_list = char_list,
        chapter_info = chapter_info
    );

    let body = json!({
        "model": model,
        "messages": [
            { "role": "system", "content": system_prompt },
            { "role": "user", "content": outline }
        ],
        "stream": true,
        "max_tokens": 16384,
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
            let _ = app.emit("llm-error", &msg);
            AppError::LlmService(msg)
        })?;

    if !response.status().is_success() {
        let status = response.status();
        let body_text = response.text().await.unwrap_or_default();
        let msg = format!("LLM API error {}: {}", status, body_text);
        let _ = app.emit("llm-error", &msg);
        return Err(AppError::LlmService(msg));
    }

    // Read SSE stream chunk by chunk for real-time streaming
    let mut accumulated_text = String::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk_result) = stream.next().await {
        // Check cancellation
        if cancel_token.is_cancelled() {
            let _ = app.emit("llm-complete", &());
            let _ = app.emit("llm-cancel", &());
            return Ok(());
        }

        let chunk = chunk_result.map_err(|e| {
            let msg = format!("Failed to read LLM response: {}", e);
            let _ = app.emit("llm-error", &msg);
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
                    // Emit thinking/reasoning content
                    if let Some(reasoning) = parsed["choices"][0]["delta"]["reasoning_content"].as_str() {
                        let _ = app.emit("llm-thinking", reasoning);
                    }
                    // Emit normal content
                    if let Some(content) = parsed["choices"][0]["delta"]["content"].as_str() {
                        accumulated_text.push_str(content);
                        let _ = app.emit("llm-token", content);
                    }
                }
            }
        }
    }

    // Signal stream completion
    let _ = app.emit("llm-complete", &());

    // Parse JSON response from LLM
    let llm_response = parse_llm_json(&accumulated_text).map_err(|e| {
        let msg = format!("Failed to parse LLM JSON: {}, raw: {}", e, accumulated_text.chars().take(200).collect::<String>());
        let _ = app.emit("llm-error", &msg);
        AppError::LlmService(msg)
    })?;

    // Delete old sections and lines, save fresh LLM output directly
    let db = db.lock().map_err(|e| {
        let msg = format!("Database lock failed: {}", e);
        let _ = app.emit("llm-error", &msg);
        AppError::Database(msg)
    })?;
    db.delete_sections(&project_id).map_err(|e| {
        let msg = format!("Failed to delete old sections: {}", e);
        let _ = app.emit("llm-error", &msg);
        AppError::Database(msg)
    })?;

    // Convert LLM sections to ScriptSections and ScriptLines
    let mut sections: Vec<ScriptSection> = Vec::new();
    let mut lines: Vec<ScriptLine> = Vec::new();
    for (i, section) in llm_response.sections.iter().enumerate() {
        let section_id = uuid::Uuid::new_v4().to_string();
        sections.push(ScriptSection {
            id: section_id.clone(),
            project_id: project_id.clone(),
            title: section.title.clone(),
            section_order: i as i32,
        });
        for (_j, line) in section.lines.iter().enumerate() {
            let text = line.text.trim();
            if text.is_empty() {
                continue;
            }
            lines.push(ScriptLine {
                id: uuid::Uuid::new_v4().to_string(),
                project_id: project_id.clone(),
                line_order: lines.len() as i32,
                text: text.to_string(),
                character_id: resolve_character(&line.character, &characters),
                gap_after_ms: line.gap_ms.unwrap_or(500) as i32,
                instructions: line.instructions.clone().unwrap_or_default(),
                section_id: Some(section_id.clone()),
            });
        }
    }

    // If LLM returned no sections, flatten to lines without section_id
    if llm_response.sections.is_empty() {
        let flat_lines: Vec<ScriptLine> = Vec::new();
        db.save_script(&project_id, &flat_lines, &[]).map_err(|e| {
            let msg = format!("Failed to save script: {}", e);
            let _ = app.emit("llm-error", &msg);
            e
        })?;
    }

    db.save_script(&project_id, &lines, &sections).map_err(|e| {
        let msg = format!("Failed to save script: {}", e);
        let _ = app.emit("llm-error", &msg);
        e
    })?;

    Ok(())
}

/// Auto-complete truncated JSON by appending missing closing delimiters.
fn auto_complete_json(json: &str) -> String {
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

    // Close any open string
    if in_string {
        result.push('"');
    }
    // Close arrays
    for _ in 0..array_depth {
        result.push(']');
    }
    // Close objects
    for _ in 0..bracket_depth {
        result.push('}');
    }

    result
}

/// Parse LLM response text as JSON LlmScriptResponse.
/// Strips markdown code block fences if present.
/// Backward compatible: accepts both new `{"sections":[...]}` format
/// and old `{"lines":[...]}` format (wraps lines in a default "正文" section).
/// Handles truncated JSON from stream cutoff.
fn parse_llm_json(text: &str) -> Result<LlmScriptResponse, String> {
    let trimmed = text.trim();

    // Strip markdown code block: ```json ... ``` or ``` ... ```
    let json_str = if trimmed.starts_with("```") {
        if let Some(first_newline) = trimmed.find('\n') {
            let after_fence = &trimmed[first_newline + 1..];
            after_fence
                .trim()
                .strip_suffix("```")
                .unwrap_or(after_fence.trim())
                .to_string()
        } else {
            trimmed
                .trim()
                .strip_prefix("```")
                .and_then(|s| s.strip_suffix("```"))
                .unwrap_or(trimmed)
                .to_string()
        }
    } else {
        trimmed.to_string()
    };

    // Try new sections format first
    if let Ok(resp) = serde_json::from_str::<LlmScriptResponse>(&json_str) {
        return Ok(resp);
    }

    // Try auto-completing truncated JSON
    let completed = auto_complete_json(&json_str);
    if let Ok(resp) = serde_json::from_str::<LlmScriptResponse>(&completed) {
        return Ok(resp);
    }

    // Fallback: try old lines format and wrap in default section
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(&json_str) {
        if let Some(lines_array) = value.get("lines").and_then(|v| v.as_array()) {
            let lines: Vec<LlmScriptLine> = lines_array
                .iter()
                .filter_map(|l| serde_json::from_value::<LlmScriptLine>(l.clone()).ok())
                .collect();
            if !lines.is_empty() {
                return Ok(LlmScriptResponse {
                    sections: vec![LlmSection {
                        title: "正文".to_string(),
                        lines,
                    }],
                });
            }
        }
    }

    // Try old lines format with auto-completed JSON
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(&completed) {
        if let Some(lines_array) = value.get("lines").and_then(|v| v.as_array()) {
            let lines: Vec<LlmScriptLine> = lines_array
                .iter()
                .filter_map(|l| serde_json::from_value::<LlmScriptLine>(l.clone()).ok())
                .collect();
            if !lines.is_empty() {
                return Ok(LlmScriptResponse {
                    sections: vec![LlmSection {
                        title: "正文".to_string(),
                        lines,
                    }],
                });
            }
        }
    }

    Err(format!("Cannot parse as sections or lines format, raw: {}", json_str.chars().take(500).collect::<String>()))
}

/// Cancel an ongoing LLM request (analyze_outline or generate_script).
#[tauri::command]
pub fn cancel_llm(
    cancel_token: tauri::State<'_, CancellationToken>,
) {
    cancel_token.cancel();
}

#[tauri::command]
pub fn save_script(
    db: tauri::State<'_, Mutex<Database>>,
    project_id: String,
    lines: Vec<ScriptLine>,
    sections: Vec<ScriptSection>,
) -> Result<(), AppError> {
    let db = db.lock().map_err(|e| AppError::Database(e.to_string()))?;
    db.save_script(&project_id, &lines, &sections)
}

#[tauri::command]
pub fn load_script(
    db: tauri::State<'_, Mutex<Database>>,
    project_id: String,
) -> Result<Vec<ScriptLine>, AppError> {
    let db = db.lock().map_err(|e| AppError::Database(e.to_string()))?;
    db.load_script(&project_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_char(id: &str, name: &str) -> Character {
        Character {
            id: id.to_string(),
            project_id: "p1".to_string(),
            name: name.to_string(),
            voice_name: "voice".to_string(),
            tts_model: "model".to_string(),
            speed: 1.0,
            pitch: 1.0,
        }
    }

    // ---- JSON parsing tests ----

    #[test]
    fn test_parse_llm_json_basic() {
        let text = r#"{"sections":[{"title":"第一幕","lines":[{"text":"台词1","character":null},{"text":"台词2","character":"Alice"}]}]}"#;
        let resp = parse_llm_json(text).unwrap();
        assert_eq!(resp.sections.len(), 1);
        assert_eq!(resp.sections[0].title, "第一幕");
        assert_eq!(resp.sections[0].lines.len(), 2);
        assert_eq!(resp.sections[0].lines[0].text, "台词1");
        assert!(resp.sections[0].lines[0].character.is_none());
        assert_eq!(resp.sections[0].lines[1].text, "台词2");
        assert_eq!(resp.sections[0].lines[1].character, Some("Alice".to_string()));
    }

    #[test]
    fn test_parse_llm_json_strips_markdown_fence() {
        let text = "```json\n{\"sections\":[{\"title\":\"开场\",\"lines\":[{\"text\":\"hello\",\"character\":null}]}]}\n```";
        let resp = parse_llm_json(text).unwrap();
        assert_eq!(resp.sections.len(), 1);
        assert_eq!(resp.sections[0].lines.len(), 1);
        assert_eq!(resp.sections[0].lines[0].text, "hello");
    }

    #[test]
    fn test_parse_llm_json_old_format_fallback() {
        // Old format without sections should be wrapped in a default "正文" section
        let text = r#"{"lines":[{"text":"台词1","character":null},{"text":"台词2","character":"Alice"}]}"#;
        let resp = parse_llm_json(text).unwrap();
        assert_eq!(resp.sections.len(), 1);
        assert_eq!(resp.sections[0].title, "正文");
        assert_eq!(resp.sections[0].lines.len(), 2);
        assert_eq!(resp.sections[0].lines[0].text, "台词1");
    }

    #[test]
    fn test_parse_llm_json_invalid() {
        let result = parse_llm_json("not json at all");
        assert!(result.is_err());
    }

    // ---- resolve_character tests ----

    #[test]
    fn test_resolve_character_found() {
        let chars = vec![make_char("id-1", "Alice"), make_char("id-2", "Bob")];
        assert_eq!(
            resolve_character(&Some("Bob".to_string()), &chars),
            Some("id-2".to_string())
        );
    }

    #[test]
    fn test_resolve_character_not_found() {
        let chars = vec![make_char("id-1", "Alice")];
        assert_eq!(
            resolve_character(&Some("Charlie".to_string()), &chars),
            None
        );
    }

    #[test]
    fn test_resolve_character_none() {
        let chars = vec![make_char("id-1", "Alice")];
        assert_eq!(resolve_character(&None, &chars), None);
    }
}
