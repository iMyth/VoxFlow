//! Script lines and sections operations.

use rusqlite::Connection;

use super::super::error::AppError;
use super::super::models::{ScriptLine, ScriptSection};

/// A script line with resolved character name and section title (for export).
#[derive(Debug, Clone)]
pub struct ScriptLineWithMeta {
    pub id: String,
    pub project_id: String,
    pub line_order: i32,
    pub text: String,
    pub character_id: Option<String>,
    pub gap_after_ms: i32,
    pub instructions: String,
    pub section_id: Option<String>,
    pub character_name: Option<String>,
    pub section_title: Option<String>,
}

// ---- Script Line operations ----

/// Save script lines for a project using upsert to avoid cascade-deleting audio fragments.
/// Lines not in the new set are deleted; existing lines are updated; new lines are inserted.
pub fn save_script(
    conn: &Connection,
    project_id: &str,
    lines: &[ScriptLine],
    sections: &[ScriptSection],
) -> Result<(), AppError> {
    // First save sections
    save_sections(conn, project_id, sections)?;

    let tx = conn
        .unchecked_transaction()
        .map_err(|e| AppError::Database(e.to_string()))?;

    // Collect IDs of lines being saved
    let new_ids: Vec<&str> = lines.iter().map(|l| l.id.as_str()).collect();

    // Delete lines that are no longer in the set (this will cascade-delete their audio fragments, which is correct)
    if new_ids.is_empty() {
        tx.execute(
            "DELETE FROM script_lines WHERE project_id = ?1",
            rusqlite::params![project_id],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
    } else {
        // Build dynamic parameterized placeholders for NOT IN clause
        let placeholders: Vec<String> = (1..=new_ids.len())
            .map(|i| format!("?{}", i + 1))
            .collect();
        let sql = format!(
            "DELETE FROM script_lines WHERE project_id = ?1 AND id NOT IN ({})",
            placeholders.join(",")
        );
        let mut params: Vec<rusqlite::types::ToSqlOutput> =
            vec![rusqlite::types::ToSqlOutput::from(project_id)];
        for id in &new_ids {
            params.push(rusqlite::types::ToSqlOutput::from(id.to_string()));
        }
        tx.execute(&sql, rusqlite::params_from_iter(params.iter()))
            .map_err(|e| AppError::Database(e.to_string()))?;
    }

    // Upsert each line
    for line in lines {
        tx.execute(
            "INSERT INTO script_lines (id, project_id, line_order, text, character_id, gap_after_ms, instructions, section_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(id) DO UPDATE SET
               line_order = excluded.line_order,
               text = excluded.text,
               character_id = CASE
                 WHEN excluded.character_id IS NOT NULL THEN excluded.character_id
                 ELSE script_lines.character_id
               END,
               gap_after_ms = excluded.gap_after_ms,
               instructions = excluded.instructions,
               section_id = excluded.section_id,
               updated_at = datetime('now')",
            rusqlite::params![
                line.id,
                project_id,
                line.line_order,
                line.text,
                line.character_id,
                line.gap_after_ms,
                line.instructions,
                line.section_id
            ],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
    }

    tx.commit()
        .map_err(|e| AppError::Database(e.to_string()))?;

    Ok(())
}

/// Load all script lines for a project, ordered by section_order then line_order ASC.
pub fn load_script(conn: &Connection, project_id: &str) -> Result<Vec<ScriptLine>, AppError> {
    let mut stmt = conn
        .prepare("SELECT sl.id, sl.project_id, sl.line_order, sl.text, sl.character_id, sl.gap_after_ms, sl.instructions, sl.section_id FROM script_lines sl LEFT JOIN script_sections ss ON sl.section_id = ss.id WHERE sl.project_id = ?1 ORDER BY COALESCE(ss.section_order, 999999), sl.line_order ASC")
        .map_err(|e| AppError::Database(e.to_string()))?;

    let lines = stmt
        .query_map(rusqlite::params![project_id], |row| {
            Ok(ScriptLine {
                id: row.get(0)?,
                project_id: row.get(1)?,
                line_order: row.get(2)?,
                text: row.get(3)?,
                character_id: row.get(4)?,
                gap_after_ms: row.get(5)?,
                instructions: row.get(6)?,
                section_id: row.get(7)?,
            })
        })
        .map_err(|e| AppError::Database(e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| AppError::Database(e.to_string()))?;

    Ok(lines)
}

// ---- Script Sections operations ----

/// List all sections for a project, ordered by section_order ASC.
pub fn list_sections(conn: &Connection, project_id: &str) -> Result<Vec<ScriptSection>, AppError> {
    let mut stmt = conn
        .prepare("SELECT id, project_id, title, section_order FROM script_sections WHERE project_id = ?1 ORDER BY section_order ASC")
        .map_err(|e| AppError::Database(e.to_string()))?;

    let sections = stmt
        .query_map(rusqlite::params![project_id], |row| {
            Ok(ScriptSection {
                id: row.get(0)?,
                project_id: row.get(1)?,
                title: row.get(2)?,
                section_order: row.get(3)?,
            })
        })
        .map_err(|e| AppError::Database(e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| AppError::Database(e.to_string()))?;

    Ok(sections)
}

/// Delete all sections for a project (lines' section_id will be set to NULL via ON DELETE SET NULL).
pub fn delete_sections(conn: &Connection, project_id: &str) -> Result<(), AppError> {
    conn.execute(
        "DELETE FROM script_sections WHERE project_id = ?1",
        rusqlite::params![project_id],
    )
    .map_err(|e| AppError::Database(e.to_string()))?;
    Ok(())
}

/// Save sections for a project. Deletes sections no longer in the set, upserts the rest.
pub fn save_sections(
    conn: &Connection,
    project_id: &str,
    sections: &[ScriptSection],
) -> Result<(), AppError> {
    let tx = conn
        .unchecked_transaction()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let new_ids: Vec<&str> = sections.iter().map(|s| s.id.as_str()).collect();

    if new_ids.is_empty() {
        tx.execute(
            "DELETE FROM script_sections WHERE project_id = ?1",
            rusqlite::params![project_id],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
    } else {
        // Build dynamic parameterized placeholders for NOT IN clause
        let placeholders: Vec<String> = (1..=new_ids.len())
            .map(|i| format!("?{}", i + 1))
            .collect();
        let sql = format!(
            "DELETE FROM script_sections WHERE project_id = ?1 AND id NOT IN ({})",
            placeholders.join(",")
        );
        let mut params: Vec<rusqlite::types::ToSqlOutput> =
            vec![rusqlite::types::ToSqlOutput::from(project_id)];
        for id in &new_ids {
            params.push(rusqlite::types::ToSqlOutput::from(id.to_string()));
        }
        tx.execute(&sql, rusqlite::params_from_iter(params.iter()))
            .map_err(|e| AppError::Database(e.to_string()))?;
    }

    for section in sections {
        tx.execute(
            "INSERT INTO script_sections (id, project_id, title, section_order)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(id) DO UPDATE SET
               title = excluded.title,
               section_order = excluded.section_order",
            rusqlite::params![section.id, project_id, section.title, section.section_order],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
    }

    tx.commit()
        .map_err(|e| AppError::Database(e.to_string()))?;

    Ok(())
}

// ---- CLI query helpers ----

/// Load all lines for a project, optionally with character/section info.
pub fn load_script_lines(conn: &Connection, project_id: &str) -> Result<Vec<ScriptLineWithMeta>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT sl.id, sl.project_id, sl.line_order, sl.text, sl.character_id, sl.gap_after_ms, sl.instructions, sl.section_id, \
         c.name as character_name, ss.title as section_title \
         FROM script_lines sl \
         LEFT JOIN characters c ON sl.character_id = c.id \
         LEFT JOIN script_sections ss ON sl.section_id = ss.id \
         WHERE sl.project_id = ?1 \
         ORDER BY sl.line_order"
    ).map_err(|e| AppError::Database(e.to_string()))?;
    let rows = stmt.query_map(rusqlite::params![project_id], |row| {
        Ok(ScriptLineWithMeta {
            id: row.get(0)?,
            project_id: row.get(1)?,
            line_order: row.get(2)?,
            text: row.get(3)?,
            character_id: row.get(4)?,
            gap_after_ms: row.get(5)?,
            instructions: row.get(6)?,
            section_id: row.get(7)?,
            character_name: row.get(8)?,
            section_title: row.get(9)?,
        })
    }).map_err(|e| AppError::Database(e.to_string()))?;
    rows.collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| AppError::Database(e.to_string()))
}
