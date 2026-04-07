use std::sync::Mutex;

use futures_util::StreamExt;
use tauri::Emitter;

use crate::core::agent::{
    AgentPlan, SuggestedCharacter,
};
use crate::core::cancel_token::CancellationToken;
use crate::core::db::Database;
use crate::core::error::AppError;
use crate::core::models::{Character, LlmScriptLine, LlmScriptResponse, ScriptLine};

/// Merge LLM-generated lines with existing script lines, preserving character
/// assignments and audio references for matched lines.
///
/// Matching strategy (in priority order):
/// 1. Exact text match
/// 2. Fuzzy match (Jaccard bigram similarity >= 0.7)
/// 3. Position match (same index)
pub fn merge_script_lines(
    old_lines: &[ScriptLine],
    new_lines: &[LlmScriptLine],
    characters: &[Character],
) -> Vec<ScriptLine> {
    let mut used_old: std::collections::HashSet<usize> = std::collections::HashSet::new();
    let mut result: Vec<ScriptLine> = Vec::new();

    for (i, new_line) in new_lines.iter().enumerate() {
        let text = new_line.text.trim();
        if text.is_empty() {
            continue;
        }

        // Find the best matching old line
        let matched_old = find_best_match(text, i, old_lines, &used_old);

        if let Some(old_idx) = matched_old {
            used_old.insert(old_idx);
            let old = &old_lines[old_idx];

            // Resolve character: old assignment takes priority
            let character_id = old.character_id.clone().or_else(|| {
                resolve_character(&new_line.character, characters)
            });

            // If text changed, use a new ID to avoid carrying over stale audio
            let id = if old.text.trim() == text {
                old.id.clone()
            } else {
                uuid::Uuid::new_v4().to_string()
            };

            // Resolve instructions: use old value if present, otherwise use LLM's suggestion
            let instructions = if !old.instructions.is_empty() {
                old.instructions.clone()
            } else {
                new_line.instructions.clone().unwrap_or_default()
            };

            result.push(ScriptLine {
                id,
                project_id: old.project_id.clone(),
                line_order: i as i32,
                text: text.to_string(),
                character_id,
                gap_after_ms: old.gap_after_ms,
                instructions,
            });
        } else {
            // Brand new line
            let gap_ms = new_line.gap_ms.unwrap_or(500);
            result.push(ScriptLine {
                id: uuid::Uuid::new_v4().to_string(),
                project_id: old_lines.first().map(|l| l.project_id.clone()).unwrap_or_default(),
                line_order: i as i32,
                text: text.to_string(),
                character_id: resolve_character(&new_line.character, characters),
                gap_after_ms: gap_ms as i32,
                instructions: new_line.instructions.clone().unwrap_or_default(),
            });
        }
    }

    result
}

/// Find the best matching old line for a given new text.
fn find_best_match(
    new_text: &str,
    index: usize,
    old_lines: &[ScriptLine],
    used: &std::collections::HashSet<usize>,
) -> Option<usize> {
    // Level 1: Exact match
    for (idx, old) in old_lines.iter().enumerate() {
        if used.contains(&idx) {
            continue;
        }
        if old.text.trim() == new_text {
            return Some(idx);
        }
    }

    // Level 2: Fuzzy match (Jaccard bigram similarity)
    let mut best_idx: Option<usize> = None;
    let mut best_score: f64 = 0.0;
    for (idx, old) in old_lines.iter().enumerate() {
        if used.contains(&idx) {
            continue;
        }
        let score = jaccard_similarity(new_text, old.text.trim());
        if score > best_score && score >= 0.6 {
            best_score = score;
            best_idx = Some(idx);
        }
    }
    if let Some(idx) = best_idx {
        return Some(idx);
    }

    // Level 3: Position match
    if index < old_lines.len() && !used.contains(&index) {
        return Some(index);
    }

    None
}

/// Compute Jaccard similarity between two strings based on character sets.
fn jaccard_similarity(a: &str, b: &str) -> f64 {
    let set_a: std::collections::HashSet<char> = a.chars().collect();
    let set_b: std::collections::HashSet<char> = b.chars().collect();

    if set_a.is_empty() && set_b.is_empty() {
        return 1.0;
    }
    if set_a.is_empty() || set_b.is_empty() {
        return 0.0;
    }

    let intersection = set_a.intersection(&set_b).count();
    let union = set_a.len() + set_b.len() - intersection;
    intersection as f64 / union as f64
}

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

    let extra = extra_instructions
        .filter(|s| !s.trim().is_empty())
        .map(|s| format!("用户额外要求：{}\n", s))
        .unwrap_or_default();

    let system_prompt = format!(
        "你是有声书剧本编写助手。根据用户提供的大纲，生成或更新有声书剧本。\n\n\
        {extra}{char_list}\n\n\
        返回 JSON 格式：\n\
        {{\"lines\":[\
        {{\"text\":\"台词内容\",\"character\":\"角色名\",\"instructions\":\"情绪/语速指令或null\",\"gap_ms\":500}},...\
        ]}}\n\n\
        规则：\n\
        1. character 字段必填，每行必须指定角色\n\
        2. instructions 字段描述语音生成指令（情绪、语速、语调），如不确定用 null\n\
        3. gap_ms 表示停顿毫秒数，推荐 500-2000，默认 500\n\
        4. 每行一句台词\n\
        5. 不要包含任何 markdown 代码块，只返回 JSON",
        extra = extra,
        char_list = char_list
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

    // Get existing script lines for merge
    let old_lines = {
        let db = db.lock().map_err(|e| AppError::Database(e.to_string()))?;
        db.load_script(&project_id)?
    };

    // Merge new lines with existing
    let merged_lines = merge_script_lines(&old_lines, &llm_response.lines, &characters);

    // Save to database
    let db = db.lock().map_err(|e| {
        let msg = format!("数据库锁获取失败: {}", e);
        let _ = app.emit("llm-error", &msg);
        AppError::Database(msg)
    })?;
    db.save_script(&project_id, &merged_lines).map_err(|e| {
        let msg = format!("保存剧本失败: {}", e);
        let _ = app.emit("llm-error", &msg);
        e
    })?;

    Ok(())
}

/// Parse LLM response text as JSON LlmScriptResponse.
/// Strips markdown code block fences if present.
fn parse_llm_json(text: &str) -> Result<LlmScriptResponse, String> {
    let trimmed = text.trim();

    // Strip markdown code block: ```json ... ``` or ``` ... ```
    let json_str = if trimmed.starts_with("```") {
        // Find the end of the first line (may contain "json" or "JSON")
        if let Some(first_newline) = trimmed.find('\n') {
            let after_fence = &trimmed[first_newline + 1..];
            // Strip trailing ```
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

    serde_json::from_str::<LlmScriptResponse>(&json_str)
        .map_err(|e| e.to_string())
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
) -> Result<(), AppError> {
    let db = db.lock().map_err(|e| AppError::Database(e.to_string()))?;
    db.save_script(&project_id, &lines)
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

    fn make_old_line(id: &str, text: &str, character_id: Option<&str>) -> ScriptLine {
        ScriptLine {
            id: id.to_string(),
            project_id: "p1".to_string(),
            line_order: 0,
            text: text.to_string(),
            character_id: character_id.map(String::from),
            gap_after_ms: 500,
            instructions: String::new(),
        }
    }

    fn make_llm_line(text: &str, character: Option<&str>, instructions: Option<&str>) -> LlmScriptLine {
        LlmScriptLine {
            text: text.to_string(),
            character: character.map(String::from),
            instructions: instructions.map(String::from),
            gap_ms: None,
        }
    }

    // ---- merge tests ----

    #[test]
    fn test_merge_exact_match_preserves_character_and_audio() {
        let old = vec![
            make_old_line("line-1", "你好世界", Some("char-A")),
            make_old_line("line-2", "第二句话", None),
        ];
        let chars = vec![make_char("char-A", "Alice"), make_char("char-B", "Bob")];
        let new = vec![
            make_llm_line("你好世界", Some("Bob"), None),
            make_llm_line("第二句话", None, None),
        ];

        let result = merge_script_lines(&old, &new, &chars);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].id, "line-1");  // preserved ID
        assert_eq!(result[0].character_id, Some("char-A".to_string()));  // preserved char
        assert_eq!(result[1].id, "line-2");
    }

    #[test]
    fn test_merge_fuzzy_match() {
        let old = vec![make_old_line("line-1", "今天天气真好", Some("char-A"))];
        let chars = vec![make_char("char-A", "Alice")];
        let new = vec![make_llm_line("今天天气很好", None, Some("语速缓慢，带有怀念的语气"))];  // similar but edited

        let result = merge_script_lines(&old, &new, &chars);
        assert_eq!(result.len(), 1);
        assert_ne!(result[0].id, "line-1");  // new ID because text changed
        assert_eq!(result[0].text, "今天天气很好");
        assert_eq!(result[0].character_id, Some("char-A".to_string()));
        // LLM provided instructions, new line adopts them
        assert_eq!(result[0].instructions, "语速缓慢，带有怀念的语气");
    }

    #[test]
    fn test_merge_new_lines_gets_character() {
        let old = vec![];
        let chars = vec![make_char("char-A", "Alice"), make_char("char-B", "Bob")];
        let new = vec![make_llm_line("新台词", Some("Bob"), None)];

        let result = merge_script_lines(&old, &new, &chars);
        assert_eq!(result.len(), 1);
        assert_ne!(result[0].id, "");  // new UUID
        assert_eq!(result[0].character_id, Some("char-B".to_string()));
    }

    #[test]
    fn test_merge_deletes_unmatched_old_lines() {
        let old = vec![
            make_old_line("line-1", "保留的行", None),
            make_old_line("line-2", "删除的行", None),
        ];
        let new = vec![make_llm_line("保留的行", None, None)];

        let result = merge_script_lines(&old, &new, &[]);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].id, "line-1");
    }

    #[test]
    fn test_merge_position_match_uses_llm_character() {
        let old = vec![make_old_line("line-1", "旧文本", None)];
        let chars = vec![make_char("char-A", "Alice")];
        let new = vec![make_llm_line("全新内容", Some("Alice"), None)];  // position match

        let result = merge_script_lines(&old, &new, &chars);
        assert_eq!(result.len(), 1);
        assert_ne!(result[0].id, "line-1");  // new ID because text changed
        assert_eq!(result[0].text, "全新内容");
        // Old had no character, LLM assigned one → use LLM's
        assert_eq!(result[0].character_id, Some("char-A".to_string()));
    }

    #[test]
    fn test_merge_position_match_preserves_old_character() {
        let old = vec![make_old_line("line-1", "旧文本", Some("char-A"))];
        let chars = vec![make_char("char-A", "Alice"), make_char("char-B", "Bob")];
        let new = vec![make_llm_line("全新内容", Some("Bob"), Some("语气坚定，语速中等"))];  // position match

        let result = merge_script_lines(&old, &new, &chars);
        assert_eq!(result.len(), 1);
        assert_ne!(result[0].id, "line-1");  // new ID because text changed
        // Old had char-A → preserve it even though LLM suggested Bob
        assert_eq!(result[0].character_id, Some("char-A".to_string()));
    }

    #[test]
    fn test_merge_skips_empty_llm_lines() {
        let old: Vec<ScriptLine> = vec![];
        let new = vec![
            make_llm_line("有效行", None, None),
            make_llm_line("   ", None, None),
            make_llm_line("", None, None),
            make_llm_line("另一行", None, None),
        ];

        let result = merge_script_lines(&old, &new, &[]);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].text, "有效行");
        assert_eq!(result[1].text, "另一行");
    }

    // ---- Jaccard similarity tests ----

    #[test]
    fn test_jaccard_identical_strings() {
        assert_eq!(jaccard_similarity("hello", "hello"), 1.0);
    }

    #[test]
    fn test_jaccard_completely_different() {
        assert_eq!(jaccard_similarity("abc", "xyz"), 0.0);
    }

    #[test]
    fn test_jaccard_similar_texts() {
        let score = jaccard_similarity("今天天气真好", "今天天气很好");
        assert!(score >= 0.6, "Expected similarity >= 0.6, got {}", score);
    }

    #[test]
    fn test_jaccard_empty_strings() {
        assert_eq!(jaccard_similarity("", ""), 1.0);
    }

    // ---- JSON parsing tests ----

    #[test]
    fn test_parse_llm_json_basic() {
        let text = r#"{"lines":[{"text":"台词1","character":null},{"text":"台词2","character":"Alice"}]}"#;
        let resp = parse_llm_json(text).unwrap();
        assert_eq!(resp.lines.len(), 2);
        assert_eq!(resp.lines[0].text, "台词1");
        assert!(resp.lines[0].character.is_none());
        assert_eq!(resp.lines[1].text, "台词2");
        assert_eq!(resp.lines[1].character, Some("Alice".to_string()));
    }

    #[test]
    fn test_parse_llm_json_strips_markdown_fence() {
        let text = "```json\n{\"lines\":[{\"text\":\"hello\",\"character\":null}]}\n```";
        let resp = parse_llm_json(text).unwrap();
        assert_eq!(resp.lines.len(), 1);
        assert_eq!(resp.lines[0].text, "hello");
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
