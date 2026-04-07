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

    let system_prompt = if existing_char_names.is_empty() {
        "你是有声书剧本分析助手。分析用户提供的大纲，返回结构化规划方案。\n\n\
        要求：\n\
        1. 识别章节/场景（chapters），每章估计台词数、涉及角色、情绪氛围\n\
        2. 提取所有角色（suggested_characters），说明角色定位（主角/配角/旁白等）\n\
        3. 判断是否匹配现有项目角色\n\
        4. 总结整体风格（overall_style）\n\
        5. 给出角色配置建议（character_notes）\n\n\
        返回 JSON 格式：\n\
        {\"chapters\":[{\"title\":\"章节名\",\"estimated_lines\":10,\"characters\":[\"角色名\"],\"mood\":\"情绪描述\"}],\
        \"suggested_characters\":[{\"name\":\"角色名\",\"role\":\"定位\",\"matched_existing\":false,\"existing_id\":null}],\
        \"overall_style\":\"风格描述\",\"character_notes\":\"配置建议\"}"
            .to_string()
    } else {
        format!(
            "你是有声书剧本分析助手。分析用户提供的大纲，返回结构化规划方案。\n\n\
            要求：\n\
            1. 识别章节/场景（chapters），每章估计台词数、涉及角色、情绪氛围\n\
            2. 提取所有角色（suggested_characters），说明角色定位（主角/配角/旁白等）\n\
            3. 判断是否匹配现有项目角色\n\
            4. 总结整体风格（overall_style）\n\
            5. 给出角色配置建议（character_notes）\n\n\
            项目已有角色: {}\n\n\
            返回 JSON 格式：\n\
            {{\"chapters\":[{{\"title\":\"章节名\",\"estimated_lines\":10,\"characters\":[\"角色名\"],\"mood\":\"情绪描述\"}}],\
            \"suggested_characters\":[{{\"name\":\"角色名\",\"role\":\"定位\",\"matched_existing\":false,\"existing_id\":null}}],\
            \"overall_style\":\"风格描述\",\"character_notes\":\"配置建议\"}}",
            existing_char_names.join(", ")
        )
    };

    let body = json!({
        "model": model,
        "messages": [
            { "role": "system", "content": system_prompt },
            { "role": "user", "content": outline }
        ],
        "stream": true,
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
            let msg = format!("LLM 请求失败: {}", e);
            let _ = app.emit("llm-error", &msg);
            AppError::LlmService(msg)
        })?;

    if !response.status().is_success() {
        let status = response.status();
        let body_text = response.text().await.unwrap_or_default();
        let msg = format!("LLM API 错误 {}: {}", status, body_text);
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
            let msg = "已取消";
            let _ = app.emit("llm-cancel", &());
            return Err(AppError::LlmService(msg.to_string()));
        }

        let chunk = chunk_result.map_err(|e| {
            let msg = format!("读取 LLM 响应失败: {}", e);
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
            let msg = format!("解析规划结果失败: {}\n原始内容: {}", e, accumulated_text.chars().take(300).collect::<String>());
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
        "（本项目暂无角色，可直接使用角色名，系统会自动创建）".to_string()
    } else {
        let names: Vec<&str> = characters.iter().map(|c| c.name.as_str()).collect();
        format!("可用角色: {}", names.join("、"))
    };

    // Build chapter reference info from the plan (as guidance, not hard requirement)
    let chapter_info = agent_plan.as_ref().map(|p| {
        let ch_descs: Vec<String> = p.chapters.iter().map(|ch| {
            format!(
                "「{}」约{}行台词，情绪氛围：{}，涉及角色：{}",
                ch.title,
                ch.estimated_lines,
                ch.mood,
                if ch.characters.is_empty() { "无特定角色".to_string() } else { ch.characters.join("、") }
            )
        }).collect();
        format!(
            "【章节规划参考】\n{}\n\n注意：以上章节结构仅供参考，请根据大纲内容自然展开剧情，每个章节的台词数量可以根据剧情需要自由调整，关键是内容充实、剧情完整。避免用省略号或重复内容填充行数，每句台词都应该推动故事发展。",
            ch_descs.join("\n")
        )
    }).unwrap_or_default();

    let extra = extra_instructions
        .filter(|s| !s.trim().is_empty())
        .map(|s| format!("用户额外要求：{}\n", s))
        .unwrap_or_default();

    let system_prompt = format!(
        "你是有声书剧本编写助手。根据用户提供的大纲，生成或更新有声书剧本。\n\n\
        {extra}{char_list}\n\n\
        {chapter_info}\n\n\
        按段落/场景分组返回剧本（如「片头」「第一幕」「第二幕」「片尾」等），\
        如果大纲没有明确的段落划分，可自动分为 3-5 个场景。\n\n\
        返回 JSON 格式：\n\
        {{\"sections\":[\
        {{\"title\":\"段落标题\",\"lines\":[\
        {{\"text\":\"台词内容\",\"character\":\"角色名\",\"instructions\":\"情绪/语速指令或null\",\"gap_ms\":500}},...\
        ]}},...\
        ]}}\n\n\
        规则：\n\
        1. character 字段必填，每行必须指定角色\n\
        2. instructions 字段描述语音生成指令（情绪、语速、语调），如不确定用 null\n\
        3. gap_ms 表示停顿毫秒数，推荐 500-2000，默认 500\n\
        4. 每行一句完整的台词，有实际内容\n\
        5. 避免过度使用省略号（……）、占位符（略）或重复内容来凑字数。省略号可用于语气停顿，但不应作为填充手段\n\
        6. 每个场景需要充分展开剧情，角色对话应推动情节发展或揭示角色性格\n\
        7. 不要包含任何 markdown 代码块，只返回 JSON",
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
            let msg = format!("LLM 请求失败: {}", e);
            let _ = app.emit("llm-error", &msg);
            AppError::LlmService(msg)
        })?;

    if !response.status().is_success() {
        let status = response.status();
        let body_text = response.text().await.unwrap_or_default();
        let msg = format!("LLM API 返回错误 {}: {}", status, body_text);
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
            let msg = format!("读取 LLM 响应失败: {}", e);
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
        let msg = format!("解析 LLM JSON 响应失败: {}，原始内容: {}", e, accumulated_text.chars().take(200).collect::<String>());
        let _ = app.emit("llm-error", &msg);
        AppError::LlmService(msg)
    })?;

    // Delete old sections and lines, save fresh LLM output directly
    let db = db.lock().map_err(|e| {
        let msg = format!("数据库锁获取失败: {}", e);
        let _ = app.emit("llm-error", &msg);
        AppError::Database(msg)
    })?;
    db.delete_sections(&project_id).map_err(|e| {
        let msg = format!("删除旧章节失败: {}", e);
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
            let msg = format!("保存剧本失败: {}", e);
            let _ = app.emit("llm-error", &msg);
            e
        })?;
    }

    db.save_script(&project_id, &lines, &sections).map_err(|e| {
        let msg = format!("保存剧本失败: {}", e);
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

    Err(format!("无法解析为 sections 或 lines 格式，原始内容: {}", json_str.chars().take(500).collect::<String>()))
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
