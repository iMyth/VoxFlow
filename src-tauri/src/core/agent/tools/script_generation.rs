use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::core::agent::llm_stream::{collect_streaming_content, extract_json};
use crate::core::db::Database;
use crate::core::error::AppError;
use crate::core::event_emitter::EventEmitter;
use crate::core::models::{Character, ScriptLine, ScriptSection};

pub struct ScriptGenerationTool;

impl ScriptGenerationTool {
    pub fn name() -> &'static str {
        "script_generation"
    }

    pub fn description() -> &'static str {
        "Generate the complete audiobook script from the outline. Creates sections with dialogue lines for each character. Saves to the project database."
    }

    pub fn parameters_schema() -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "outline": {
                    "type": "string",
                    "description": "The user's outline to generate a script from"
                },
                "extra_instructions": {
                    "type": "string",
                    "description": "Additional instructions for script generation (optional)"
                }
            },
            "required": ["outline"]
        })
    }
}

/// A line of generated script from the LLM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedLine {
    pub text: String,
    pub character: String,
    pub gap_after_ms: i32,
}

/// A section with its generated lines.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedSection {
    pub title: String,
    pub lines: Vec<GeneratedLine>,
}

/// Full script generation response from the LLM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScriptGenerationResponse {
    pub sections: Vec<GeneratedSection>,
    pub narration_style: String,
}

/// Result of script validation.
#[derive(Debug, Serialize)]
pub struct ScriptValidation {
    pub total_lines: usize,
    pub total_sections: usize,
    pub unmatched_characters: Vec<String>,
    pub section_stats: Vec<SectionStat>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct SectionStat {
    pub title: String,
    pub line_count: usize,
    pub characters: Vec<String>,
}

/// Perform the actual script generation with auto-retry, per-section streaming,
/// save to database, and validation report.
pub async fn do_script_generation<E: EventEmitter>(
    emitter: &E,
    db: &std::sync::Mutex<Database>,
    project_id: &str,
    outline: &str,
    characters: &[Character],
    api_endpoint: &str,
    api_key: &str,
    model: &str,
    plan: Option<&crate::core::agent::AgentPlan>,
    extra_instructions: Option<&str>,
    enable_thinking: bool,
) -> Result<ScriptGenerationResponse, AppError> {
    emitter.emit_json(
        "agent-tool-call",
        &json!({
            "tool": "script_generation",
            "outline": outline.chars().take(100).collect::<String>()
        }),
    );

    emitter.emit_json(
        "agent-step",
        &json!({
            "step": "script_generation",
            "status": "started"
        }),
    );

    let char_list: Vec<&str> = characters.iter().map(|c| c.name.as_str()).collect();
    let chars_section = if char_list.is_empty() {
        String::new()
    } else {
        format!("\nAvailable characters: {}\n", char_list.join(", "))
    };

    let plan_context = plan
        .map(|p| {
            let ch_descs: Vec<String> = p
                .chapters
                .iter()
                .map(|ch| {
                    format!(
                        "- \"{}\" (~{} lines, mood: {})",
                        ch.title, ch.estimated_lines, ch.mood
                    )
                })
                .collect();
            format!("\nPlan:\n{}\n", ch_descs.join("\n"))
        })
        .unwrap_or_default();

    let extra = extra_instructions
        .filter(|s| !s.trim().is_empty())
        .map(|s| format!("\nAdditional instructions: {}\n", s))
        .unwrap_or_default();

    let system_prompt = format!(
        "You are an expert audiobook script writer. Take the user's outline and generate a full script with dialogue.\n\n\
        CRITICAL LANGUAGE RULE: You MUST detect the language of the user's outline and write the entire script in that SAME language.\n\n\
        CRITICAL CHARACTER RULE: Only use characters from this list: {chars}.\
        {plan}\
        {extra}\
        Rules:\n\
        1. Write natural, engaging dialogue for each character\n\
        2. Include narrator lines when needed\n\
        3. Use realistic character voices matching their personality\n\
        4. Add appropriate gaps between lines for pacing\n\
        5. Keep the script flowing and immersive\n\n\
        Return ONLY valid JSON (no markdown fences):\n\
        {{\"sections\":[{{\"title\":\"Chapter 1\",\"lines\":[{{\"text\":\"Dialogue here\",\"character\":\"Character Name\",\"gap_after_ms\":500}}]}},\"narration_style\":\"descriptive\"}}",
        chars = chars_section,
        plan = plan_context,
        extra = extra
    );

    // Stream + parse with retry
    let script_resp = stream_and_parse_script(
        emitter,
        &system_prompt,
        outline,
        api_endpoint,
        api_key,
        model,
        enable_thinking,
    )
    .await?;

    // Convert to DB model and save, emitting per-section events
    let mut all_lines = Vec::new();
    let mut sections = Vec::new();
    let mut line_order = 0;

    for (section_idx, section) in script_resp.sections.iter().enumerate() {
        let section_id = format!("sec_{}", section_idx + 1);
        sections.push(ScriptSection {
            id: section_id.clone(),
            project_id: project_id.to_string(),
            title: section.title.clone(),
            section_order: section_idx as i32,
        });

        let mut section_lines = Vec::new();
        let mut section_chars: Vec<String> = Vec::new();

        for gen_line in &section.lines {
            let character_id = characters
                .iter()
                .find(|c| c.name == gen_line.character)
                .map(|c| c.id.clone());

            if !section_chars.contains(&gen_line.character) {
                section_chars.push(gen_line.character.clone());
            }

            section_lines.push(ScriptLine {
                id: format!("line_{}", line_order),
                project_id: project_id.to_string(),
                line_order,
                text: gen_line.text.clone(),
                character_id,
                gap_after_ms: gen_line.gap_after_ms,
                instructions: String::new(),
                section_id: Some(section_id.clone()),
            });
            line_order += 1;
        }

        all_lines.extend(section_lines);

        // Emit per-section event so frontend can display in real-time
        emitter.emit_json(
            "agent-section-generated",
            &json!({
                "section_index": section_idx,
                "section_id": section_id,
                "title": section.title,
                "line_count": section.lines.len(),
                "characters": section_chars,
            }),
        );
    }

    let db_lock = db
        .lock()
        .map_err(|e| AppError::LlmService(format!("Database lock poisoned: {}", e)))?;
    db_lock
        .save_script(project_id, &all_lines, &sections)
        .map_err(|e| AppError::LlmService(format!("Failed to save script: {}", e)))?;

    // Validate and emit report
    let validation = validate_script(&script_resp.sections, characters);
    emitter.emit_json("agent-validation", &json!(validation));

    emitter.emit_json(
        "agent-step",
        &json!({
            "step": "script_generation",
            "status": "completed",
            "total_lines": validation.total_lines,
            "total_sections": validation.total_sections
        }),
    );

    Ok(script_resp)
}

/// Stream LLM response and parse as ScriptGenerationResponse, with auto-retry on failure.
async fn stream_and_parse_script<E: EventEmitter>(
    emitter: &E,
    system_prompt: &str,
    outline: &str,
    api_endpoint: &str,
    api_key: &str,
    model: &str,
    enable_thinking: bool,
) -> Result<ScriptGenerationResponse, AppError> {
    let max_retries = 2;
    let mut attempt = 0;
    let mut parse_error = String::new();

    loop {
        attempt += 1;

        let mut messages = vec![
            json!({ "role": "system", "content": system_prompt }),
            json!({ "role": "user", "content": outline }),
        ];

        // On retry, add correction message
        if attempt > 1 {
            emitter.emit_json(
                "agent-retry",
                &json!({
                    "attempt": attempt,
                    "max_retries": max_retries + 1,
                    "reason": "JSON parse failed, retrying with correction"
                }),
            );

            messages.push(json!({
                "role": "user",
                "content": format!(
                    "Your previous response could not be parsed as valid JSON. Error: {}\n\
                    Please return ONLY valid JSON with no markdown code fences, no surrounding text.",
                    parse_error
                )
            }));
        }

        let url = format!("{}/chat/completions", api_endpoint.trim_end_matches('/'));
        let body = json!({
            "model": model,
            "messages": messages,
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

        match parse_script_response(&content) {
            Ok(resp) => return Ok(resp),
            Err(e) => {
                if attempt > max_retries {
                    return Err(AppError::LlmService(format!(
                        "Failed to parse script after {} attempts: {}",
                        attempt, e
                    )));
                }
                parse_error = e;
            }
        }
    }
}

/// Validate the generated script and return a report.
fn validate_script(sections: &[GeneratedSection], characters: &[Character]) -> ScriptValidation {
    let mut unmatched: Vec<String> = Vec::new();
    let mut section_stats = Vec::new();
    let mut warnings: Vec<String> = Vec::new();
    let total_lines: usize = sections.iter().map(|s| s.lines.len()).sum();

    for section in sections {
        let mut chars_in_section: Vec<String> = Vec::new();
        for line in &section.lines {
            if !characters.iter().any(|c| c.name == line.character)
                && !unmatched.contains(&line.character)
            {
                unmatched.push(line.character.clone());
            }
            if !chars_in_section.contains(&line.character) {
                chars_in_section.push(line.character.clone());
            }
        }

        section_stats.push(SectionStat {
            title: section.title.clone(),
            line_count: section.lines.len(),
            characters: chars_in_section,
        });

        if section.lines.is_empty() {
            warnings.push(format!("Section '{}' has no lines", section.title));
        }
        if section.lines.len() < 3 {
            warnings.push(format!(
                "Section '{}' has only {} lines — may be too short",
                section.title,
                section.lines.len()
            ));
        }
    }

    if total_lines == 0 {
        warnings.push("Script has no lines at all".to_string());
    }
    if sections.is_empty() {
        warnings.push("Script has no sections at all".to_string());
    }
    if !unmatched.is_empty() {
        warnings.push(format!(
            "{} characters not matched to existing: {}",
            unmatched.len(),
            unmatched.join(", ")
        ));
    }

    ScriptValidation {
        total_lines,
        total_sections: sections.len(),
        unmatched_characters: unmatched,
        section_stats,
        warnings,
    }
}

fn parse_script_response(text: &str) -> Result<ScriptGenerationResponse, String> {
    let json_str = extract_json(text).ok_or("No JSON found in response")?;
    serde_json::from_str::<ScriptGenerationResponse>(json_str)
        .map_err(|e| format!("JSON parse error: {}", e))
}
