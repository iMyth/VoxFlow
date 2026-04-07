use std::sync::Mutex;

use tauri::Manager;

use crate::core::db::Database;
use crate::core::error::AppError;
use crate::core::models::{Project, ProjectDetail};

#[tauri::command]
pub fn create_project(
    db: tauri::State<'_, Mutex<Database>>,
    app: tauri::AppHandle,
    name: String,
) -> Result<Project, AppError> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

    let project = Project {
        id: id.clone(),
        name,
        outline: String::new(),
        created_at: now.clone(),
        updated_at: now,
    };

    // Insert into database
    let db = db.lock().map_err(|e| AppError::Database(e.to_string()))?;
    db.insert_project(&project)?;

    // Create project directories on the filesystem
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::FileSystem(e.to_string()))?;

    let project_base = app_data_dir.join("projects").join(&id);
    let subdirs = ["audio", "bgm", "export"];

    for subdir in &subdirs {
        let dir_path = project_base.join(subdir);
        std::fs::create_dir_all(&dir_path).map_err(|e| {
            AppError::FileSystem(format!(
                "Failed to create directory {}: {}",
                dir_path.display(),
                e
            ))
        })?;
    }

    Ok(project)
}

#[tauri::command]
pub fn list_projects(
    db: tauri::State<'_, Mutex<Database>>,
) -> Result<Vec<Project>, AppError> {
    let db = db.lock().map_err(|e| AppError::Database(e.to_string()))?;
    db.list_projects()
}

#[tauri::command]
pub fn load_project(
    db: tauri::State<'_, Mutex<Database>>,
    project_id: String,
) -> Result<ProjectDetail, AppError> {
    let db = db.lock().map_err(|e| AppError::Database(e.to_string()))?;
    db.load_project(&project_id)
}

#[tauri::command]
pub fn delete_project(
    db: tauri::State<'_, Mutex<Database>>,
    app: tauri::AppHandle,
    project_id: String,
) -> Result<(), AppError> {
    // Delete from database first
    let db = db.lock().map_err(|e| AppError::Database(e.to_string()))?;
    db.delete_project(&project_id)?;

    // Remove project directory from filesystem
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::FileSystem(e.to_string()))?;

    let project_dir = app_data_dir.join("projects").join(&project_id);
    if project_dir.exists() {
        std::fs::remove_dir_all(&project_dir).map_err(|e| {
            AppError::FileSystem(format!(
                "Failed to remove directory {}: {}",
                project_dir.display(),
                e
            ))
        })?;
    }

    Ok(())
}

#[tauri::command]
pub fn save_outline(
    db: tauri::State<'_, Mutex<Database>>,
    project_id: String,
    outline: String,
) -> Result<(), AppError> {
    let db = db.lock().map_err(|e| AppError::Database(e.to_string()))?;
    db.save_project_outline(&project_id, &outline)
}
