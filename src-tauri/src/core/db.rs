use std::path::Path;

use rusqlite::Connection;

use super::error::AppError;
use super::models::{AudioFragment, Character, Project, ProjectDetail, ScriptLine, TtsEngine, UserSettings};

pub struct Database {
    conn: Connection,
}

impl Database {
    /// Open (or create) a SQLite database at the given path and enable foreign keys.
    pub fn open(path: &Path) -> Result<Self, AppError> {
        let conn = Connection::open(path)
            .map_err(|e| AppError::Database(e.to_string()))?;

        conn.execute_batch("PRAGMA foreign_keys = ON;")
            .map_err(|e| AppError::Database(e.to_string()))?;

        Ok(Self { conn })
    }

    /// Run schema migrations – creates all 6 tables if they don't already exist.
    pub fn migrate(&self) -> Result<(), AppError> {
        self.conn
            .execute_batch(
                "
                CREATE TABLE IF NOT EXISTS projects (
                    id          TEXT PRIMARY KEY,
                    name        TEXT NOT NULL,
                    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
                    updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
                );

                CREATE TABLE IF NOT EXISTS characters (
                    id          TEXT PRIMARY KEY,
                    project_id  TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
                    name        TEXT NOT NULL,
                    tts_engine  TEXT NOT NULL DEFAULT 'edge-tts',
                    voice_name  TEXT NOT NULL,
                    speed       REAL NOT NULL DEFAULT 1.0,
                    pitch       REAL NOT NULL DEFAULT 1.0,
                    created_at  TEXT NOT NULL DEFAULT (datetime('now'))
                );

                CREATE TABLE IF NOT EXISTS script_lines (
                    id            TEXT PRIMARY KEY,
                    project_id    TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
                    line_order    INTEGER NOT NULL,
                    text          TEXT NOT NULL,
                    character_id  TEXT REFERENCES characters(id) ON DELETE SET NULL,
                    updated_at    TEXT NOT NULL DEFAULT (datetime('now'))
                );

                CREATE TABLE IF NOT EXISTS audio_fragments (
                    id          TEXT PRIMARY KEY,
                    project_id  TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
                    line_id     TEXT NOT NULL REFERENCES script_lines(id) ON DELETE CASCADE,
                    file_path   TEXT NOT NULL,
                    duration_ms INTEGER,
                    created_at  TEXT NOT NULL DEFAULT (datetime('now'))
                );

                CREATE TABLE IF NOT EXISTS bgm_files (
                    id          TEXT PRIMARY KEY,
                    project_id  TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
                    file_path   TEXT NOT NULL,
                    name        TEXT NOT NULL,
                    created_at  TEXT NOT NULL DEFAULT (datetime('now'))
                );

                CREATE TABLE IF NOT EXISTS user_settings (
                    key   TEXT PRIMARY KEY,
                    value TEXT NOT NULL
                );
                ",
            )
            .map_err(|e| AppError::Database(e.to_string()))?;

        Ok(())
    }

    // ---- Project CRUD ----

    /// Insert a new project into the database.
    pub fn insert_project(&self, project: &Project) -> Result<(), AppError> {
        self.conn
            .execute(
                "INSERT INTO projects (id, name, created_at, updated_at) VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![project.id, project.name, project.created_at, project.updated_at],
            )
            .map_err(|e| AppError::Database(e.to_string()))?;
        Ok(())
    }

    /// List all projects ordered by creation time (newest first).
    pub fn list_projects(&self) -> Result<Vec<Project>, AppError> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, name, created_at, updated_at FROM projects ORDER BY created_at DESC")
            .map_err(|e| AppError::Database(e.to_string()))?;

        let projects = stmt
            .query_map([], |row| {
                Ok(Project {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    created_at: row.get(2)?,
                    updated_at: row.get(3)?,
                })
            })
            .map_err(|e| AppError::Database(e.to_string()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| AppError::Database(e.to_string()))?;

        Ok(projects)
    }

    /// Get a single project by ID.
    pub fn get_project(&self, id: &str) -> Result<Project, AppError> {
        self.conn
            .query_row(
                "SELECT id, name, created_at, updated_at FROM projects WHERE id = ?1",
                rusqlite::params![id],
                |row| {
                    Ok(Project {
                        id: row.get(0)?,
                        name: row.get(1)?,
                        created_at: row.get(2)?,
                        updated_at: row.get(3)?,
                    })
                },
            )
            .map_err(|e| AppError::Database(e.to_string()))
    }

    /// Delete a project by ID. Cascade delete is handled by the SQL schema (ON DELETE CASCADE).
    pub fn delete_project(&self, id: &str) -> Result<(), AppError> {
        let affected = self
            .conn
            .execute("DELETE FROM projects WHERE id = ?1", rusqlite::params![id])
            .map_err(|e| AppError::Database(e.to_string()))?;

        if affected == 0 {
            return Err(AppError::Database(format!("Project not found: {}", id)));
        }
        Ok(())
    }

    // ---- Character CRUD ----

    /// Insert a new character into the database.
    pub fn insert_character(&self, character: &Character) -> Result<(), AppError> {
        self.conn
            .execute(
                "INSERT INTO characters (id, project_id, name, tts_engine, voice_name, speed, pitch) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                rusqlite::params![
                    character.id,
                    character.project_id,
                    character.name,
                    tts_engine_to_str(&character.tts_engine),
                    character.voice_name,
                    character.speed,
                    character.pitch,
                ],
            )
            .map_err(|e| AppError::Database(e.to_string()))?;
        Ok(())
    }

    /// Update an existing character's fields.
    pub fn update_character(&self, character: &Character) -> Result<(), AppError> {
        let affected = self
            .conn
            .execute(
                "UPDATE characters SET name = ?1, tts_engine = ?2, voice_name = ?3, speed = ?4, pitch = ?5 WHERE id = ?6",
                rusqlite::params![
                    character.name,
                    tts_engine_to_str(&character.tts_engine),
                    character.voice_name,
                    character.speed,
                    character.pitch,
                    character.id,
                ],
            )
            .map_err(|e| AppError::Database(e.to_string()))?;

        if affected == 0 {
            return Err(AppError::Database(format!("Character not found: {}", character.id)));
        }
        Ok(())
    }

    /// Get the project_id for a character by its ID.
    pub fn get_character_project_id(&self, character_id: &str) -> Result<String, AppError> {
        self.conn
            .query_row(
                "SELECT project_id FROM characters WHERE id = ?1",
                rusqlite::params![character_id],
                |row| row.get(0),
            )
            .map_err(|e| AppError::Database(e.to_string()))
    }

    /// Delete a character by ID.
    pub fn delete_character(&self, id: &str) -> Result<(), AppError> {
        let affected = self
            .conn
            .execute("DELETE FROM characters WHERE id = ?1", rusqlite::params![id])
            .map_err(|e| AppError::Database(e.to_string()))?;

        if affected == 0 {
            return Err(AppError::Database(format!("Character not found: {}", id)));
        }
        Ok(())
    }

    /// List all characters for a given project.
    pub fn list_characters(&self, project_id: &str) -> Result<Vec<Character>, AppError> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, project_id, name, tts_engine, voice_name, speed, pitch FROM characters WHERE project_id = ?1")
            .map_err(|e| AppError::Database(e.to_string()))?;

        let characters = stmt
            .query_map(rusqlite::params![project_id], |row| {
                let engine_str: String = row.get(3)?;
                let tts_engine = tts_engine_from_str(&engine_str).map_err(|e| {
                    rusqlite::Error::FromSqlConversionFailure(
                        3,
                        rusqlite::types::Type::Text,
                        Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
                    )
                })?;
                Ok(Character {
                    id: row.get(0)?,
                    project_id: row.get(1)?,
                    name: row.get(2)?,
                    tts_engine,
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

    // ---- Script Line operations ----

    /// Delete all existing script lines for the project and insert the new ones in a transaction.
    pub fn save_script(&self, project_id: &str, lines: &[ScriptLine]) -> Result<(), AppError> {
        let tx = self
            .conn
            .unchecked_transaction()
            .map_err(|e| AppError::Database(e.to_string()))?;

        tx.execute(
            "DELETE FROM script_lines WHERE project_id = ?1",
            rusqlite::params![project_id],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;

        for line in lines {
            tx.execute(
                "INSERT INTO script_lines (id, project_id, line_order, text, character_id) VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![line.id, line.project_id, line.line_order, line.text, line.character_id],
            )
            .map_err(|e| AppError::Database(e.to_string()))?;
        }

        tx.commit()
            .map_err(|e| AppError::Database(e.to_string()))?;

        Ok(())
    }

    /// Load all script lines for a project, ordered by line_order ASC.
    pub fn load_script(&self, project_id: &str) -> Result<Vec<ScriptLine>, AppError> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, project_id, line_order, text, character_id FROM script_lines WHERE project_id = ?1 ORDER BY line_order ASC")
            .map_err(|e| AppError::Database(e.to_string()))?;

        let lines = stmt
            .query_map(rusqlite::params![project_id], |row| {
                Ok(ScriptLine {
                    id: row.get(0)?,
                    project_id: row.get(1)?,
                    line_order: row.get(2)?,
                    text: row.get(3)?,
                    character_id: row.get(4)?,
                })
            })
            .map_err(|e| AppError::Database(e.to_string()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| AppError::Database(e.to_string()))?;

        Ok(lines)
    }

    // ---- Audio Fragment operations ----

    /// Insert or update an audio fragment. If a fragment already exists for the same line_id,
    /// it is deleted first, then the new fragment is inserted.
    pub fn upsert_audio_fragment(&self, fragment: &AudioFragment) -> Result<(), AppError> {
        let tx = self
            .conn
            .unchecked_transaction()
            .map_err(|e| AppError::Database(e.to_string()))?;

        tx.execute(
            "DELETE FROM audio_fragments WHERE line_id = ?1",
            rusqlite::params![fragment.line_id],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;

        tx.execute(
            "INSERT INTO audio_fragments (id, project_id, line_id, file_path, duration_ms) VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![
                fragment.id,
                fragment.project_id,
                fragment.line_id,
                fragment.file_path,
                fragment.duration_ms,
            ],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;

        tx.commit()
            .map_err(|e| AppError::Database(e.to_string()))?;

        Ok(())
    }

    /// List all audio fragments for a given project.
    pub fn list_audio_fragments(&self, project_id: &str) -> Result<Vec<AudioFragment>, AppError> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, project_id, line_id, file_path, duration_ms FROM audio_fragments WHERE project_id = ?1")
            .map_err(|e| AppError::Database(e.to_string()))?;

        let fragments = stmt
            .query_map(rusqlite::params![project_id], |row| {
                Ok(AudioFragment {
                    id: row.get(0)?,
                    project_id: row.get(1)?,
                    line_id: row.get(2)?,
                    file_path: row.get(3)?,
                    duration_ms: row.get(4)?,
                })
            })
            .map_err(|e| AppError::Database(e.to_string()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| AppError::Database(e.to_string()))?;

        Ok(fragments)
    }

    // ---- User Settings operations ----

    /// Save user settings by serializing the entire UserSettings struct as JSON
    /// and storing it under the "user_settings" key in the user_settings table.
    pub fn save_settings(&self, settings: &UserSettings) -> Result<(), AppError> {
        let json = serde_json::to_string(settings)
            .map_err(|e| AppError::Serialization(e.to_string()))?;

        self.conn
            .execute(
                "INSERT INTO user_settings (key, value) VALUES ('user_settings', ?1)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                rusqlite::params![json],
            )
            .map_err(|e| AppError::Database(e.to_string()))?;

        Ok(())
    }

    /// Load user settings from the database. If no settings exist, return defaults:
    /// - llm_endpoint: "https://api.openai.com/v1"
    /// - llm_model: "gpt-4"
    /// - default_tts_engine: TtsEngine::EdgeTts
    /// - default_voice_name: "zh-CN-XiaoxiaoNeural"
    /// - default_speed: 1.0
    /// - default_pitch: 1.0
    pub fn load_settings(&self) -> Result<UserSettings, AppError> {
        let result: Result<String, _> = self.conn.query_row(
            "SELECT value FROM user_settings WHERE key = 'user_settings'",
            [],
            |row| row.get(0),
        );

        match result {
            Ok(json) => serde_json::from_str(&json)
                .map_err(|e| AppError::Serialization(e.to_string())),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(UserSettings {
                llm_endpoint: "https://api.openai.com/v1".to_string(),
                llm_model: "gpt-4".to_string(),
                default_tts_engine: TtsEngine::EdgeTts,
                default_voice_name: "zh-CN-XiaoxiaoNeural".to_string(),
                default_speed: 1.0,
                default_pitch: 1.0,
            }),
            Err(e) => Err(AppError::Database(e.to_string())),
        }
    }

    // ---- Aggregate load ----

    /// Load a complete project with all associated characters, script lines, and audio fragments.
    pub fn load_project(&self, project_id: &str) -> Result<ProjectDetail, AppError> {
        let project = self.get_project(project_id)?;
        let characters = self.list_characters(project_id)?;
        let script_lines = self.load_script(project_id)?;
        let audio_fragments = self.list_audio_fragments(project_id)?;

        Ok(ProjectDetail {
            project,
            characters,
            script_lines,
            audio_fragments,
        })
    }

    // ---- BGM operations ----

    /// Insert a BGM file record into the database.
    pub fn insert_bgm(
        &self,
        id: &str,
        project_id: &str,
        file_path: &str,
        name: &str,
    ) -> Result<(), AppError> {
        self.conn
            .execute(
                "INSERT INTO bgm_files (id, project_id, file_path, name) VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![id, project_id, file_path, name],
            )
            .map_err(|e| AppError::Database(e.to_string()))?;
        Ok(())
    }
}

/// Convert a TtsEngine enum to its string representation for database storage.
fn tts_engine_to_str(engine: &TtsEngine) -> &'static str {
    match engine {
        TtsEngine::EdgeTts => "edge-tts",
        TtsEngine::AzureTts => "azure-tts",
        TtsEngine::DashscopeTts => "dashscope-tts",
    }
}

/// Parse a TtsEngine from its string representation stored in the database.
fn tts_engine_from_str(s: &str) -> Result<TtsEngine, String> {
    match s {
        "edge-tts" => Ok(TtsEngine::EdgeTts),
        "azure-tts" => Ok(TtsEngine::AzureTts),
        "dashscope-tts" => Ok(TtsEngine::DashscopeTts),
        other => Err(format!("Unknown TTS engine: {}", other)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_db() -> (Database, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let db = Database::open(&db_path).unwrap();
        db.migrate().unwrap();
        (db, dir)
    }

    #[test]
    fn test_open_creates_database_file() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("new.db");
        assert!(!db_path.exists());
        let _db = Database::open(&db_path).unwrap();
        assert!(db_path.exists());
    }

    #[test]
    fn test_foreign_keys_enabled() {
        let (db, _dir) = temp_db();
        let fk: bool = db
            .conn
            .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
            .unwrap();
        assert!(fk, "foreign_keys pragma should be ON");
    }

    #[test]
    fn test_migrate_creates_all_tables() {
        let (db, _dir) = temp_db();

        let tables: Vec<String> = {
            let mut stmt = db
                .conn
                .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
                .unwrap();
            stmt.query_map([], |row| row.get(0))
                .unwrap()
                .filter_map(|r| r.ok())
                .collect()
        };

        let expected = vec![
            "audio_fragments",
            "bgm_files",
            "characters",
            "projects",
            "script_lines",
            "user_settings",
        ];
        for table in &expected {
            assert!(
                tables.contains(&table.to_string()),
                "Table '{}' should exist after migration, found: {:?}",
                table,
                tables
            );
        }
    }

    #[test]
    fn test_migrate_is_idempotent() {
        let (db, _dir) = temp_db();
        // Running migrate a second time should not fail
        db.migrate().unwrap();
    }

    #[test]
    fn test_cascade_delete_characters_on_project_delete() {
        let (db, _dir) = temp_db();

        db.conn
            .execute(
                "INSERT INTO projects (id, name) VALUES ('p1', 'Test Project')",
                [],
            )
            .unwrap();
        db.conn
            .execute(
                "INSERT INTO characters (id, project_id, name, voice_name) VALUES ('c1', 'p1', 'Alice', 'en-US-1')",
                [],
            )
            .unwrap();

        db.conn
            .execute("DELETE FROM projects WHERE id = 'p1'", [])
            .unwrap();

        let count: i64 = db
            .conn
            .query_row(
                "SELECT COUNT(*) FROM characters WHERE project_id = 'p1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 0, "Characters should be cascade-deleted with project");
    }

    #[test]
    fn test_cascade_delete_script_lines_on_project_delete() {
        let (db, _dir) = temp_db();

        db.conn
            .execute(
                "INSERT INTO projects (id, name) VALUES ('p1', 'Test')",
                [],
            )
            .unwrap();
        db.conn
            .execute(
                "INSERT INTO script_lines (id, project_id, line_order, text) VALUES ('l1', 'p1', 1, 'Hello')",
                [],
            )
            .unwrap();

        db.conn
            .execute("DELETE FROM projects WHERE id = 'p1'", [])
            .unwrap();

        let count: i64 = db
            .conn
            .query_row(
                "SELECT COUNT(*) FROM script_lines WHERE project_id = 'p1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_character_delete_sets_null_on_script_lines() {
        let (db, _dir) = temp_db();

        db.conn
            .execute(
                "INSERT INTO projects (id, name) VALUES ('p1', 'Test')",
                [],
            )
            .unwrap();
        db.conn
            .execute(
                "INSERT INTO characters (id, project_id, name, voice_name) VALUES ('c1', 'p1', 'Alice', 'en-US-1')",
                [],
            )
            .unwrap();
        db.conn
            .execute(
                "INSERT INTO script_lines (id, project_id, line_order, text, character_id) VALUES ('l1', 'p1', 1, 'Hello', 'c1')",
                [],
            )
            .unwrap();

        db.conn
            .execute("DELETE FROM characters WHERE id = 'c1'", [])
            .unwrap();

        let char_id: Option<String> = db
            .conn
            .query_row(
                "SELECT character_id FROM script_lines WHERE id = 'l1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(char_id, None, "character_id should be NULL after character deletion");
    }

    #[test]
    fn test_cascade_delete_audio_fragments_on_script_line_delete() {
        let (db, _dir) = temp_db();

        db.conn
            .execute(
                "INSERT INTO projects (id, name) VALUES ('p1', 'Test')",
                [],
            )
            .unwrap();
        db.conn
            .execute(
                "INSERT INTO script_lines (id, project_id, line_order, text) VALUES ('l1', 'p1', 1, 'Hello')",
                [],
            )
            .unwrap();
        db.conn
            .execute(
                "INSERT INTO audio_fragments (id, project_id, line_id, file_path) VALUES ('a1', 'p1', 'l1', '/tmp/a.mp3')",
                [],
            )
            .unwrap();

        db.conn
            .execute("DELETE FROM script_lines WHERE id = 'l1'", [])
            .unwrap();

        let count: i64 = db
            .conn
            .query_row(
                "SELECT COUNT(*) FROM audio_fragments WHERE line_id = 'l1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 0, "Audio fragments should be cascade-deleted with script line");
    }

    // ---- Project CRUD tests ----

    #[test]
    fn test_insert_and_get_project() {
        let (db, _dir) = temp_db();
        let project = Project {
            id: "p1".to_string(),
            name: "My Audiobook".to_string(),
            created_at: "2024-01-01 00:00:00".to_string(),
            updated_at: "2024-01-01 00:00:00".to_string(),
        };
        db.insert_project(&project).unwrap();

        let loaded = db.get_project("p1").unwrap();
        assert_eq!(loaded.id, "p1");
        assert_eq!(loaded.name, "My Audiobook");
        assert_eq!(loaded.created_at, "2024-01-01 00:00:00");
        assert_eq!(loaded.updated_at, "2024-01-01 00:00:00");
    }

    #[test]
    fn test_list_projects_empty() {
        let (db, _dir) = temp_db();
        let projects = db.list_projects().unwrap();
        assert!(projects.is_empty());
    }

    #[test]
    fn test_list_projects_returns_all() {
        let (db, _dir) = temp_db();
        let p1 = Project {
            id: "p1".to_string(),
            name: "First".to_string(),
            created_at: "2024-01-01 00:00:00".to_string(),
            updated_at: "2024-01-01 00:00:00".to_string(),
        };
        let p2 = Project {
            id: "p2".to_string(),
            name: "Second".to_string(),
            created_at: "2024-01-02 00:00:00".to_string(),
            updated_at: "2024-01-02 00:00:00".to_string(),
        };
        db.insert_project(&p1).unwrap();
        db.insert_project(&p2).unwrap();

        let projects = db.list_projects().unwrap();
        assert_eq!(projects.len(), 2);
        // Ordered by created_at DESC, so p2 first
        assert_eq!(projects[0].id, "p2");
        assert_eq!(projects[1].id, "p1");
    }

    #[test]
    fn test_get_project_not_found() {
        let (db, _dir) = temp_db();
        let result = db.get_project("nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_delete_project() {
        let (db, _dir) = temp_db();
        let project = Project {
            id: "p1".to_string(),
            name: "To Delete".to_string(),
            created_at: "2024-01-01 00:00:00".to_string(),
            updated_at: "2024-01-01 00:00:00".to_string(),
        };
        db.insert_project(&project).unwrap();
        db.delete_project("p1").unwrap();

        let projects = db.list_projects().unwrap();
        assert!(projects.is_empty());
    }

    #[test]
    fn test_delete_project_not_found() {
        let (db, _dir) = temp_db();
        let result = db.delete_project("nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_delete_project_cascades_all_related_data() {
        let (db, _dir) = temp_db();

        // Create project with characters, script lines, and audio fragments
        let project = Project {
            id: "p1".to_string(),
            name: "Cascade Test".to_string(),
            created_at: "2024-01-01 00:00:00".to_string(),
            updated_at: "2024-01-01 00:00:00".to_string(),
        };
        db.insert_project(&project).unwrap();

        db.conn
            .execute(
                "INSERT INTO characters (id, project_id, name, voice_name) VALUES ('c1', 'p1', 'Alice', 'en-US-1')",
                [],
            )
            .unwrap();
        db.conn
            .execute(
                "INSERT INTO script_lines (id, project_id, line_order, text, character_id) VALUES ('l1', 'p1', 1, 'Hello', 'c1')",
                [],
            )
            .unwrap();
        db.conn
            .execute(
                "INSERT INTO audio_fragments (id, project_id, line_id, file_path) VALUES ('a1', 'p1', 'l1', '/tmp/a.mp3')",
                [],
            )
            .unwrap();

        db.delete_project("p1").unwrap();

        // Verify all related data is gone
        let char_count: i64 = db.conn.query_row("SELECT COUNT(*) FROM characters", [], |row| row.get(0)).unwrap();
        let line_count: i64 = db.conn.query_row("SELECT COUNT(*) FROM script_lines", [], |row| row.get(0)).unwrap();
        let audio_count: i64 = db.conn.query_row("SELECT COUNT(*) FROM audio_fragments", [], |row| row.get(0)).unwrap();

        assert_eq!(char_count, 0, "Characters should be cascade-deleted");
        assert_eq!(line_count, 0, "Script lines should be cascade-deleted");
        assert_eq!(audio_count, 0, "Audio fragments should be cascade-deleted");
    }

    // ---- Character CRUD tests ----

    fn make_character(id: &str, project_id: &str, name: &str, engine: TtsEngine) -> Character {
        Character {
            id: id.to_string(),
            project_id: project_id.to_string(),
            name: name.to_string(),
            tts_engine: engine,
            voice_name: "en-US-AriaNeural".to_string(),
            speed: 1.0,
            pitch: 1.0,
        }
    }

    fn setup_project(db: &Database, id: &str) {
        let project = Project {
            id: id.to_string(),
            name: "Test Project".to_string(),
            created_at: "2024-01-01 00:00:00".to_string(),
            updated_at: "2024-01-01 00:00:00".to_string(),
        };
        db.insert_project(&project).unwrap();
    }

    #[test]
    fn test_insert_and_list_characters() {
        let (db, _dir) = temp_db();
        setup_project(&db, "p1");

        let c1 = make_character("c1", "p1", "Alice", TtsEngine::EdgeTts);
        let c2 = make_character("c2", "p1", "Bob", TtsEngine::AzureTts);
        db.insert_character(&c1).unwrap();
        db.insert_character(&c2).unwrap();

        let chars = db.list_characters("p1").unwrap();
        assert_eq!(chars.len(), 2);

        let names: Vec<&str> = chars.iter().map(|c| c.name.as_str()).collect();
        assert!(names.contains(&"Alice"));
        assert!(names.contains(&"Bob"));
    }

    #[test]
    fn test_list_characters_empty() {
        let (db, _dir) = temp_db();
        setup_project(&db, "p1");

        let chars = db.list_characters("p1").unwrap();
        assert!(chars.is_empty());
    }

    #[test]
    fn test_list_characters_filters_by_project() {
        let (db, _dir) = temp_db();
        setup_project(&db, "p1");
        setup_project(&db, "p2");

        let c1 = make_character("c1", "p1", "Alice", TtsEngine::EdgeTts);
        let c2 = make_character("c2", "p2", "Bob", TtsEngine::AzureTts);
        db.insert_character(&c1).unwrap();
        db.insert_character(&c2).unwrap();

        let p1_chars = db.list_characters("p1").unwrap();
        assert_eq!(p1_chars.len(), 1);
        assert_eq!(p1_chars[0].name, "Alice");

        let p2_chars = db.list_characters("p2").unwrap();
        assert_eq!(p2_chars.len(), 1);
        assert_eq!(p2_chars[0].name, "Bob");
    }

    #[test]
    fn test_insert_character_preserves_fields() {
        let (db, _dir) = temp_db();
        setup_project(&db, "p1");

        let c = Character {
            id: "c1".to_string(),
            project_id: "p1".to_string(),
            name: "Narrator".to_string(),
            tts_engine: TtsEngine::AzureTts,
            voice_name: "zh-CN-XiaoxiaoNeural".to_string(),
            speed: 1.5,
            pitch: 0.8,
        };
        db.insert_character(&c).unwrap();

        let chars = db.list_characters("p1").unwrap();
        assert_eq!(chars.len(), 1);
        let loaded = &chars[0];
        assert_eq!(loaded.id, "c1");
        assert_eq!(loaded.project_id, "p1");
        assert_eq!(loaded.name, "Narrator");
        assert_eq!(loaded.voice_name, "zh-CN-XiaoxiaoNeural");
        assert!((loaded.speed - 1.5).abs() < f32::EPSILON);
        assert!((loaded.pitch - 0.8).abs() < f32::EPSILON);
        // Verify TtsEngine round-trips correctly
        match &loaded.tts_engine {
            TtsEngine::AzureTts => {} // expected
            other => panic!("Expected AzureTts, got {:?}", other),
        }
    }

    #[test]
    fn test_update_character() {
        let (db, _dir) = temp_db();
        setup_project(&db, "p1");

        let c = make_character("c1", "p1", "Alice", TtsEngine::EdgeTts);
        db.insert_character(&c).unwrap();

        let updated = Character {
            id: "c1".to_string(),
            project_id: "p1".to_string(),
            name: "Alice Updated".to_string(),
            tts_engine: TtsEngine::AzureTts,
            voice_name: "zh-CN-YunxiNeural".to_string(),
            speed: 2.0,
            pitch: 0.5,
        };
        db.update_character(&updated).unwrap();

        let chars = db.list_characters("p1").unwrap();
        assert_eq!(chars.len(), 1);
        let loaded = &chars[0];
        assert_eq!(loaded.name, "Alice Updated");
        assert_eq!(loaded.voice_name, "zh-CN-YunxiNeural");
        assert!((loaded.speed - 2.0).abs() < f32::EPSILON);
        assert!((loaded.pitch - 0.5).abs() < f32::EPSILON);
        match &loaded.tts_engine {
            TtsEngine::AzureTts => {}
            other => panic!("Expected AzureTts, got {:?}", other),
        }
    }

    #[test]
    fn test_update_character_not_found() {
        let (db, _dir) = temp_db();
        setup_project(&db, "p1");

        let c = make_character("nonexistent", "p1", "Ghost", TtsEngine::EdgeTts);
        let result = db.update_character(&c);
        assert!(result.is_err());
    }

    #[test]
    fn test_delete_character() {
        let (db, _dir) = temp_db();
        setup_project(&db, "p1");

        let c = make_character("c1", "p1", "Alice", TtsEngine::EdgeTts);
        db.insert_character(&c).unwrap();
        db.delete_character("c1").unwrap();

        let chars = db.list_characters("p1").unwrap();
        assert!(chars.is_empty());
    }

    #[test]
    fn test_delete_character_not_found() {
        let (db, _dir) = temp_db();
        let result = db.delete_character("nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_delete_character_sets_null_on_script_lines_via_crud() {
        let (db, _dir) = temp_db();
        setup_project(&db, "p1");

        let c = make_character("c1", "p1", "Alice", TtsEngine::EdgeTts);
        db.insert_character(&c).unwrap();

        // Insert a script line referencing this character
        db.conn
            .execute(
                "INSERT INTO script_lines (id, project_id, line_order, text, character_id) VALUES ('l1', 'p1', 1, 'Hello world', 'c1')",
                [],
            )
            .unwrap();

        db.delete_character("c1").unwrap();

        let char_id: Option<String> = db
            .conn
            .query_row(
                "SELECT character_id FROM script_lines WHERE id = 'l1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(char_id, None, "character_id should be NULL after character deletion");
    }

    #[test]
    fn test_insert_character_edge_tts_engine() {
        let (db, _dir) = temp_db();
        setup_project(&db, "p1");

        let c = make_character("c1", "p1", "Narrator", TtsEngine::EdgeTts);
        db.insert_character(&c).unwrap();

        let chars = db.list_characters("p1").unwrap();
        match &chars[0].tts_engine {
            TtsEngine::EdgeTts => {}
            other => panic!("Expected EdgeTts, got {:?}", other),
        }
    }

    #[test]
    fn test_insert_character_dashscope_tts_engine() {
        let (db, _dir) = temp_db();
        setup_project(&db, "p1");

        let c = make_character("c1", "p1", "Narrator", TtsEngine::DashscopeTts);
        db.insert_character(&c).unwrap();

        let chars = db.list_characters("p1").unwrap();
        match &chars[0].tts_engine {
            TtsEngine::DashscopeTts => {}
            other => panic!("Expected DashscopeTts, got {:?}", other),
        }
    }

    // ---- Script Line tests ----

    fn make_script_line(id: &str, project_id: &str, order: i32, text: &str, character_id: Option<&str>) -> ScriptLine {
        ScriptLine {
            id: id.to_string(),
            project_id: project_id.to_string(),
            line_order: order,
            text: text.to_string(),
            character_id: character_id.map(|s| s.to_string()),
        }
    }

    #[test]
    fn test_save_and_load_script_roundtrip() {
        let (db, _dir) = temp_db();
        setup_project(&db, "p1");

        let lines = vec![
            make_script_line("l1", "p1", 0, "Hello world", None),
            make_script_line("l2", "p1", 1, "How are you?", None),
            make_script_line("l3", "p1", 2, "Goodbye!", None),
        ];

        db.save_script("p1", &lines).unwrap();
        let loaded = db.load_script("p1").unwrap();

        assert_eq!(loaded.len(), 3);
        assert_eq!(loaded, lines);
    }

    #[test]
    fn test_load_script_empty() {
        let (db, _dir) = temp_db();
        setup_project(&db, "p1");

        let loaded = db.load_script("p1").unwrap();
        assert!(loaded.is_empty());
    }

    #[test]
    fn test_save_script_replaces_existing() {
        let (db, _dir) = temp_db();
        setup_project(&db, "p1");

        let original = vec![
            make_script_line("l1", "p1", 0, "Original line 1", None),
            make_script_line("l2", "p1", 1, "Original line 2", None),
        ];
        db.save_script("p1", &original).unwrap();

        let replacement = vec![
            make_script_line("l3", "p1", 0, "New line 1", None),
        ];
        db.save_script("p1", &replacement).unwrap();

        let loaded = db.load_script("p1").unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].id, "l3");
        assert_eq!(loaded[0].text, "New line 1");
    }

    #[test]
    fn test_load_script_ordered_by_line_order() {
        let (db, _dir) = temp_db();
        setup_project(&db, "p1");

        // Insert out of order
        let lines = vec![
            make_script_line("l3", "p1", 2, "Third", None),
            make_script_line("l1", "p1", 0, "First", None),
            make_script_line("l2", "p1", 1, "Second", None),
        ];
        db.save_script("p1", &lines).unwrap();

        let loaded = db.load_script("p1").unwrap();
        assert_eq!(loaded[0].text, "First");
        assert_eq!(loaded[1].text, "Second");
        assert_eq!(loaded[2].text, "Third");
    }

    #[test]
    fn test_save_script_with_character_id() {
        let (db, _dir) = temp_db();
        setup_project(&db, "p1");

        let c = make_character("c1", "p1", "Alice", TtsEngine::EdgeTts);
        db.insert_character(&c).unwrap();

        let lines = vec![
            make_script_line("l1", "p1", 0, "Hello", Some("c1")),
            make_script_line("l2", "p1", 1, "World", None),
        ];
        db.save_script("p1", &lines).unwrap();

        let loaded = db.load_script("p1").unwrap();
        assert_eq!(loaded[0].character_id, Some("c1".to_string()));
        assert_eq!(loaded[1].character_id, None);
    }

    #[test]
    fn test_save_script_empty_clears_all() {
        let (db, _dir) = temp_db();
        setup_project(&db, "p1");

        let lines = vec![
            make_script_line("l1", "p1", 0, "Hello", None),
        ];
        db.save_script("p1", &lines).unwrap();

        // Save empty list should clear all lines
        db.save_script("p1", &[]).unwrap();
        let loaded = db.load_script("p1").unwrap();
        assert!(loaded.is_empty());
    }

    #[test]
    fn test_load_script_filters_by_project() {
        let (db, _dir) = temp_db();
        setup_project(&db, "p1");
        setup_project(&db, "p2");

        let lines_p1 = vec![make_script_line("l1", "p1", 0, "Project 1 line", None)];
        let lines_p2 = vec![make_script_line("l2", "p2", 0, "Project 2 line", None)];
        db.save_script("p1", &lines_p1).unwrap();
        db.save_script("p2", &lines_p2).unwrap();

        let loaded_p1 = db.load_script("p1").unwrap();
        assert_eq!(loaded_p1.len(), 1);
        assert_eq!(loaded_p1[0].text, "Project 1 line");

        let loaded_p2 = db.load_script("p2").unwrap();
        assert_eq!(loaded_p2.len(), 1);
        assert_eq!(loaded_p2[0].text, "Project 2 line");
    }

    // ---- Audio Fragment tests ----

    fn make_audio_fragment(id: &str, project_id: &str, line_id: &str, file_path: &str, duration_ms: Option<i64>) -> AudioFragment {
        AudioFragment {
            id: id.to_string(),
            project_id: project_id.to_string(),
            line_id: line_id.to_string(),
            file_path: file_path.to_string(),
            duration_ms,
        }
    }

    #[test]
    fn test_upsert_audio_fragment_insert() {
        let (db, _dir) = temp_db();
        setup_project(&db, "p1");
        let lines = vec![make_script_line("l1", "p1", 0, "Hello", None)];
        db.save_script("p1", &lines).unwrap();

        let frag = make_audio_fragment("a1", "p1", "l1", "/audio/l1.mp3", Some(3000));
        db.upsert_audio_fragment(&frag).unwrap();

        let fragments = db.list_audio_fragments("p1").unwrap();
        assert_eq!(fragments.len(), 1);
        assert_eq!(fragments[0].id, "a1");
        assert_eq!(fragments[0].line_id, "l1");
        assert_eq!(fragments[0].file_path, "/audio/l1.mp3");
        assert_eq!(fragments[0].duration_ms, Some(3000));
    }

    #[test]
    fn test_upsert_audio_fragment_replaces_existing_for_same_line() {
        let (db, _dir) = temp_db();
        setup_project(&db, "p1");
        let lines = vec![make_script_line("l1", "p1", 0, "Hello", None)];
        db.save_script("p1", &lines).unwrap();

        let frag1 = make_audio_fragment("a1", "p1", "l1", "/audio/l1_old.mp3", Some(2000));
        db.upsert_audio_fragment(&frag1).unwrap();

        let frag2 = make_audio_fragment("a2", "p1", "l1", "/audio/l1_new.mp3", Some(4000));
        db.upsert_audio_fragment(&frag2).unwrap();

        let fragments = db.list_audio_fragments("p1").unwrap();
        assert_eq!(fragments.len(), 1, "Should only have one fragment per line_id after upsert");
        assert_eq!(fragments[0].id, "a2");
        assert_eq!(fragments[0].file_path, "/audio/l1_new.mp3");
        assert_eq!(fragments[0].duration_ms, Some(4000));
    }

    #[test]
    fn test_upsert_audio_fragment_different_lines() {
        let (db, _dir) = temp_db();
        setup_project(&db, "p1");
        let lines = vec![
            make_script_line("l1", "p1", 0, "Hello", None),
            make_script_line("l2", "p1", 1, "World", None),
        ];
        db.save_script("p1", &lines).unwrap();

        let frag1 = make_audio_fragment("a1", "p1", "l1", "/audio/l1.mp3", Some(1000));
        let frag2 = make_audio_fragment("a2", "p1", "l2", "/audio/l2.mp3", Some(2000));
        db.upsert_audio_fragment(&frag1).unwrap();
        db.upsert_audio_fragment(&frag2).unwrap();

        let fragments = db.list_audio_fragments("p1").unwrap();
        assert_eq!(fragments.len(), 2);
    }

    #[test]
    fn test_upsert_audio_fragment_with_no_duration() {
        let (db, _dir) = temp_db();
        setup_project(&db, "p1");
        let lines = vec![make_script_line("l1", "p1", 0, "Hello", None)];
        db.save_script("p1", &lines).unwrap();

        let frag = make_audio_fragment("a1", "p1", "l1", "/audio/l1.mp3", None);
        db.upsert_audio_fragment(&frag).unwrap();

        let fragments = db.list_audio_fragments("p1").unwrap();
        assert_eq!(fragments.len(), 1);
        assert_eq!(fragments[0].duration_ms, None);
    }

    #[test]
    fn test_list_audio_fragments_empty() {
        let (db, _dir) = temp_db();
        setup_project(&db, "p1");

        let fragments = db.list_audio_fragments("p1").unwrap();
        assert!(fragments.is_empty());
    }

    #[test]
    fn test_list_audio_fragments_filters_by_project() {
        let (db, _dir) = temp_db();
        setup_project(&db, "p1");
        setup_project(&db, "p2");

        let lines_p1 = vec![make_script_line("l1", "p1", 0, "Hello", None)];
        let lines_p2 = vec![make_script_line("l2", "p2", 0, "World", None)];
        db.save_script("p1", &lines_p1).unwrap();
        db.save_script("p2", &lines_p2).unwrap();

        let frag1 = make_audio_fragment("a1", "p1", "l1", "/audio/l1.mp3", Some(1000));
        let frag2 = make_audio_fragment("a2", "p2", "l2", "/audio/l2.mp3", Some(2000));
        db.upsert_audio_fragment(&frag1).unwrap();
        db.upsert_audio_fragment(&frag2).unwrap();

        let p1_frags = db.list_audio_fragments("p1").unwrap();
        assert_eq!(p1_frags.len(), 1);
        assert_eq!(p1_frags[0].id, "a1");

        let p2_frags = db.list_audio_fragments("p2").unwrap();
        assert_eq!(p2_frags.len(), 1);
        assert_eq!(p2_frags[0].id, "a2");
    }

    // ---- User Settings tests ----

    use super::super::models::UserSettings;

    fn default_settings() -> UserSettings {
        UserSettings {
            llm_endpoint: "https://api.openai.com/v1".to_string(),
            llm_model: "gpt-4".to_string(),
            default_tts_engine: TtsEngine::EdgeTts,
            default_voice_name: "zh-CN-XiaoxiaoNeural".to_string(),
            default_speed: 1.0,
            default_pitch: 1.0,
        }
    }

    #[test]
    fn test_load_settings_returns_defaults_when_empty() {
        let (db, _dir) = temp_db();
        let settings = db.load_settings().unwrap();

        assert_eq!(settings.llm_endpoint, "https://api.openai.com/v1");
        assert_eq!(settings.llm_model, "gpt-4");
        assert_eq!(settings.default_voice_name, "zh-CN-XiaoxiaoNeural");
        assert!((settings.default_speed - 1.0).abs() < f32::EPSILON);
        assert!((settings.default_pitch - 1.0).abs() < f32::EPSILON);
        match settings.default_tts_engine {
            TtsEngine::EdgeTts => {}
            other => panic!("Expected EdgeTts, got {:?}", other),
        }
    }

    #[test]
    fn test_save_and_load_settings_roundtrip() {
        let (db, _dir) = temp_db();
        let settings = UserSettings {
            llm_endpoint: "https://custom.api.com/v1".to_string(),
            llm_model: "gpt-3.5-turbo".to_string(),
            default_tts_engine: TtsEngine::AzureTts,
            default_voice_name: "en-US-AriaNeural".to_string(),
            default_speed: 1.5,
            default_pitch: 0.8,
        };

        db.save_settings(&settings).unwrap();
        let loaded = db.load_settings().unwrap();

        assert_eq!(loaded.llm_endpoint, "https://custom.api.com/v1");
        assert_eq!(loaded.llm_model, "gpt-3.5-turbo");
        assert_eq!(loaded.default_voice_name, "en-US-AriaNeural");
        assert!((loaded.default_speed - 1.5).abs() < f32::EPSILON);
        assert!((loaded.default_pitch - 0.8).abs() < f32::EPSILON);
        match loaded.default_tts_engine {
            TtsEngine::AzureTts => {}
            other => panic!("Expected AzureTts, got {:?}", other),
        }
    }

    #[test]
    fn test_save_settings_overwrites_previous() {
        let (db, _dir) = temp_db();

        let first = default_settings();
        db.save_settings(&first).unwrap();

        let second = UserSettings {
            llm_endpoint: "https://other.api.com".to_string(),
            llm_model: "claude-3".to_string(),
            default_tts_engine: TtsEngine::AzureTts,
            default_voice_name: "ja-JP-NanamiNeural".to_string(),
            default_speed: 2.0,
            default_pitch: 0.5,
        };
        db.save_settings(&second).unwrap();

        let loaded = db.load_settings().unwrap();
        assert_eq!(loaded.llm_endpoint, "https://other.api.com");
        assert_eq!(loaded.llm_model, "claude-3");
        assert_eq!(loaded.default_voice_name, "ja-JP-NanamiNeural");
        assert!((loaded.default_speed - 2.0).abs() < f32::EPSILON);
        assert!((loaded.default_pitch - 0.5).abs() < f32::EPSILON);
        match loaded.default_tts_engine {
            TtsEngine::AzureTts => {}
            other => panic!("Expected AzureTts, got {:?}", other),
        }
    }

    #[test]
    fn test_save_settings_preserves_default_values() {
        let (db, _dir) = temp_db();

        let settings = default_settings();
        db.save_settings(&settings).unwrap();
        let loaded = db.load_settings().unwrap();

        assert_eq!(loaded.llm_endpoint, settings.llm_endpoint);
        assert_eq!(loaded.llm_model, settings.llm_model);
        assert_eq!(loaded.default_voice_name, settings.default_voice_name);
        assert!((loaded.default_speed - settings.default_speed).abs() < f32::EPSILON);
        assert!((loaded.default_pitch - settings.default_pitch).abs() < f32::EPSILON);
    }

    // ---- load_project aggregate test ----

    #[test]
    fn test_load_project_aggregates_all_data() {
        let (db, _dir) = temp_db();
        setup_project(&db, "p1");

        // Insert characters
        let c1 = make_character("c1", "p1", "Alice", TtsEngine::EdgeTts);
        let c2 = make_character("c2", "p1", "Bob", TtsEngine::AzureTts);
        db.insert_character(&c1).unwrap();
        db.insert_character(&c2).unwrap();

        // Insert script lines
        let lines = vec![
            make_script_line("l1", "p1", 0, "Hello from Alice", Some("c1")),
            make_script_line("l2", "p1", 1, "Hello from Bob", Some("c2")),
            make_script_line("l3", "p1", 2, "Narrator line", None),
        ];
        db.save_script("p1", &lines).unwrap();

        // Insert audio fragments
        let frag1 = make_audio_fragment("a1", "p1", "l1", "/audio/l1.mp3", Some(3000));
        let frag2 = make_audio_fragment("a2", "p1", "l2", "/audio/l2.mp3", Some(2500));
        db.upsert_audio_fragment(&frag1).unwrap();
        db.upsert_audio_fragment(&frag2).unwrap();

        // Load the complete project
        let detail = db.load_project("p1").unwrap();

        assert_eq!(detail.project.id, "p1");
        assert_eq!(detail.characters.len(), 2);
        assert_eq!(detail.script_lines.len(), 3);
        assert_eq!(detail.audio_fragments.len(), 2);

        // Verify script lines are ordered
        assert_eq!(detail.script_lines[0].text, "Hello from Alice");
        assert_eq!(detail.script_lines[1].text, "Hello from Bob");
        assert_eq!(detail.script_lines[2].text, "Narrator line");
    }

    #[test]
    fn test_load_project_empty_associations() {
        let (db, _dir) = temp_db();
        setup_project(&db, "p1");

        let detail = db.load_project("p1").unwrap();

        assert_eq!(detail.project.id, "p1");
        assert!(detail.characters.is_empty());
        assert!(detail.script_lines.is_empty());
        assert!(detail.audio_fragments.is_empty());
    }

    #[test]
    fn test_load_project_not_found() {
        let (db, _dir) = temp_db();
        let result = db.load_project("nonexistent");
        assert!(result.is_err());
    }
}
