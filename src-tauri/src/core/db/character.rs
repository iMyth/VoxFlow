//! Character CRUD operations.

use std::collections::HashMap;

use rusqlite::Connection;

use super::super::error::AppError;
use super::super::models::Character;

/// Insert a new character into the database.
pub fn insert_character(conn: &Connection, character: &Character) -> Result<(), AppError> {
    conn.execute(
        "INSERT INTO characters (id, project_id, name, tts_model, voice_name, speed, pitch) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![
            character.id,
            character.project_id,
            character.name,
            character.tts_model,
            character.voice_name,
            character.speed,
            character.pitch,
        ],
    )
    .map_err(|e| AppError::Database(e.to_string()))?;
    Ok(())
}

/// Update an existing character's fields.
pub fn update_character(conn: &Connection, character: &Character) -> Result<(), AppError> {
    let affected = conn
        .execute(
            "UPDATE characters SET name = ?1, tts_model = ?2, voice_name = ?3, speed = ?4, pitch = ?5 WHERE id = ?6",
            rusqlite::params![
                character.name,
                character.tts_model,
                character.voice_name,
                character.speed,
                character.pitch,
                character.id,
            ],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;

    if affected == 0 {
        return Err(AppError::Database(format!(
            "Character not found: {}",
            character.id
        )));
    }
    Ok(())
}

/// Get the project_id for a character by its ID.
pub fn get_character_project_id(conn: &Connection, character_id: &str) -> Result<String, AppError> {
    conn.query_row(
        "SELECT project_id FROM characters WHERE id = ?1",
        rusqlite::params![character_id],
        |row| row.get(0),
    )
    .map_err(|e| AppError::Database(e.to_string()))
}

/// Delete a character by ID.
pub fn delete_character(conn: &Connection, id: &str) -> Result<(), AppError> {
    let affected = conn
        .execute("DELETE FROM characters WHERE id = ?1", rusqlite::params![id])
        .map_err(|e| AppError::Database(e.to_string()))?;

    if affected == 0 {
        return Err(AppError::Database(format!("Character not found: {}", id)));
    }
    Ok(())
}

/// List all characters for a given project.
pub fn list_characters(conn: &Connection, project_id: &str) -> Result<Vec<Character>, AppError> {
    let mut stmt = conn
        .prepare("SELECT id, project_id, name, tts_model, voice_name, speed, pitch FROM characters WHERE project_id = ?1")
        .map_err(|e| AppError::Database(e.to_string()))?;

    let characters = stmt
        .query_map(rusqlite::params![project_id], |row| {
            Ok(Character {
                id: row.get(0)?,
                project_id: row.get(1)?,
                name: row.get(2)?,
                tts_model: row.get(3)?,
                voice_name: row.get(4)?,
                speed: row.get(5)?,
                pitch: row.get(6)?,
            })
        })
        .map_err(|e| AppError::Database(e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| AppError::Database(e.to_string()))?;

    Ok(characters)
}

/// List all characters across all projects, grouped by (project_id, characters).
pub fn list_all_project_characters(
    conn: &Connection,
) -> Result<Vec<(String, String, Vec<Character>)>, AppError> {
    let mut stmt = conn
        .prepare("SELECT c.id, c.project_id, p.name, c.name, c.tts_model, c.voice_name, c.speed, c.pitch FROM characters c JOIN projects p ON c.project_id = p.id ORDER BY p.name, c.name")
        .map_err(|e| AppError::Database(e.to_string()))?;

    let characters = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                crate::core::models::Character {
                    id: row.get(0)?,
                    project_id: row.get(1)?,
                    name: row.get(3)?,
                    tts_model: row.get(4)?,
                    voice_name: row.get(5)?,
                    speed: row.get(6)?,
                    pitch: row.get(7)?,
                },
            ))
        })
        .map_err(|e| AppError::Database(e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| AppError::Database(e.to_string()))?;

    // Group by project_id, keeping project_name
    let mut grouped: HashMap<String, (String, Vec<crate::core::models::Character>)> =
        HashMap::new();
    for (_, project_id, project_name, c) in characters {
        grouped
            .entry(project_id.clone())
            .or_insert((project_name, Vec::new()))
            .1
            .push(c);
    }

    Ok(grouped
        .into_iter()
        .map(|(project_id, (project_name, chars))| (project_id, project_name, chars))
        .collect())
}

/// Get a single character by ID.
pub fn get_character_by_id(conn: &Connection, character_id: &str) -> Result<Character, AppError> {
    let mut stmt = conn
        .prepare("SELECT id, project_id, name, tts_model, voice_name, speed, pitch FROM characters WHERE id = ?1")
        .map_err(|e| AppError::Database(e.to_string()))?;

    stmt.query_row(rusqlite::params![character_id], |row| {
        Ok(Character {
            id: row.get(0)?,
            project_id: row.get(1)?,
            name: row.get(2)?,
            tts_model: row.get(3)?,
            voice_name: row.get(4)?,
            speed: row.get(5)?,
            pitch: row.get(6)?,
        })
    })
    .map_err(|e| AppError::Database(e.to_string()))
}
