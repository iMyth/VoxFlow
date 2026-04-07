use std::sync::Mutex;

use crate::core::db::Database;
use crate::core::error::AppError;
use crate::core::models::{Character, CharacterInput};

#[tauri::command]
pub fn create_character(
    db: tauri::State<'_, Mutex<Database>>,
    project_id: String,
    input: CharacterInput,
) -> Result<Character, AppError> {
    let id = uuid::Uuid::new_v4().to_string();

    let character = Character {
        id,
        project_id,
        name: input.name,
        tts_model: input.tts_model,
        voice_name: input.voice_name,
        speed: input.speed,
        pitch: input.pitch,
    };

    let db = db.lock().map_err(|e| AppError::Database(e.to_string()))?;
    db.insert_character(&character)?;

    Ok(character)
}

#[tauri::command]
pub fn update_character(
    db: tauri::State<'_, Mutex<Database>>,
    character_id: String,
    input: CharacterInput,
) -> Result<Character, AppError> {
    let db = db.lock().map_err(|e| AppError::Database(e.to_string()))?;

    let project_id = db.get_character_project_id(&character_id)?;

    let character = Character {
        id: character_id,
        project_id,
        name: input.name,
        tts_model: input.tts_model,
        voice_name: input.voice_name,
        speed: input.speed,
        pitch: input.pitch,
    };

    db.update_character(&character)?;

    Ok(character)
}

#[tauri::command]
pub fn delete_character(
    db: tauri::State<'_, Mutex<Database>>,
    character_id: String,
) -> Result<(), AppError> {
    let db = db.lock().map_err(|e| AppError::Database(e.to_string()))?;
    db.delete_character(&character_id)
}

#[tauri::command]
pub fn list_characters(
    db: tauri::State<'_, Mutex<Database>>,
    project_id: String,
) -> Result<Vec<Character>, AppError> {
    let db = db.lock().map_err(|e| AppError::Database(e.to_string()))?;
    db.list_characters(&project_id)
}

/// List all characters across all projects, grouped by project.
#[tauri::command]
pub fn list_all_project_characters(
    db: tauri::State<'_, Mutex<Database>>,
) -> Result<Vec<(String, String, Vec<Character>)>, AppError> {
    let db = db.lock().map_err(|e| AppError::Database(e.to_string()))?;
    db.list_all_project_characters()
}

/// Import characters from one project to another.
#[tauri::command]
pub fn import_characters(
    db: tauri::State<'_, Mutex<Database>>,
    to_project_id: String,
    character_ids: Vec<String>,
) -> Result<Vec<Character>, AppError> {
    let db = db.lock().map_err(|e| AppError::Database(e.to_string()))?;
    let mut imported = Vec::new();

    for char_id in &character_ids {
        let source = db.get_character_by_id(char_id)?;
        let new_id = uuid::Uuid::new_v4().to_string();
        let character = crate::core::models::Character {
            id: new_id,
            project_id: to_project_id.clone(),
            name: source.name.clone(),
            tts_model: source.tts_model.clone(),
            voice_name: source.voice_name.clone(),
            speed: source.speed,
            pitch: source.pitch,
        };
        db.insert_character(&character)?;
        imported.push(character);
    }

    Ok(imported)
}
