use serde::{Deserialize, Serialize};

/// Agent plan phase: analysis result before generation.
/// Returned to frontend for user review and confirmation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentPlan {
    /// Detected chapters/scenes from the outline
    pub chapters: Vec<ChapterPlan>,
    /// Characters suggested by the LLM analysis
    pub suggested_characters: Vec<SuggestedCharacter>,
    /// Overall style/tone of the audiobook
    pub overall_style: String,
    /// LLM's recommendation on how to handle missing characters
    pub character_notes: String,
}

/// A single chapter/scene detected from the outline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChapterPlan {
    /// Chapter title or description (from outline)
    pub title: String,
    /// Estimated number of lines
    pub estimated_lines: u32,
    /// Characters used in this chapter
    pub characters: Vec<String>,
    /// Mood/tone for this chapter
    pub mood: String,
}

/// A character suggested by the LLM during analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuggestedCharacter {
    /// Character name as mentioned in the outline
    pub name: String,
    /// Role description (protagonist, antagonist, narrator, etc.)
    pub role: String,
    /// Whether this character matches an existing project character
    pub matched_existing: bool,
    /// The existing character ID if matched
    pub existing_id: Option<String>,
}

/// User's response to the plan — which characters to use, any adjustments.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanConfirmation {
    /// Whether user confirms the plan as-is or with adjustments
    pub confirmed: bool,
    /// Character name mapping: suggested_name → existing_character_id
    /// If a suggested character has no match, this key is absent
    pub character_mapping: std::collections::HashMap<String, String>,
    /// New characters to create (name → TTS config)
    pub new_characters: Vec<NewCharacterInput>,
    /// User's additional instructions for generation
    pub extra_instructions: String,
}

/// Input for creating a new character during plan confirmation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewCharacterInput {
    pub name: String,
    pub voice_name: String,
    pub tts_model: String,
    pub speed: f32,
    pub pitch: f32,
}
