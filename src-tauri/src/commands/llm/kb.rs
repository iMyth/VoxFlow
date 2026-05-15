use std::sync::Mutex;

use serde_json::json;

use crate::core::db::Database;
use crate::core::error::AppError;
use crate::core::event_emitter::EventEmitter;
use crate::core::models::StoryKnowledgeItem;
use crate::core::vector_store::fetch_embedding;

use super::utils::TauriEmitter;

/// Semantic search over the story knowledge base.
#[tauri::command]
pub async fn story_recall(
    app: tauri::AppHandle,
    db: tauri::State<'_, Mutex<Database>>,
    project_id: String,
    query: String,
    api_endpoint: String,
    api_key: String,
    model: String,
    kb_type: Option<String>,
    top_k: Option<usize>,
    _enable_thinking: bool,
) -> Result<Vec<crate::core::vector_store::StoryRecallResult>, AppError> {
    let args = crate::core::agent::tools::story_recall::StoryRecallArgs {
        query,
        kb_type,
        top_k: top_k.unwrap_or(5),
    };

    let emitter = TauriEmitter(app);

    let resp = crate::core::agent::tools::story_recall::do_story_recall(
        &emitter,
        &db,
        &project_id,
        &api_endpoint,
        &api_key,
        &model,
        &args,
    )
    .await?;

    Ok(resp.results)
}

/// Build the knowledge base from existing script content.
#[tauri::command]
pub async fn build_story_kb(
    app: tauri::AppHandle,
    db: tauri::State<'_, Mutex<Database>>,
    project_id: String,
    api_endpoint: String,
    api_key: String,
    model: String,
) -> Result<usize, AppError> {
    let emitter = TauriEmitter(app);

    // Load existing script
    let (sections, lines) = {
        let db_lock = db
            .lock()
            .map_err(|e| AppError::LlmService(format!("Database lock poisoned: {}", e)))?;
        db_lock
            .load_script_with_sections(&project_id)
            .map_err(|e| AppError::LlmService(format!("Failed to load script: {}", e)))?
    };

    // Clear existing KB for this project
    {
        let db_lock = db
            .lock()
            .map_err(|e| AppError::LlmService(format!("Database lock poisoned: {}", e)))?;
        db_lock
            .delete_all_story_kb(&project_id)
            .map_err(|e| AppError::LlmService(format!("Failed to clear KB: {}", e)))?;
    }

    // Index each section as a chunk
    let mut count = 0;
    for section in &sections {
        let section_lines: Vec<&crate::core::models::ScriptLine> = lines
            .iter()
            .filter(|l| l.section_id.as_deref() == Some(&section.id))
            .collect();

        if section_lines.is_empty() {
            continue;
        }

        let text = section_lines
            .iter()
            .map(|l| l.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");

        // Fetch embedding
        let embedding_vec = fetch_embedding(&api_endpoint, &api_key, &model, &text).await?;
        let embedding_json = serde_json::to_string(&embedding_vec)
            .map_err(|e| AppError::LlmService(format!("Failed to serialize embedding: {}", e)))?;

        let item = StoryKnowledgeItem {
            id: uuid::Uuid::new_v4().to_string(),
            project_id: project_id.clone(),
            text: format!("[{}] {}", section.title, text),
            embedding: embedding_json,
            kb_type: "plot".to_string(),
            metadata: serde_json::to_string(&json!({
                "section_id": section.id,
                "section_title": section.title,
                "line_count": section_lines.len()
            }))
            .unwrap_or_default(),
            created_at: chrono::Utc::now().to_rfc3339(),
        };

        {
            let db_lock = db
                .lock()
                .map_err(|e| AppError::LlmService(format!("Database lock poisoned: {}", e)))?;
            db_lock
                .insert_story_kb(&item)
                .map_err(|e| AppError::LlmService(format!("Failed to index section: {}", e)))?;
        }

        emitter.emit_json(
            "agent-kb-indexed",
            &json!({
                "section": section.title,
                "lines": section_lines.len()
            }),
        );

        count += 1;
    }

    Ok(count)
}
