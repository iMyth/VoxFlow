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
    pub sections: Vec<ScriptSection>,
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
    pub instructions: String,
    pub section_id: Option<String>,
}

/// A section/group of script lines (e.g. "片头", "第一幕", "片尾").
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScriptSection {
    pub id: String,
    pub project_id: String,
    pub title: String,
    pub section_order: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioFragment {
    pub id: String,
    pub project_id: String,
    pub line_id: String,
    pub file_path: String,
    pub duration_ms: Option<i64>,
    pub source: String, // "tts" or "recording"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceConfig {
    pub voice_name: String,
    pub tts_model: String,
    pub speed: f32,
    pub pitch: f32,
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
    pub enable_thinking: bool,
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
/// Contains sections, each with a title and lines.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmScriptResponse {
    pub sections: Vec<LlmSection>,
}

/// A section from LLM generation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmSection {
    pub title: String,
    pub lines: Vec<LlmScriptLine>,
}

/// A single line from LLM generation. `character` is a human-readable name.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmScriptLine {
    pub text: String,
    pub character: Option<String>,
    pub instructions: Option<String>,
    pub gap_ms: Option<u32>,
}

/// A knowledge entry in the story vector database.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoryKnowledgeItem {
    pub id: String,
    pub project_id: String,
    pub text: String,
    /// JSON array of f32 values representing the embedding vector.
    pub embedding: String,
    pub kb_type: String, // "plot", "character", "setting", "foreshadow"
    pub metadata: String, // JSON string for extra context
    pub created_at: String,
}

/// Summary stats for a project shown on the project list card.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectStats {
    pub id: String,
    pub name: String,
    pub created_at: String,
    pub line_count: i32,
    pub audio_count: i32,
    pub character_count: i32,
}
