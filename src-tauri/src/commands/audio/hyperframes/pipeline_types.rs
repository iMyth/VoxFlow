//! Data model types for the LLM orchestration pipeline.
//!
//! Defines all structures used across the orchestrator, worker, and merger modules.

use super::timeline::TimelineEntry;

// ─── Orchestration Plan (Orchestrator LLM Output) ───────────────────────────

/// The structured plan produced by the Orchestrator LLM.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OrchestrationPlan {
    /// Global visual theme that spans all chunks.
    pub global_theme: GlobalTheme,
    /// Ordered list of chunk definitions.
    pub chunks: Vec<ChunkPlan>,
}

/// Global visual theme spanning the entire composition.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GlobalTheme {
    /// Overall mood/atmosphere keywords (e.g., "mysterious", "epic").
    pub mood: Vec<String>,
    /// Shared visual motifs that recur across chunks.
    pub shared_motifs: Vec<String>,
    /// Global color progression (start palette → end palette).
    pub color_progression: ColorProgression,
}

/// Describes how colors evolve across the full composition.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ColorProgression {
    /// Starting palette colors (hex).
    pub start_palette: Vec<String>,
    /// Ending palette colors (hex).
    pub end_palette: Vec<String>,
}

/// A single chunk definition within the orchestration plan.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ChunkPlan {
    /// Index of this chunk (0-based).
    pub index: usize,
    /// Start index into the timeline entries array (inclusive).
    pub entry_start: usize,
    /// End index into the timeline entries array (exclusive).
    pub entry_end: usize,
    /// Visual directive for this chunk.
    pub visual_directive: VisualDirective,
    /// Transition-in specification (how this chunk enters).
    pub transition_in: TransitionSpec,
    /// Transition-out specification (how this chunk exits).
    pub transition_out: TransitionSpec,
}

/// Visual parameters assigned to a chunk by the Orchestrator.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct VisualDirective {
    /// Color palette for this chunk (3-6 hex colors).
    pub palette: Vec<String>,
    /// Visual style keywords (up to 5).
    pub style_keywords: Vec<String>,
    /// Animation rhythm: "slow", "moderate", "fast", or "dynamic".
    pub rhythm: String,
    /// Specific visual concept description for this chunk.
    pub concept: String,
}

/// Specification for a transition between chunks.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TransitionSpec {
    /// Transition type identifier (e.g., "fade", "wipe-left", "dissolve", "morph").
    pub transition_type: String,
    /// Colors to use in the transition (from previous/to next chunk).
    pub colors: Vec<String>,
}

// ─── Worker Input/Output ────────────────────────────────────────────────────

/// Input passed to each Worker LLM call.
#[derive(Debug, Clone)]
pub struct WorkerInput {
    /// The chunk plan from the Orchestrator.
    pub chunk_plan: ChunkPlan,
    /// The timeline entries for this chunk.
    pub entries: Vec<TimelineEntry>,
    /// Total duration of the full composition (for context).
    pub total_duration: f64,
    /// Previous chunk's ending palette (None for first chunk).
    pub prev_ending_palette: Option<Vec<String>>,
    /// Total number of chunks (for context).
    pub total_chunks: usize,
}

/// Output from a successful Worker call.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct WorkerOutput {
    /// Chunk index.
    pub chunk_index: usize,
    /// Generated HTML fragment (complete HTML file for this chunk).
    pub html: String,
    /// Number of retries used before success.
    pub retries_used: usize,
}

/// Result of a Worker call (success or failure).
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum WorkerResult {
    Success(WorkerOutput),
    Failed {
        chunk_index: usize,
        errors: Vec<String>,
        retries_exhausted: bool,
    },
}

// ─── Pipeline Progress Events ───────────────────────────────────────────────

/// Progress event payload emitted via Tauri event system.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PipelineProgress {
    /// Current stage identifier.
    pub stage: PipelineStage,
    /// Human-readable description.
    pub message: String,
    /// Overall progress percentage (0.0 - 100.0).
    pub percent: f32,
    /// Chunk-specific info (if applicable).
    pub chunk_info: Option<ChunkProgress>,
}

/// Identifies the current stage of the pipeline.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum PipelineStage {
    Orchestrating,
    GeneratingChunk,
    ChunkCompleted,
    ChunkFailed,
    Merging,
    Complete,
    FallbackActivated,
}

/// Progress information specific to chunk processing.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ChunkProgress {
    pub chunk_index: usize,
    pub total_chunks: usize,
    pub chunks_completed: usize,
    pub chunks_failed: usize,
}

// ─── Token Budget ───────────────────────────────────────────────────────────

/// Token budget configuration for LLM calls.
#[derive(Debug, Clone)]
pub struct TokenBudget {
    /// Max input tokens for the Orchestrator call.
    pub orchestrator_input: usize,
    /// Max input tokens for each Worker call.
    pub worker_input: usize,
    /// Max output tokens for each Worker call.
    pub worker_output: usize,
}

impl TokenBudget {
    /// Default budget based on common model limits (128k context).
    /// Uses conservative defaults suitable for most OpenAI-compatible models.
    pub fn default_for_model(_model: &str) -> Self {
        TokenBudget {
            orchestrator_input: 32_000,
            worker_input: 24_000,
            worker_output: 16_000,
        }
    }
}

// ─── Error Types ────────────────────────────────────────────────────────────

/// Errors that can occur during pipeline execution.
#[derive(Debug)]
pub enum PipelineError {
    /// LLM returned context-length-exceeded error.
    ContextLengthExceeded(String),
    /// Orchestrator call failed (non-context-length reason).
    OrchestratorFailed(String),
    /// Orchestrator returned invalid/unparseable JSON.
    InvalidPlan(String),
    /// All workers failed.
    AllWorkersFailed(Vec<String>),
    /// Merger failed.
    MergerFailed(MergerError),
    /// Generic error.
    Other(String),
}

/// Errors specific to the deterministic merger.
#[derive(Debug)]
#[allow(dead_code)]
pub enum MergerError {
    /// Could not parse a chunk's HTML.
    ParseFailed { chunk_index: usize, reason: String },
    /// Transition incompatibility detected between adjacent chunks.
    TransitionMismatch {
        chunk_a: usize,
        chunk_b: usize,
        reason: String,
    },
    /// Final HTML failed validation.
    ValidationFailed(Vec<String>),
}

// ─── Merger Data Structures ─────────────────────────────────────────────────

/// A parsed representation of a Worker's HTML output.
#[derive(Debug, Clone)]
pub struct ParsedChunk {
    /// Chunk index.
    pub chunk_index: usize,
    /// Extracted CSS content (already namespaced).
    pub css: String,
    /// Extracted clip elements.
    pub clips: Vec<ClipElement>,
    /// Extracted GSAP timeline code (selectors updated).
    pub gsap_code: String,
}

/// A single clip element extracted from a Worker's HTML output.
#[derive(Debug, Clone)]
pub struct ClipElement {
    /// The HTML content of this clip element.
    pub html: String,
    /// Start time in seconds.
    pub data_start: f64,
    /// Duration in seconds.
    pub data_duration: f64,
    /// Track index for layering.
    pub data_track_index: u32,
}
