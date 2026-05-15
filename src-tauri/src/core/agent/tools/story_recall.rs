use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::core::db::Database;
use crate::core::error::AppError;
use crate::core::event_emitter::EventEmitter;
use crate::core::vector_store::{fetch_embedding, semantic_search, StoryRecallResult};

pub struct StoryRecallTool;

impl StoryRecallTool {
    pub fn name() -> &'static str {
        "story_recall"
    }

    pub fn description() -> &'static str {
        "Search the story knowledge base for relevant plot points, character details, or settings. Use this to recall established facts before writing new content."
    }

    pub fn parameters_schema() -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Natural language query — what you want to recall from the story"
                },
                "kb_type": {
                    "type": "string",
                    "enum": ["plot", "character", "setting", "foreshadow", "any"],
                    "description": "Filter by knowledge type, or 'any' for all types"
                },
                "top_k": {
                    "type": "integer",
                    "description": "Number of results to return (default 5)"
                }
            },
            "required": ["query"]
        })
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct StoryRecallArgs {
    pub query: String,
    #[serde(default)]
    pub kb_type: Option<String>,
    #[serde(default = "default_top_k")]
    pub top_k: usize,
}

fn default_top_k() -> usize {
    5
}

#[derive(Debug, Clone, Serialize)]
pub struct StoryRecallResponse {
    pub query: String,
    pub results: Vec<StoryRecallResult>,
}

/// Perform semantic search over the story knowledge base.
pub async fn do_story_recall<E: EventEmitter>(
    emitter: &E,
    db: &std::sync::Mutex<Database>,
    project_id: &str,
    api_endpoint: &str,
    api_key: &str,
    model: &str,
    args: &StoryRecallArgs,
) -> Result<StoryRecallResponse, AppError> {
    emitter.emit_json(
        "agent-tool-call",
        &json!({
            "tool": "story_recall",
            "query": args.query
        }),
    );

    // Fetch embedding for the query
    let query_embedding = fetch_embedding(api_endpoint, api_key, model, &args.query).await?;

    // Load knowledge items (optionally filtered by type)
    let kb_type_filter = args
        .kb_type
        .as_ref()
        .filter(|t| *t != "any")
        .map(|s| s.as_str());
    let items = {
        let db_lock = db
            .lock()
            .map_err(|e| AppError::LlmService(format!("Database lock poisoned: {}", e)))?;
        db_lock
            .list_story_kb(project_id, kb_type_filter)
            .map_err(|e| AppError::LlmService(format!("Failed to read story KB: {}", e)))?
    };

    let top_k = if args.top_k == 0 { 5 } else { args.top_k };
    let results = semantic_search(&items, &query_embedding, top_k);

    emitter.emit_json(
        "agent-tool-result",
        &json!({
            "tool": "story_recall",
            "results_count": results.len()
        }),
    );

    Ok(StoryRecallResponse {
        query: args.query.clone(),
        results,
    })
}
