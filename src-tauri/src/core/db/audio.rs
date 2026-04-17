//! Audio fragment and BGM operations.

use rusqlite::Connection;

use super::super::error::AppError;
use super::super::models::AudioFragment;

// ---- Audio Fragment operations ----

/// Insert or update an audio fragment. If a fragment already exists for the same line_id,
/// it is deleted first, then the new fragment is inserted.
pub fn upsert_audio_fragment(conn: &Connection, fragment: &AudioFragment) -> Result<(), AppError> {
    let tx = conn
        .unchecked_transaction()
        .map_err(|e| AppError::Database(e.to_string()))?;

    tx.execute(
        "DELETE FROM audio_fragments WHERE line_id = ?1",
        rusqlite::params![fragment.line_id],
    )
    .map_err(|e| AppError::Database(e.to_string()))?;

    tx.execute(
        "INSERT INTO audio_fragments (id, project_id, line_id, file_path, duration_ms, source) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![
            fragment.id,
            fragment.project_id,
            fragment.line_id,
            fragment.file_path,
            fragment.duration_ms,
            fragment.source,
        ],
    )
    .map_err(|e| AppError::Database(e.to_string()))?;

    tx.commit()
        .map_err(|e| AppError::Database(e.to_string()))?;

    Ok(())
}

/// Delete all audio fragments for a given project and return their file paths.
pub fn clear_audio_fragments(conn: &Connection, project_id: &str) -> Result<Vec<String>, AppError> {
    let mut stmt = conn
        .prepare("SELECT file_path FROM audio_fragments WHERE project_id = ?1")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let paths: Vec<String> = stmt
        .query_map(rusqlite::params![project_id], |row| row.get(0))
        .map_err(|e| AppError::Database(e.to_string()))?
        .filter_map(|r| r.ok())
        .collect();
    conn.execute(
        "DELETE FROM audio_fragments WHERE project_id = ?1",
        rusqlite::params![project_id],
    )
    .map_err(|e| AppError::Database(e.to_string()))?;
    Ok(paths)
}

/// Delete only TTS-source audio fragments for a given project, returning their file paths.
pub fn clear_tts_fragments(conn: &Connection, project_id: &str) -> Result<Vec<String>, AppError> {
    let mut stmt = conn
        .prepare("SELECT file_path FROM audio_fragments WHERE project_id = ?1 AND source = 'tts'")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let paths: Vec<String> = stmt
        .query_map(rusqlite::params![project_id], |row| row.get(0))
        .map_err(|e| AppError::Database(e.to_string()))?
        .filter_map(|r| r.ok())
        .collect();
    conn.execute(
        "DELETE FROM audio_fragments WHERE project_id = ?1 AND source = 'tts'",
        rusqlite::params![project_id],
    )
    .map_err(|e| AppError::Database(e.to_string()))?;
    Ok(paths)
}

/// List all audio fragments for a given project.
pub fn list_audio_fragments(conn: &Connection, project_id: &str) -> Result<Vec<AudioFragment>, AppError> {
    let mut stmt = conn
        .prepare("SELECT id, project_id, line_id, file_path, duration_ms, source FROM audio_fragments WHERE project_id = ?1")
        .map_err(|e| AppError::Database(e.to_string()))?;

    let fragments = stmt
        .query_map(rusqlite::params![project_id], |row| {
            Ok(AudioFragment {
                id: row.get(0)?,
                project_id: row.get(1)?,
                line_id: row.get(2)?,
                file_path: row.get(3)?,
                duration_ms: row.get(4)?,
                source: row.get(5)?,
            })
        })
        .map_err(|e| AppError::Database(e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| AppError::Database(e.to_string()))?;

    Ok(fragments)
}

// ---- BGM operations ----

/// Insert a BGM file record into the database.
pub fn insert_bgm(
    conn: &Connection,
    id: &str,
    project_id: &str,
    file_path: &str,
    name: &str,
) -> Result<(), AppError> {
    conn.execute(
        "INSERT INTO bgm_files (id, project_id, file_path, name) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![id, project_id, file_path, name],
    )
    .map_err(|e| AppError::Database(e.to_string()))?;
    Ok(())
}
