use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub outline: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectDetail {
    pub project: Project,
    pub characters: Vec<Character>,
    pub script_lines: Vec<ScriptLine>,
    pub audio_fragments: Vec<AudioFragment>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Character {
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub voice_name: String,
    pub tts_model: String,
    pub speed: f32,
    pub pitch: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScriptLine {
    pub id: String,
    pub project_id: String,
    pub line_order: i32,
    pub text: String,
    pub character_id: Option<String>,
    pub gap_after_ms: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioFragment {
    pub id: String,
    pub project_id: String,
    pub line_id: String,
    pub file_path: String,
    pub duration_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceConfig {
    pub voice_name: String,
    pub tts_model: String,
    pub speed: f32,
    pub pitch: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    pub api_endpoint: String,
    pub api_key: String,
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MixConfig {
    pub fragment_paths: Vec<String>,
    pub bgm_path: Option<String>,
    pub bgm_volume: f32,
    pub output_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MixProgress {
    pub percent: f32,
    pub stage: String,
}

/// Progress event for batch TTS generation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TtsBatchProgress {
    pub current: usize,
    pub total: usize,
    pub line_id: String,
    pub success: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserSettings {
    pub llm_endpoint: String,
    pub llm_model: String,
    pub default_tts_model: String,
    pub default_voice_name: String,
    pub default_speed: f32,
    pub default_pitch: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterInput {
    pub name: String,
    pub voice_name: String,
    pub tts_model: String,
    pub speed: f32,
    pub pitch: f32,
}

/// LLM script generation response — parsed from JSON output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmScriptResponse {
    pub lines: Vec<LlmScriptLine>,
}

/// A single line from LLM generation. `character` is a human-readable name.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmScriptLine {
    pub text: String,
    pub character: Option<String>,
}
