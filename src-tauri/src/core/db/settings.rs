//! User settings operations.

use rusqlite::Connection;

use super::super::error::AppError;
use super::super::models::UserSettings;

/// Save user settings by serializing the entire UserSettings struct as JSON
/// and storing it under the "user_settings" key in the user_settings table.
pub fn save_settings(conn: &Connection, settings: &UserSettings) -> Result<(), AppError> {
    let json = serde_json::to_string(settings)
        .map_err(|e| AppError::Serialization(e.to_string()))?;

    conn.execute(
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
/// - default_tts_model: "qwen3-tts-flash"
/// - default_voice_name: "Cherry"
/// - default_speed: 1.0
/// - default_pitch: 1.0
pub fn load_settings(conn: &Connection) -> Result<UserSettings, AppError> {
    let result: Result<String, _> = conn.query_row(
        "SELECT value FROM user_settings WHERE key = 'user_settings'",
        [],
        |row| row.get(0),
    );

    match result {
        Ok(json) => {
            serde_json::from_str(&json).map_err(|e| AppError::Serialization(e.to_string()))
        }
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(UserSettings {
            llm_endpoint: "https://api.openai.com/v1".to_string(),
            llm_model: "gpt-4".to_string(),
            default_tts_model: "qwen3-tts-flash".to_string(),
            default_voice_name: "Cherry".to_string(),
            default_speed: 1.0,
            default_pitch: 1.0,
            enable_thinking: true,
        }),
        Err(e) => Err(AppError::Database(e.to_string())),
    }
}
