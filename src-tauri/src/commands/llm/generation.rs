use std::sync::Mutex;

use futures_util::StreamExt;
use serde_json::json;
use tauri::Emitter;

use crate::core::agent::AgentPlan;
use crate::core::cancel_token::CancellationToken;
use crate::core::db::Database;
use crate::core::error::AppError;
use crate::core::models::{Character, ScriptLine, ScriptSection};

use super::parser::parse_llm_json;
use super::utils::resolve_character;

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

    cancel_token.reset();

    // Emit tool-call event so UI shows this as a Skill invocation
    let _ = app.emit(
        "agent-tool-call",
        &json!({
            "tool": "script_generation",
            "query": outline.chars().take(100).collect::<String>()
        }),
    );

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
                    if let Some(reasoning) =
                        parsed["choices"][0]["delta"]["reasoning_content"].as_str()
                    {
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
        let msg = format!(
            "Failed to parse LLM JSON: {}, raw: {}",
            e,
            accumulated_text.chars().take(200).collect::<String>()
        );
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
        for line in section.lines.iter() {
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

    db.save_script(&project_id, &lines, &sections)
        .map_err(|e| {
            let msg = format!("Failed to save script: {}", e);
            let _ = app.emit("llm-error", &msg);
            e
        })?;

    // Emit tool-result event
    let _ = app.emit(
        "agent-tool-result",
        &json!({
            "tool": "script_generation",
            "results_count": sections.len()
        }),
    );

    Ok(())
}
