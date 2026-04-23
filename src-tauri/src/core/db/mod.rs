//! Database module for VoxFlow.
//!
//! This module provides database operations organized by domain:
//! - `schema`: Schema migrations
//! - `project`: Project CRUD operations
//! - `character`: Character CRUD operations
//! - `script`: Script lines and sections operations
//! - `audio`: Audio fragment and BGM operations
//! - `settings`: User settings operations
//! - `story_kb`: Story knowledge base operations

use std::path::Path;

use rusqlite::Connection;

use super::error::AppError;
use super::models::{AudioFragment, Character, Project, ProjectDetail, ProjectStats, ScriptLine, ScriptSection, StoryKnowledgeItem, UserSettings};

pub use audio::{clear_audio_fragments, clear_tts_fragments, insert_bgm, list_audio_fragments, upsert_audio_fragment};
pub use character::{
    delete_character, get_character_by_id, get_character_project_id, insert_character, list_all_project_characters,
    list_characters, update_character,
};
pub use project::{delete_project, get_project, insert_project, list_projects, save_project_outline};
pub use script::{delete_sections, list_sections, load_script, load_script_lines, save_script, save_sections, ScriptLineWithMeta};
pub use settings::{load_settings, save_settings};
pub use story_kb::{delete_all_story_kb, delete_story_kb, insert_story_kb, list_story_kb, load_script_with_sections};

mod audio;
mod character;
mod project;
mod script;
mod schema;
mod settings;
mod story_kb;
#[cfg(test)]
mod tests;

/// Database wrapper providing all database operations.
pub struct Database {
    conn: Connection,
}

impl Database {
    /// Open (or create) a SQLite database at the given path and enable foreign keys.
    pub fn open(path: &Path) -> Result<Self, AppError> {
        let conn = Connection::open(path).map_err(|e| AppError::Database(e.to_string()))?;

        conn.execute_batch("PRAGMA foreign_keys = ON;")
            .map_err(|e| AppError::Database(e.to_string()))?;

        Ok(Self { conn })
    }

    /// Run schema migrations.
    pub fn migrate(&self) -> Result<(), AppError> {
        schema::migrate(&self.conn)
    }

    // ---- Project CRUD ----

    /// Insert a new project into the database.
    pub fn insert_project(&self, project: &Project) -> Result<(), AppError> {
        project::insert_project(&self.conn, project)
    }

    /// List all projects ordered by creation time (newest first).
    pub fn list_projects(&self) -> Result<Vec<Project>, AppError> {
        project::list_projects(&self.conn)
    }

    /// List all projects with aggregate stats (line count, audio count, character count).
    pub fn list_projects_with_stats(&self) -> Result<Vec<ProjectStats>, AppError> {
        let projects = project::list_projects(&self.conn)?;
        let mut stats = Vec::new();
        for p in &projects {
            let line_count: i32 = self
                .conn
                .query_row(
                    "SELECT COUNT(*) FROM script_lines WHERE project_id = ?",
                    rusqlite::params![p.id],
                    |row| row.get(0),
                )
                .unwrap_or(0);
            let audio_count: i32 = self
                .conn
                .query_row(
                    "SELECT COUNT(*) FROM audio_fragments WHERE project_id = ?",
                    rusqlite::params![p.id],
                    |row| row.get(0),
                )
                .unwrap_or(0);
            let char_count: i32 = self
                .conn
                .query_row(
                    "SELECT COUNT(*) FROM characters WHERE project_id = ?",
                    rusqlite::params![p.id],
                    |row| row.get(0),
                )
                .unwrap_or(0);
            stats.push(ProjectStats {
                id: p.id.clone(),
                name: p.name.clone(),
                created_at: p.created_at.clone(),
                line_count,
                audio_count,
                character_count: char_count,
            });
        }
        Ok(stats)
    }

    /// Get a single project by ID.
    pub fn get_project(&self, id: &str) -> Result<Project, AppError> {
        project::get_project(&self.conn, id)
    }

    /// Delete a project by ID. Cascade delete is handled by the SQL schema (ON DELETE CASCADE).
    pub fn delete_project(&self, id: &str) -> Result<(), AppError> {
        project::delete_project(&self.conn, id)
    }

    /// Save only the outline text for a project.
    pub fn save_project_outline(&self, id: &str, outline: &str) -> Result<(), AppError> {
        project::save_project_outline(&self.conn, id, outline)
    }

    // ---- Character CRUD ----

    /// Insert a new character into the database.
    pub fn insert_character(&self, character: &Character) -> Result<(), AppError> {
        character::insert_character(&self.conn, character)
    }

    /// Update an existing character's fields.
    pub fn update_character(&self, character: &Character) -> Result<(), AppError> {
        character::update_character(&self.conn, character)
    }

    /// Get the project_id for a character by its ID.
    pub fn get_character_project_id(&self, character_id: &str) -> Result<String, AppError> {
        character::get_character_project_id(&self.conn, character_id)
    }

    /// Delete a character by ID.
    pub fn delete_character(&self, id: &str) -> Result<(), AppError> {
        character::delete_character(&self.conn, id)
    }

    /// List all characters for a given project.
    pub fn list_characters(&self, project_id: &str) -> Result<Vec<Character>, AppError> {
        character::list_characters(&self.conn, project_id)
    }

    /// List all characters across all projects, grouped by (project_id, characters).
    pub fn list_all_project_characters(&self) -> Result<Vec<(String, String, Vec<Character>)>, AppError> {
        character::list_all_project_characters(&self.conn)
    }

    /// Get a single character by ID.
    pub fn get_character_by_id(&self, character_id: &str) -> Result<Character, AppError> {
        character::get_character_by_id(&self.conn, character_id)
    }

    // ---- Script Line operations ----

    /// Save script lines for a project using upsert to avoid cascade-deleting audio fragments.
    pub fn save_script(&self, project_id: &str, lines: &[ScriptLine], sections: &[ScriptSection]) -> Result<(), AppError> {
        script::save_script(&self.conn, project_id, lines, sections)
    }

    /// Load all script lines for a project, ordered by section_order then line_order ASC.
    pub fn load_script(&self, project_id: &str) -> Result<Vec<ScriptLine>, AppError> {
        script::load_script(&self.conn, project_id)
    }

    // ---- Script Sections operations ----

    /// List all sections for a project, ordered by section_order ASC.
    pub fn list_sections(&self, project_id: &str) -> Result<Vec<ScriptSection>, AppError> {
        script::list_sections(&self.conn, project_id)
    }

    /// Delete all sections for a project.
    pub fn delete_sections(&self, project_id: &str) -> Result<(), AppError> {
        script::delete_sections(&self.conn, project_id)
    }

    /// Save sections for a project.
    pub fn save_sections(&self, project_id: &str, sections: &[ScriptSection]) -> Result<(), AppError> {
        script::save_sections(&self.conn, project_id, sections)
    }

    // ---- Audio Fragment operations ----

    /// Insert or update an audio fragment.
    pub fn upsert_audio_fragment(&self, fragment: &AudioFragment) -> Result<(), AppError> {
        audio::upsert_audio_fragment(&self.conn, fragment)
    }

    /// Delete all audio fragments for a given project and return their file paths.
    pub fn clear_audio_fragments(&self, project_id: &str) -> Result<Vec<String>, AppError> {
        audio::clear_audio_fragments(&self.conn, project_id)
    }

    /// Delete only TTS-source audio fragments for a given project.
    pub fn clear_tts_fragments(&self, project_id: &str) -> Result<Vec<String>, AppError> {
        audio::clear_tts_fragments(&self.conn, project_id)
    }

    /// List all audio fragments for a given project.
    pub fn list_audio_fragments(&self, project_id: &str) -> Result<Vec<AudioFragment>, AppError> {
        audio::list_audio_fragments(&self.conn, project_id)
    }

    // ---- User Settings operations ----

    /// Save user settings.
    pub fn save_settings(&self, settings: &UserSettings) -> Result<(), AppError> {
        settings::save_settings(&self.conn, settings)
    }

    /// Load user settings from the database.
    pub fn load_settings(&self) -> Result<UserSettings, AppError> {
        settings::load_settings(&self.conn)
    }

    // ---- Aggregate load ----

    /// Load a complete project with all associated characters, sections, script lines, and audio fragments.
    pub fn load_project(&self, project_id: &str) -> Result<ProjectDetail, AppError> {
        let project = self.get_project(project_id)?;
        let characters = self.list_characters(project_id)?;
        let sections = self.list_sections(project_id)?;
        let script_lines = self.load_script(project_id)?;
        let audio_fragments = self.list_audio_fragments(project_id)?;

        Ok(ProjectDetail {
            project,
            characters,
            sections,
            script_lines,
            audio_fragments,
        })
    }

    // ---- BGM operations ----

    /// Insert a BGM file record into the database.
    pub fn insert_bgm(&self, id: &str, project_id: &str, file_path: &str, name: &str) -> Result<(), AppError> {
        audio::insert_bgm(&self.conn, id, project_id, file_path, name)
    }

    // ---- CLI query helpers ----

    /// List all projects with their line counts (for CLI).
    pub fn list_project_stats(&self) -> Result<Vec<(String, i32)>, AppError> {
        let mut stmt = self
            .conn
            .prepare("SELECT project_id, COUNT(*) as line_count FROM script_lines GROUP BY project_id ORDER BY project_id")
            .map_err(|e| AppError::Database(e.to_string()))?;
        let rows = stmt
            .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, i32>(1)?)))
            .map_err(|e| AppError::Database(e.to_string()))?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| AppError::Database(e.to_string()))
    }

    /// Load all lines for a project, optionally with character/section info.
    pub fn load_script_lines(&self, project_id: &str) -> Result<Vec<ScriptLineWithMeta>, AppError> {
        script::load_script_lines(&self.conn, project_id)
    }

    /// Load all characters for a project.
    pub fn load_characters(&self, project_id: &str) -> Result<Vec<Character>, AppError> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, name, project_id, voice_name, tts_model, speed, pitch FROM characters WHERE project_id = ?1 ORDER BY name")
            .map_err(|e| AppError::Database(e.to_string()))?;
        let rows = stmt
            .query_map(rusqlite::params![project_id], |row| {
                Ok(Character {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    project_id: row.get(2)?,
                    voice_name: row.get(3)?,
                    tts_model: row.get(4)?,
                    speed: row.get(5).unwrap_or(1.0),
                    pitch: row.get(6).unwrap_or(1.0),
                })
            })
            .map_err(|e| AppError::Database(e.to_string()))?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| AppError::Database(e.to_string()))
    }

    // ---- Story Knowledge Base (vector search) ----

    /// Insert a knowledge item into the story vector DB.
    pub fn insert_story_kb(&self, item: &StoryKnowledgeItem) -> Result<(), AppError> {
        story_kb::insert_story_kb(&self.conn, item)
    }

    /// Delete a knowledge item.
    pub fn delete_story_kb(&self, id: &str) -> Result<(), AppError> {
        story_kb::delete_story_kb(&self.conn, id)
    }

    /// List all knowledge items for a project.
    pub fn list_story_kb(&self, project_id: &str, kb_type: Option<&str>) -> Result<Vec<StoryKnowledgeItem>, AppError> {
        story_kb::list_story_kb(&self.conn, project_id, kb_type)
    }

    /// Bulk delete all story_kb items for a project.
    pub fn delete_all_story_kb(&self, project_id: &str) -> Result<(), AppError> {
        story_kb::delete_all_story_kb(&self.conn, project_id)
    }

    /// Load script sections and lines together (for KB indexing).
    pub fn load_script_with_sections(&self, project_id: &str) -> Result<(Vec<ScriptSection>, Vec<ScriptLine>), AppError> {
        story_kb::load_script_with_sections(&self.conn, project_id)
    }
}
