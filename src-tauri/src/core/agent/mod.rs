pub mod tools;
mod llm_stream;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Agent plan phase: analysis result before generation.
/// Returned to frontend for user review and confirmation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentPlan {
    pub chapters: Vec<ChapterPlan>,
    pub suggested_characters: Vec<SuggestedCharacter>,
    pub overall_style: String,
    pub character_notes: String,
}

/// A single chapter/scene detected from the outline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChapterPlan {
    pub title: String,
    pub estimated_lines: u32,
    pub characters: Vec<String>,
    pub mood: String,
}

/// A character suggested by the LLM during analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuggestedCharacter {
    pub name: String,
    pub role: String,
    pub matched_existing: bool,
    pub existing_id: Option<String>,
}

/// User's response to the plan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanConfirmation {
    pub confirmed: bool,
    pub character_mapping: HashMap<String, String>,
    pub new_characters: Vec<NewCharacterInput>,
    pub extra_instructions: String,
}

/// Input for creating a new character during plan confirmation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewCharacterInput {
    pub name: String,
    pub voice_name: String,
    pub tts_model: String,
    pub speed: f32,
    pub pitch: f32,
}

/// Summary generated for a chapter/section.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChapterSummary {
    pub title: String,
    pub summary: String,
}

/// Revision request: regenerate specific sections with new instructions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RevisionRequest {
    pub instructions: String,
    pub section_indices: Option<Vec<usize>>,
}

// ============================================================
// VoxAgent — agent pipeline (works with any EventEmitter)
// ============================================================

use std::sync::Mutex;

use futures_util::StreamExt;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use serde_json::json;

use crate::core::cancel_token::CancellationToken;
use crate::core::db::Database;
use crate::core::error::AppError;
use crate::core::event_emitter::{EventEmitter, EmitExt};
use crate::core::models::Character;
use crate::core::agent::tools::outline_analysis::do_outline_analysis;
use crate::core::agent::tools::script_generation::do_script_generation;
use crate::core::agent::tools::story_recall::{do_story_recall, StoryRecallArgs};
use crate::core::agent::tools::story_memory::{do_story_memory, StoryMemoryArgs};

/// Configuration for the agent pipeline.
pub struct AgentConfig {
    pub api_endpoint: String,
    pub api_key: String,
    pub model: String,
    pub enable_thinking: bool,
    pub project_id: String,
}

// ---- Step 1: Analysis (returns plan, does NOT generate) ----

/// Run only the analysis step. Returns the plan for user review.
/// This is the first step in the two-phase flow.
pub async fn run_analysis_step<E: EventEmitter>(
    emitter: &E,
    cancel_token: &CancellationToken,
    config: &AgentConfig,
    outline: &str,
    characters: &[Character],
) -> Result<AgentPlan, AppError> {
    cancel_token.reset();

    emitter.emit("agent-pipeline-started", &json!({
        "project_id": config.project_id,
        "phase": "analysis"
    }));

    let plan = do_outline_analysis(
        emitter,
        outline,
        characters,
        &config.api_endpoint,
        &config.api_key,
        &config.model,
        config.enable_thinking,
    ).await?;

    emitter.emit("agent-step-complete", &json!({
        "phase": "analysis",
        "plan": plan,
        "project_id": config.project_id
    }));

    Ok(plan)
}

// ---- Step 2: Generation (takes confirmed plan, generates script) ----

/// Run the generation step. Takes a confirmed plan (or None) and generates + saves the script.
pub async fn run_generation_step<E: EventEmitter>(
    emitter: &E,
    cancel_token: &CancellationToken,
    db: &Mutex<Database>,
    config: &AgentConfig,
    outline: &str,
    characters: &[Character],
    plan: Option<&AgentPlan>,
    extra_instructions: Option<&str>,
) -> Result<(), AppError> {
    cancel_token.reset();

    emitter.emit("agent-pipeline-started", &json!({
        "project_id": config.project_id,
        "phase": "generation"
    }));

    do_script_generation(
        emitter,
        db,
        &config.project_id,
        outline,
        characters,
        &config.api_endpoint,
        &config.api_key,
        &config.model,
        plan,
        extra_instructions,
        config.enable_thinking,
    ).await?;

    emitter.emit("agent-step-complete", &json!({
        "phase": "generation",
        "project_id": config.project_id
    }));

    Ok(())
}

// ---- Step 3: Revision (regenerate specific sections) ----

/// Revise specific sections of an existing script.
pub async fn run_revision_step<E: EventEmitter>(
    emitter: &E,
    cancel_token: &CancellationToken,
    db: &Mutex<Database>,
    config: &AgentConfig,
    outline: &str,
    characters: &[Character],
    revision: &RevisionRequest,
    existing_plan: Option<&AgentPlan>,
) -> Result<(), AppError> {
    cancel_token.reset();

    emitter.emit("agent-pipeline-started", &json!({
        "project_id": config.project_id,
        "phase": "revision",
        "sections": revision.section_indices
    }));

    let sections_hint = if let Some(indices) = &revision.section_indices {
        if let Some(plan) = existing_plan {
            let hints: Vec<String> = indices.iter()
                .filter_map(|&i| plan.chapters.get(i).map(|ch| &ch.title))
                .map(|t| format!("- \"{}\"", t))
                .collect();
            format!("\nRevise these sections:\n{}\n", hints.join("\n"))
        } else {
            format!("\nRevise sections at indices: {:?}\n", indices)
        }
    } else {
        String::new()
    };

    let extra = format!(
        "REVISION REQUEST: {}\n\
        {}\
        Only modify the requested sections. Keep everything else unchanged.",
        revision.instructions, sections_hint
    );

    do_script_generation(
        emitter,
        db,
        &config.project_id,
        outline,
        characters,
        &config.api_endpoint,
        &config.api_key,
        &config.model,
        existing_plan,
        Some(&extra),
        config.enable_thinking,
    ).await?;

    emitter.emit("agent-step-complete", &json!({
        "phase": "revision",
        "project_id": config.project_id
    }));

    Ok(())
}

// ---- Legacy: Full pipeline (analysis → generation in one shot) ----

/// Run the full agent pipeline: analyze outline → generate script.
/// This is the one-shot entry point for backward compatibility.
#[deprecated(since = "0.1.5", note = "Use individual step functions (run_analysis_step, run_generation_step, run_revision_step) instead")]
pub async fn run_agent_pipeline<E: EventEmitter>(
    emitter: &E,
    cancel_token: &CancellationToken,
    db: &Mutex<Database>,
    config: &AgentConfig,
    outline: &str,
    characters: Vec<Character>,
    agent_plan: Option<&AgentPlan>,
    extra_instructions: Option<&str>,
) -> Result<(), AppError> {
    cancel_token.reset();

    emitter.emit("agent-pipeline-started", &json!({
        "project_id": config.project_id
    }));

    let system_prompt = build_agent_system_prompt(
        characters.as_slice(),
        agent_plan,
        extra_instructions,
    );

    let mut messages = build_initial_messages(&system_prompt, outline);

    let result = agent_loop(
        emitter,
        cancel_token,
        config,
        &mut messages,
        &characters,
        db,
    ).await;

    if result.is_ok() {
        emitter.emit("agent-pipeline-complete", &json!({
            "project_id": config.project_id,
            "success": true
        }));
    } else if let Err(ref e) = result {
        emitter.emit("agent-pipeline-complete", &json!({
            "project_id": config.project_id,
            "success": false,
            "error": e.to_string()
        }));
    }

    result
}

/// Build the system prompt for the agent.
fn build_agent_system_prompt(
    characters: &[Character],
    agent_plan: Option<&AgentPlan>,
    extra_instructions: Option<&str>,
) -> String {
    let char_list = if characters.is_empty() {
        "(No characters defined yet — use character names freely)".to_string()
    } else {
        let names: Vec<&str> = characters.iter().map(|c| c.name.as_str()).collect();
        format!("Available characters: {}", names.join(", "))
    };

    let plan_context = agent_plan.map(|p| {
        let ch_descs: Vec<String> = p.chapters.iter().map(|ch| {
            format!(
                "- \"{}\" ~{} lines, mood: {}, characters: {}",
                ch.title, ch.estimated_lines, ch.mood,
                if ch.characters.is_empty() { "unspecified".to_string() } else { ch.characters.join(", ") }
            )
        }).collect();
        format!("Confirmed plan:\n{}\n\nTotal estimated lines: {}", ch_descs.join("\n"), p.chapters.iter().map(|ch| ch.estimated_lines).sum::<u32>())
    }).unwrap_or_default();

    let extra = extra_instructions
        .filter(|s| !s.trim().is_empty())
        .map(|s| format!("ADDITIONAL INSTRUCTIONS: {}\n", s))
        .unwrap_or_default();

    format!(
        "You are an expert audiobook production agent. Your job is to take a user's outline and produce a complete audiobook script.\n\n\
        CRITICAL: You MUST detect the language of the user's outline and work entirely in that language.\n\n\
        AVAILABLE CHARACTERS:\n{char_list}\n\n\
        {plan_context}\n\n\
        {extra}\
        You have tools available to help you:\n\
        - outline_analysis: Analyze the outline and create a structured plan\n\
        - character_extraction: Extract characters from text\n\
        - script_generation: Generate the complete script and save it\n\
        - story_recall: Search the story knowledge base for relevant plot/character info (semantic search)\n\
        - story_memory: Store or list important story facts, relationships, and worldbuilding\n\n\
        IMPORTANT: For long-form writing, ALWAYS use story_recall before writing to recall established facts,\
        and use story_memory to record new plot points or character developments as you write.\n\n\
        Follow this workflow:\n\
        1. If no plan exists yet, use outline_analysis first\n\
        2. Then use script_generation to create and save the script\n\
        Always use the tools in order. Do NOT skip steps.",
        char_list = char_list,
        plan_context = plan_context,
        extra = extra
    )
}

/// Build the initial conversation messages.
fn build_initial_messages(system_prompt: &str, outline: &str) -> Vec<serde_json::Value> {
    vec![
        json!({ "role": "system", "content": system_prompt }),
        json!({ "role": "user", "content": outline }),
    ]
}

/// The core agent loop: send messages to LLM, execute tool calls, repeat.
async fn agent_loop<E: EventEmitter>(
    emitter: &E,
    cancel_token: &CancellationToken,
    config: &AgentConfig,
    messages: &mut Vec<serde_json::Value>,
    characters: &[Character],
    db: &Mutex<Database>,
) -> Result<(), AppError> {
    let url = format!("{}/chat/completions", config.api_endpoint.trim_end_matches('/'));
    let client = reqwest::Client::new();
    let max_turns = 10;
    let mut turn = 0;

    while turn < max_turns {
        if cancel_token.is_cancelled() {
            emitter.emit_json("llm-complete", &json!(()));
            emitter.emit_json("llm-cancel", &json!(()));
            return Err(AppError::LlmService("Cancelled".to_string()));
        }

        turn += 1;

        let body = json!({
            "model": config.model,
            "messages": messages,
            "stream": true,
            "max_tokens": 16384,
            "enable_thinking": config.enable_thinking,
            "tools": build_tool_definitions()
        });

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

        let (tool_calls, _assistant_text) = process_streaming_response(emitter, response).await?;

        let assistant_message = if !tool_calls.is_empty() {
            let tool_calls_array: Vec<serde_json::Value> = tool_calls.iter().map(|tc| {
                json!({
                    "id": tc.call_id,
                    "type": "function",
                    "function": {
                        "name": tc.name,
                        "arguments": tc.arguments.to_string()
                    }
                })
            }).collect();

            json!({
                "role": "assistant",
                "content": null,
                "tool_calls": tool_calls_array
            })
        } else {
            json!({
                "role": "assistant",
                "content": _assistant_text
            })
        };

        messages.push(assistant_message);

        if tool_calls.is_empty() {
            break;
        }

        let mut results: Vec<Result<String, AppError>> = Vec::new();
        for tc in &tool_calls {
            let result = execute_tool(emitter, db, characters, config, &tc.name, tc.arguments.clone()).await;
            results.push(result);
        }

        let tool_messages = build_tool_result_message(&tool_calls, &results);
        if let serde_json::Value::Array(arr) = tool_messages {
            messages.extend(arr);
        }

        for (i, result) in results.iter().enumerate() {
            if let Err(e) = result {
                let msg = format!("Tool '{}' failed: {}", tool_calls[i].name, e);
                emitter.emit_json("llm-error", &json!(msg));
                return Err(AppError::LlmService(msg));
            }
        }
    }

    emitter.emit_json("llm-complete", &json!(()));
    Ok(())
}

/// Build tool definitions for the LLM.
fn build_tool_definitions() -> Vec<serde_json::Value> {
    vec![
        json!({
            "type": "function",
            "function": {
                "name": tools::OutlineAnalysisTool::name(),
                "description": tools::OutlineAnalysisTool::description(),
                "parameters": tools::OutlineAnalysisTool::parameters_schema()
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": tools::CharacterExtractionTool::name(),
                "description": tools::CharacterExtractionTool::description(),
                "parameters": tools::CharacterExtractionTool::parameters_schema()
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": tools::ScriptGenerationTool::name(),
                "description": tools::ScriptGenerationTool::description(),
                "parameters": tools::ScriptGenerationTool::parameters_schema()
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": tools::StoryRecallTool::name(),
                "description": tools::StoryRecallTool::description(),
                "parameters": tools::StoryRecallTool::parameters_schema()
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": tools::StoryMemoryTool::name(),
                "description": tools::StoryMemoryTool::description(),
                "parameters": tools::StoryMemoryTool::parameters_schema()
            }
        }),
    ]
}

/// Process the streaming response and return tool calls + accumulated text.
async fn process_streaming_response<E: EventEmitter>(
    emitter: &E,
    response: reqwest::Response,
) -> Result<(Vec<ParsedToolCall>, String), AppError> {
    let mut accumulated_text = String::new();
    let mut tool_calls_map: HashMap<usize, (String, String, String)> = HashMap::new();

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
                    if let Some(reasoning) = parsed["choices"][0]["delta"]["reasoning_content"].as_str() {
                        emitter.emit_json("llm-thinking", &json!(reasoning));
                    }
                    if let Some(content) = parsed["choices"][0]["delta"]["content"].as_str() {
                        accumulated_text.push_str(content);
                        emitter.emit_json("llm-token", &json!(content));
                    }
                    if let Some(tc_array) = parsed["choices"][0]["delta"]["tool_calls"].as_array() {
                        for tc in tc_array {
                            let index = tc.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                            let entry = tool_calls_map.entry(index).or_insert_with(|| {
                                (
                                    tc.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                                    tc.get("function").and_then(|f| f.get("name")).and_then(|v| v.as_str()).unwrap_or("").to_string(),
                                    String::new(),
                                )
                            });
                            if let Some(id) = tc.get("id").and_then(|v| v.as_str()) {
                                if !id.is_empty() { entry.0 = id.to_string(); }
                            }
                            if let Some(name) = tc.get("function").and_then(|f| f.get("name")).and_then(|v| v.as_str()) {
                                if !name.is_empty() { entry.1 = name.to_string(); }
                            }
                            if let Some(args) = tc.get("function").and_then(|f| f.get("arguments")).and_then(|v| v.as_str()) {
                                entry.2.push_str(args);
                            }
                        }
                    }
                }
            }
        }
    }

    let tool_calls: Vec<ParsedToolCall> = tool_calls_map
        .into_values()
        .filter_map(|(call_id, name, args_str)| {
            if call_id.is_empty() || name.is_empty() { return None; }
            let arguments: serde_json::Value = serde_json::from_str(&args_str).unwrap_or_else(|e| {
                eprintln!("[agent] Warning: failed to parse tool call args for '{}': {}", name, e);
                serde_json::Value::Null
            });
            Some(ParsedToolCall { call_id, name, arguments })
        })
        .collect();

    Ok((tool_calls, accumulated_text))
}

/// Build tool result messages for the conversation.
fn build_tool_result_message(tool_calls: &[ParsedToolCall], results: &[Result<String, AppError>]) -> serde_json::Value {
    let content: Vec<serde_json::Value> = tool_calls
        .iter()
        .zip(results.iter())
        .map(|(tc, result)| {
            let result_text = match result {
                Ok(s) => s.clone(),
                Err(e) => format!("Error: {}", e),
            };
            json!({
                "role": "tool",
                "tool_call_id": tc.call_id,
                "content": result_text
            })
        })
        .collect();

    serde_json::Value::Array(content)
}

/// A tool call parsed from the LLM response.
#[derive(Debug)]
struct ParsedToolCall {
    call_id: String,
    name: String,
    arguments: serde_json::Value,
}

/// Execute a named tool with the given arguments.
async fn execute_tool<E: EventEmitter>(
    emitter: &E,
    db: &Mutex<Database>,
    characters: &[Character],
    config: &AgentConfig,
    tool_name: &str,
    arguments: serde_json::Value,
) -> Result<String, AppError> {
    match tool_name {
        "outline_analysis" => {
            let outline = arguments.get("outline")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            tools::outline_analysis::do_outline_analysis(
                emitter,
                &outline,
                characters,
                &config.api_endpoint,
                &config.api_key,
                &config.model,
                config.enable_thinking,
            ).await.map(|plan| {
                format!("Plan generated successfully:\n{}", serde_json::to_string_pretty(&plan).unwrap_or_default())
            })
        }
        "character_extraction" => {
            let text = arguments.get("text")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            tools::character_extraction::do_character_extraction(
                emitter,
                &text,
                &config.api_endpoint,
                &config.api_key,
                &config.model,
                config.enable_thinking,
            ).await.map(|chars| {
                format!("Extracted {} characters:\n{}",
                    chars.len(),
                    serde_json::to_string_pretty(&chars).unwrap_or_default()
                )
            })
        }
        "script_generation" => {
            let outline = arguments.get("outline")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let extra = arguments.get("extra_instructions")
                .and_then(|v| v.as_str())
                .map(String::from);

            tools::script_generation::do_script_generation(
                emitter,
                db,
                &config.project_id,
                &outline,
                characters,
                &config.api_endpoint,
                &config.api_key,
                &config.model,
                None,
                extra.as_deref(),
                config.enable_thinking,
            ).await?;

            Ok(format!("Script generated and saved for project {}.", config.project_id))
        }
        "story_recall" => {
            let args: StoryRecallArgs = serde_json::from_value(arguments.clone())
                .map_err(|e| AppError::LlmService(format!("Invalid story_recall args: {}", e)))?;

            do_story_recall(
                emitter,
                db,
                &config.project_id,
                &config.api_endpoint,
                &config.api_key,
                &config.model,
                &args,
            ).await.map(|resp| {
                if resp.results.is_empty() {
                    "No relevant knowledge found.".to_string()
                } else {
                    let texts: Vec<String> = resp.results.iter().map(|r| {
                        format!("[{:.3}] {}\n{}", r.score, r.kb_type, r.text)
                    }).collect();
                    format!("Found {} results:\n\n{}", resp.results.len(), texts.join("\n\n"))
                }
            })
        }
        "story_memory" => {
            let args: StoryMemoryArgs = serde_json::from_value(arguments.clone())
                .map_err(|e| AppError::LlmService(format!("Invalid story_memory args: {}", e)))?;

            do_story_memory(
                emitter,
                db,
                &config.project_id,
                &config.api_endpoint,
                &config.api_key,
                &config.model,
                &args,
            ).await.map(|resp| {
                match &resp.items {
                    Some(items) => {
                        let lines: Vec<String> = items.iter().map(|i| {
                            format!("- [{}] {}: {}", i.kb_type, i.id, i.text)
                        }).collect();
                        format!("{}\n\n{}", resp.message, lines.join("\n"))
                    }
                    None => resp.message.clone(),
                }
            })
        }
        _ => Err(AppError::LlmService(format!("Unknown tool: {}", tool_name))),
    }
}
