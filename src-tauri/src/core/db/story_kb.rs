//! Story knowledge base operations for vector search.

use rusqlite::Connection;

use super::super::error::AppError;
use super::super::models::StoryKnowledgeItem;

/// Insert a knowledge item into the story vector DB.
pub fn insert_story_kb(conn: &Connection, item: &StoryKnowledgeItem) -> Result<(), AppError> {
    conn.execute(
        "INSERT INTO story_kb (id, project_id, text, embedding, kb_type, metadata) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![
            item.id,
            item.project_id,
            item.text,
            item.embedding,
            item.kb_type,
            item.metadata
        ],
    )
    .map_err(|e| AppError::Database(e.to_string()))?;
    Ok(())
}

/// Delete a knowledge item.
pub fn delete_story_kb(conn: &Connection, id: &str) -> Result<(), AppError> {
    conn.execute(
        "DELETE FROM story_kb WHERE id = ?1",
        rusqlite::params![id],
    )
    .map_err(|e| AppError::Database(e.to_string()))?;
    Ok(())
}

/// List all knowledge items for a project.
pub fn list_story_kb(
    conn: &Connection,
    project_id: &str,
    kb_type: Option<&str>,
) -> Result<Vec<StoryKnowledgeItem>, AppError> {
    let mut items = Vec::new();

    match kb_type {
        Some(t) => {
            let mut stmt = conn
                .prepare_cached(
                    "SELECT id, project_id, text, embedding, kb_type, metadata, created_at FROM story_kb WHERE project_id = ?1 AND kb_type = ?2 ORDER BY created_at"
                )
                .map_err(|e| AppError::Database(e.to_string()))?;
            let mut rows = stmt
                .query(rusqlite::params![project_id, t])
                .map_err(|e| AppError::Database(e.to_string()))?;
            while let Some(row) = rows.next().map_err(|e| AppError::Database(e.to_string()))? {
                items.push(StoryKnowledgeItem {
                    id: row.get(0).map_err(|e| AppError::Database(e.to_string()))?,
                    project_id: row.get(1).map_err(|e| AppError::Database(e.to_string()))?,
                    text: row.get(2).map_err(|e| AppError::Database(e.to_string()))?,
                    embedding: row.get(3).map_err(|e| AppError::Database(e.to_string()))?,
                    kb_type: row.get(4).map_err(|e| AppError::Database(e.to_string()))?,
                    metadata: row.get(5).map_err(|e| AppError::Database(e.to_string()))?,
                    created_at: row.get(6).map_err(|e| AppError::Database(e.to_string()))?,
                });
            }
        }
        None => {
            let mut stmt = conn
                .prepare_cached(
                    "SELECT id, project_id, text, embedding, kb_type, metadata, created_at FROM story_kb WHERE project_id = ?1 ORDER BY created_at"
                )
                .map_err(|e| AppError::Database(e.to_string()))?;
            let mut rows = stmt
                .query(rusqlite::params![project_id])
                .map_err(|e| AppError::Database(e.to_string()))?;
            while let Some(row) = rows.next().map_err(|e| AppError::Database(e.to_string()))? {
                items.push(StoryKnowledgeItem {
                    id: row.get(0).map_err(|e| AppError::Database(e.to_string()))?,
                    project_id: row.get(1).map_err(|e| AppError::Database(e.to_string()))?,
                    text: row.get(2).map_err(|e| AppError::Database(e.to_string()))?,
                    embedding: row.get(3).map_err(|e| AppError::Database(e.to_string()))?,
                    kb_type: row.get(4).map_err(|e| AppError::Database(e.to_string()))?,
                    metadata: row.get(5).map_err(|e| AppError::Database(e.to_string()))?,
                    created_at: row.get(6).map_err(|e| AppError::Database(e.to_string()))?,
                });
            }
        }
    }

    Ok(items)
}

/// Bulk delete all story_kb items for a project.
pub fn delete_all_story_kb(conn: &Connection, project_id: &str) -> Result<(), AppError> {
    conn.execute(
        "DELETE FROM story_kb WHERE project_id = ?1",
        rusqlite::params![project_id],
    )
    .map_err(|e| AppError::Database(e.to_string()))?;
    Ok(())
}

/// Load script sections and lines together (for KB indexing).
pub fn load_script_with_sections(
    conn: &Connection,
    project_id: &str,
) -> Result<(Vec<crate::core::models::ScriptSection>, Vec<crate::core::models::ScriptLine>), AppError> {
    let sections = super::script::list_sections(conn, project_id)?;
    let lines = super::script::load_script(conn, project_id)?;
    Ok((sections, lines))
}
