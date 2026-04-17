//! Project CRUD operations.

use rusqlite::Connection;

use super::super::error::AppError;
use super::super::models::Project;

/// Insert a new project into the database.
pub fn insert_project(conn: &Connection, project: &Project) -> Result<(), AppError> {
    conn.execute(
        "INSERT INTO projects (id, name, outline, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![project.id, project.name, project.outline, project.created_at, project.updated_at],
    )
    .map_err(|e| AppError::Database(e.to_string()))?;
    Ok(())
}

/// List all projects ordered by creation time (newest first).
pub fn list_projects(conn: &Connection) -> Result<Vec<Project>, AppError> {
    let mut stmt = conn
        .prepare("SELECT id, name, outline, created_at, updated_at FROM projects ORDER BY created_at DESC")
        .map_err(|e| AppError::Database(e.to_string()))?;

    let projects = stmt
        .query_map([], |row| {
            Ok(Project {
                id: row.get(0)?,
                name: row.get(1)?,
                outline: row.get(2)?,
                created_at: row.get(3)?,
                updated_at: row.get(4)?,
            })
        })
        .map_err(|e| AppError::Database(e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| AppError::Database(e.to_string()))?;

    Ok(projects)
}

/// Get a single project by ID.
pub fn get_project(conn: &Connection, id: &str) -> Result<Project, AppError> {
    conn.query_row(
        "SELECT id, name, outline, created_at, updated_at FROM projects WHERE id = ?1",
        rusqlite::params![id],
        |row| {
            Ok(Project {
                id: row.get(0)?,
                name: row.get(1)?,
                outline: row.get(2)?,
                created_at: row.get(3)?,
                updated_at: row.get(4)?,
            })
        },
    )
    .map_err(|e| AppError::Database(e.to_string()))
}

/// Delete a project by ID. Cascade delete is handled by the SQL schema (ON DELETE CASCADE).
pub fn delete_project(conn: &Connection, id: &str) -> Result<(), AppError> {
    let affected = conn
        .execute("DELETE FROM projects WHERE id = ?1", rusqlite::params![id])
        .map_err(|e| AppError::Database(e.to_string()))?;

    if affected == 0 {
        return Err(AppError::Database(format!("Project not found: {}", id)));
    }
    Ok(())
}

/// Save only the outline text for a project.
pub fn save_project_outline(conn: &Connection, id: &str, outline: &str) -> Result<(), AppError> {
    conn.execute(
        "UPDATE projects SET outline = ?1, updated_at = datetime('now') WHERE id = ?2",
        rusqlite::params![outline, id],
    )
    .map_err(|e| AppError::Database(e.to_string()))?;
    Ok(())
}
