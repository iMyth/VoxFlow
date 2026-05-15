use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::core::db::Database;
use crate::core::error::AppError;
use crate::core::event_emitter::EventEmitter;
use crate::core::models::StoryKnowledgeItem;
use crate::core::vector_store::fetch_embedding;

pub struct StoryMemoryTool;

impl StoryMemoryTool {
    pub fn name() -> &'static str {
        "story_memory"
    }

    pub fn description() -> &'static str {
        "Store or list important story facts, character relationships, foreshadowing, or worldbuilding details. Call this to remember key information for future writing sessions."
    }

    pub fn parameters_schema() -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["add", "list", "delete"],
                    "description": "Action to perform"
                },
                "text": {
                    "type": "string",
                    "description": "The story fact to remember (required for add)"
                },
                "kb_type": {
                    "type": "string",
                    "enum": ["plot", "character", "setting", "foreshadow"],
                    "description": "Type of knowledge (required for add, default 'plot')"
                },
                "item_id": {
                    "type": "string",
                    "description": "ID of item to delete (required for delete)"
                }
            },
            "required": ["action"]
        })
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct StoryMemoryArgs {
    pub action: String,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub kb_type: Option<String>,
    #[serde(default)]
    pub item_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StoryMemoryResponse {
    pub action: String,
    pub message: String,
    pub items: Option<Vec<StoredMemory>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredMemory {
    pub id: String,
    pub text: String,
    pub kb_type: String,
    pub metadata: String,
}

/// Store or list story facts in the knowledge base.
pub async fn do_story_memory<E: EventEmitter>(
    emitter: &E,
    db: &std::sync::Mutex<Database>,
    project_id: &str,
    api_endpoint: &str,
    api_key: &str,
    model: &str,
    args: &StoryMemoryArgs,
) -> Result<StoryMemoryResponse, AppError> {
    match args.action.as_str() {
        "add" => {
            let text = args.text.as_ref().ok_or_else(|| {
                AppError::LlmService("'text' is required for action 'add'".to_string())
            })?;

            emitter.emit_json(
                "agent-tool-call",
                &json!({
                    "tool": "story_memory",
                    "action": "add",
                    "text": text
                }),
            );

            let kb_type = args.kb_type.as_deref().unwrap_or("plot");

            // Fetch embedding for the stored text
            let embedding = fetch_embedding(api_endpoint, api_key, model, text).await?;
            let embedding_json = serde_json::to_string(&embedding).map_err(|e| {
                AppError::LlmService(format!("Failed to serialize embedding: {}", e))
            })?;

            let item = StoryKnowledgeItem {
                id: uuid::Uuid::new_v4().to_string(),
                project_id: project_id.to_string(),
                text: text.clone(),
                embedding: embedding_json,
                kb_type: kb_type.to_string(),
                metadata: "{}".to_string(),
                created_at: chrono::Utc::now().to_rfc3339(),
            };

            let db_lock = db
                .lock()
                .map_err(|e| AppError::LlmService(format!("Database lock poisoned: {}", e)))?;
            db_lock
                .insert_story_kb(&item)
                .map_err(|e| AppError::LlmService(format!("Failed to store memory: {}", e)))?;

            emitter.emit_json(
                "agent-tool-result",
                &json!({
                    "tool": "story_memory",
                    "action": "add",
                    "id": item.id
                }),
            );

            Ok(StoryMemoryResponse {
                action: "add".to_string(),
                message: format!("Stored: \"{}\"", text),
                items: None,
            })
        }
        "list" => {
            let db_lock = db
                .lock()
                .map_err(|e| AppError::LlmService(format!("Database lock poisoned: {}", e)))?;
            let items = db_lock
                .list_story_kb(project_id, None)
                .map_err(|e| AppError::LlmService(format!("Failed to list memories: {}", e)))?;

            let stored: Vec<StoredMemory> = items
                .into_iter()
                .map(|i| StoredMemory {
                    id: i.id,
                    text: i.text,
                    kb_type: i.kb_type,
                    metadata: i.metadata,
                })
                .collect();

            Ok(StoryMemoryResponse {
                action: "list".to_string(),
                message: format!("{} memories stored", stored.len()),
                items: Some(stored),
            })
        }
        "delete" => {
            let item_id = args.item_id.as_ref().ok_or_else(|| {
                AppError::LlmService("'item_id' is required for action 'delete'".to_string())
            })?;

            let db_lock = db
                .lock()
                .map_err(|e| AppError::LlmService(format!("Database lock poisoned: {}", e)))?;
            db_lock
                .delete_story_kb(item_id)
                .map_err(|e| AppError::LlmService(format!("Failed to delete memory: {}", e)))?;

            Ok(StoryMemoryResponse {
                action: "delete".to_string(),
                message: format!("Deleted memory: {}", item_id),
                items: None,
            })
        }
        other => Err(AppError::LlmService(format!(
            "Unknown action for story_memory: {}",
            other
        ))),
    }
}
