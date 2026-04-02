use std::sync::Mutex;

use tauri::Emitter;

use crate::core::db::Database;
use crate::core::error::AppError;
use crate::core::models::ScriptLine;

/// Parse raw LLM output text into a list of ScriptLine structs.
///
/// Each non-empty line becomes a ScriptLine with:
/// - A new UUID for `id`
/// - The given `project_id`
/// - `line_order` set to the 0-based index among non-empty lines
/// - `text` set to the trimmed line content
/// - `character_id` set to `None`
pub fn parse_llm_output(raw_text: &str, project_id: &str) -> Vec<ScriptLine> {
    raw_text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .enumerate()
        .map(|(i, line)| ScriptLine {
            id: uuid::Uuid::new_v4().to_string(),
            project_id: project_id.to_string(),
            line_order: i as i32,
            text: line.trim().to_string(),
            character_id: None,
        })
        .collect()
}

#[tauri::command]
pub async fn generate_script(
    app: tauri::AppHandle,
    db: tauri::State<'_, Mutex<Database>>,
    project_id: String,
    outline: String,
    api_endpoint: String,
    api_key: String,
    model: String,
) -> Result<(), AppError> {
    use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
    use serde_json::json;

    let url = format!("{}/chat/completions", api_endpoint.trim_end_matches('/'));

    let body = json!({
        "model": model,
        "messages": [
            {
                "role": "system",
                "content": "你是一个有声书剧本编写助手。根据用户提供的大纲，生成有声书剧本。每行一句台词，不要添加角色标注或编号。"
            },
            {
                "role": "user",
                "content": outline
            }
        ],
        "stream": true
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

    // Read SSE stream
    let mut accumulated_text = String::new();
    let bytes = response.bytes().await.map_err(|e| {
        let msg = format!("读取 LLM 响应失败: {}", e);
        let _ = app.emit("llm-error", &msg);
        AppError::LlmService(msg)
    })?;

    let body_str = String::from_utf8_lossy(&bytes);

    for line in body_str.lines() {
        let line = line.trim();
        if line.is_empty() || line == "data: [DONE]" {
            continue;
        }
        if let Some(data) = line.strip_prefix("data: ") {
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(data) {
                if let Some(content) = parsed["choices"][0]["delta"]["content"].as_str() {
                    accumulated_text.push_str(content);
                    let _ = app.emit("llm-token", content);
                }
            }
        }
    }

    // Signal stream completion
    let _ = app.emit("llm-complete", &());

    // Parse accumulated text into script lines
    let lines = parse_llm_output(&accumulated_text, &project_id);

    // Save to database
    let db = db.lock().map_err(|e| {
        let msg = format!("数据库锁获取失败: {}", e);
        let _ = app.emit("llm-error", &msg);
        AppError::Database(msg)
    })?;
    db.save_script(&project_id, &lines).map_err(|e| {
        let msg = format!("保存剧本失败: {}", e);
        let _ = app.emit("llm-error", &msg);
        e
    })?;

    Ok(())
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

    #[test]
    fn test_parse_llm_output_basic() {
        let text = "第一行台词\n第二行台词\n第三行台词";
        let lines = parse_llm_output(text, "proj-1");

        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0].text, "第一行台词");
        assert_eq!(lines[0].line_order, 0);
        assert_eq!(lines[0].project_id, "proj-1");
        assert!(lines[0].character_id.is_none());

        assert_eq!(lines[1].text, "第二行台词");
        assert_eq!(lines[1].line_order, 1);

        assert_eq!(lines[2].text, "第三行台词");
        assert_eq!(lines[2].line_order, 2);
    }

    #[test]
    fn test_parse_llm_output_skips_empty_lines() {
        let text = "第一行\n\n\n第二行\n   \n第三行\n";
        let lines = parse_llm_output(text, "proj-1");

        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0].text, "第一行");
        assert_eq!(lines[0].line_order, 0);
        assert_eq!(lines[1].text, "第二行");
        assert_eq!(lines[1].line_order, 1);
        assert_eq!(lines[2].text, "第三行");
        assert_eq!(lines[2].line_order, 2);
    }

    #[test]
    fn test_parse_llm_output_trims_whitespace() {
        let text = "  hello  \n  world  ";
        let lines = parse_llm_output(text, "p1");

        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].text, "hello");
        assert_eq!(lines[1].text, "world");
    }

    #[test]
    fn test_parse_llm_output_empty_input() {
        let lines = parse_llm_output("", "p1");
        assert!(lines.is_empty());
    }

    #[test]
    fn test_parse_llm_output_only_whitespace() {
        let lines = parse_llm_output("   \n  \n\n", "p1");
        assert!(lines.is_empty());
    }

    #[test]
    fn test_parse_llm_output_unique_ids() {
        let text = "line1\nline2\nline3";
        let lines = parse_llm_output(text, "p1");

        let ids: Vec<&str> = lines.iter().map(|l| l.id.as_str()).collect();
        let unique: std::collections::HashSet<&str> = ids.iter().copied().collect();
        assert_eq!(ids.len(), unique.len(), "All line IDs should be unique");
    }

    #[test]
    fn test_parse_llm_output_single_line() {
        let text = "只有一行";
        let lines = parse_llm_output(text, "p1");

        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].text, "只有一行");
        assert_eq!(lines[0].line_order, 0);
    }
}
