//! Types for paragraph-level (per-ScriptSection) video generation.

use serde::{Deserialize, Serialize};

/// Configuration for how a section's video should be generated.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SectionStyleConfig {
    pub mode: GenerationMode,
    /// Optional user instructions for the agent.
    pub user_prompt: Option<String>,
}

/// The generation approach for a section's video.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GenerationMode {
    Agent,
}

/// Result of generating a single section's video.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SectionVideoResult {
    pub section_id: String,
    pub video_path: String,
    pub duration_ms: i64,
    pub file_size_bytes: u64,
}

/// Result of a batch generation operation across multiple sections.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchGenerationResult {
    /// Section IDs that completed successfully.
    pub completed: Vec<String>,
    /// Sections that failed: (section_id, error_message).
    pub failed: Vec<(String, String)>,
}

/// Progress event emitted during per-section video generation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SectionProgress {
    pub section_id: String,
    pub percent: f32,
    pub stage: String,
}

/// Progress event emitted during the final video merge.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeProgress {
    pub percent: f32,
    pub stage: String,
}

/// Represents a section's video file for the merge step.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SectionVideoFile {
    pub section_id: String,
    pub section_order: i32,
    pub file_path: String,
    pub duration_ms: i64,
}
