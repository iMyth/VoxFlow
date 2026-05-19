//! Agentic Hyperframes composition generation using rig.
//!
//! This module implements a local AI agent that uses the official Hyperframes
//! skill files (`.agents/skills/hyperframes/`) as dynamic context, with tools
//! for validation, reference loading, and iterative self-correction.
//!
//! Architecture:
//! - System prompt: loaded from SKILL.md (core rules, always present)
//! - Tools:
//!   - `read_reference`: load skill reference docs on demand
//!   - `validate_html`: run composition validation
//! - Agent loop: plan → generate → validate → fix (multi-turn)

use std::path::{Path, PathBuf};

use log::info;
use rig_core::client::CompletionClient;
use rig_core::completion::{Prompt, ToolDefinition};
use rig_core::providers;
use rig_core::tool::Tool;
use serde::{Deserialize, Serialize};

use super::timeline::TimelineEntry;
use super::validation::validate_composition;

// ─── Error Type ─────────────────────────────────────────────────────────────

/// Tool error type that implements std::error::Error (required by rig 0.37).
#[derive(Debug, thiserror::Error)]
pub enum AgentToolError {
    #[error("{0}")]
    General(String),
}

// ─── Skills Loader ──────────────────────────────────────────────────────────

/// Manages loading skill files from the `.agents/skills/hyperframes/` directory.
pub struct SkillsContext {
    /// Root path to the skills directory
    skills_dir: PathBuf,
    /// Cached system prompt (SKILL.md + house-style + motion-principles)
    pub system_prompt: String,
}

impl SkillsContext {
    /// Create a new SkillsContext by loading SKILL.md from the given directory.
    ///
    /// `project_root` should be the VoxFlow project root (where `.agents/` lives).
    pub fn new(project_root: &Path) -> Result<Self, String> {
        let skills_dir = project_root.join(".agents/skills/hyperframes");

        if !skills_dir.exists() {
            return Err(format!(
                "Skills directory not found: {}",
                skills_dir.display()
            ));
        }

        // Load SKILL.md as the base system prompt
        let skill_md_path = skills_dir.join("SKILL.md");
        let system_prompt = std::fs::read_to_string(&skill_md_path).map_err(|e| {
            format!(
                "Failed to read SKILL.md: {} ({})",
                skill_md_path.display(),
                e
            )
        })?;

        // Also load house-style.md and motion-principles.md as always-on context
        let house_style =
            std::fs::read_to_string(skills_dir.join("house-style.md")).unwrap_or_default();
        let motion_principles =
            std::fs::read_to_string(skills_dir.join("references/motion-principles.md"))
                .unwrap_or_default();

        // Compose the full system prompt
        let full_prompt = format!(
            "{}\n\n---\n\n# House Style (always loaded)\n\n{}\n\n---\n\n# Motion Principles (always loaded)\n\n{}",
            system_prompt, house_style, motion_principles
        );

        Ok(Self {
            skills_dir,
            system_prompt: full_prompt,
        })
    }

    /// List available reference documents.
    pub fn list_references(&self) -> Vec<String> {
        let refs_dir = self.skills_dir.join("references");
        let mut refs = Vec::new();

        if let Ok(entries) = std::fs::read_dir(&refs_dir) {
            for entry in entries.flatten() {
                if let Some(name) = entry.file_name().to_str() {
                    if name.ends_with(".md") {
                        refs.push(name.to_string());
                    }
                }
            }
        }

        for name in &["visual-styles.md", "patterns.md", "data-in-motion.md"] {
            if self.skills_dir.join(name).exists() {
                refs.push(name.to_string());
            }
        }

        refs.sort();
        refs
    }

    /// Load a specific reference document by name.
    pub fn load_reference(&self, name: &str) -> Result<String, String> {
        let ref_path = self.skills_dir.join("references").join(name);
        if ref_path.exists() {
            return std::fs::read_to_string(&ref_path)
                .map_err(|e| format!("Failed to read {}: {}", name, e));
        }

        let top_path = self.skills_dir.join(name);
        if top_path.exists() {
            return std::fs::read_to_string(&top_path)
                .map_err(|e| format!("Failed to read {}: {}", name, e));
        }

        Err(format!(
            "Reference '{}' not found. Available: {:?}",
            name,
            self.list_references()
        ))
    }
}

// ─── Tools ──────────────────────────────────────────────────────────────────

/// Tool: Read a Hyperframes skill reference document.
#[derive(Debug, Serialize, Deserialize)]
pub struct ReadReferenceTool {
    skills_dir: PathBuf,
}

impl ReadReferenceTool {
    pub fn new(skills_dir: PathBuf) -> Self {
        Self { skills_dir }
    }
}

/// Arguments for the read_reference tool.
#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
pub struct ReadReferenceArgs {
    /// Name of the reference file to load (e.g. "transitions.md", "techniques.md",
    /// "beat-direction.md", "typography.md", "video-composition.md").
    pub name: String,
}

impl Tool for ReadReferenceTool {
    const NAME: &'static str = "read_reference";
    type Error = AgentToolError;
    type Args = ReadReferenceArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "read_reference".to_string(),
            description: "Load a Hyperframes skill reference document for detailed guidance on a specific topic. Available: transitions.md, techniques.md, beat-direction.md, typography.md, video-composition.md, captions.md, audio-reactive.md, css-patterns.md, dynamic-techniques.md, visual-styles.md, patterns.md, data-in-motion.md".to_string(),
            parameters: serde_json::to_value(schemars::schema_for!(ReadReferenceArgs)).unwrap(),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let ctx = SkillsContext {
            skills_dir: self.skills_dir.clone(),
            system_prompt: String::new(),
        };
        ctx.load_reference(&args.name)
            .map_err(AgentToolError::General)
    }
}

/// Tool: Validate a Hyperframes HTML composition.
#[derive(Debug, Serialize, Deserialize)]
pub struct ValidateHtmlTool;

/// Arguments for the validate_html tool.
#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
pub struct ValidateHtmlArgs {
    /// The complete HTML composition to validate.
    pub html: String,
}

impl Tool for ValidateHtmlTool {
    const NAME: &'static str = "validate_html";
    type Error = AgentToolError;
    type Args = ValidateHtmlArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "validate_html".to_string(),
            description: "Validate a Hyperframes HTML composition against the spec. Returns 'VALID' or a list of errors to fix.".to_string(),
            parameters: serde_json::to_value(schemars::schema_for!(ValidateHtmlArgs)).unwrap(),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        match validate_composition(&args.html) {
            Ok(()) => Ok("VALID: Composition passes all structural checks.".to_string()),
            Err(errors) => Ok(format!(
                "ERRORS FOUND ({} issues):\n- {}",
                errors.len(),
                errors.join("\n- ")
            )),
        }
    }
}

// ─── Agent Builder ──────────────────────────────────────────────────────────

/// Configuration for the agent-based generation.
pub struct AgentConfig {
    pub api_endpoint: String,
    pub api_key: String,
    pub model: String,
    pub project_root: PathBuf,
}

/// Run the agent-based Hyperframes generation pipeline.
///
/// This is the main entry point for the agent mode. It:
/// 1. Loads skills context (SKILL.md + house-style + motion-principles)
/// 2. Builds a rig agent with tools (read_reference, validate_html)
/// 3. Sends the timeline data as a user prompt
/// 4. Lets the agent iterate (generate → validate → fix) via multi-turn
/// 5. Extracts the final HTML from the agent's response
pub async fn generate_with_agent(
    entries: &[TimelineEntry],
    config: &AgentConfig,
    on_progress: Option<Box<dyn Fn(&str) + Send + Sync>>,
    user_instructions: Option<&str>,
) -> Result<String, String> {
    let report = |msg: &str| {
        if let Some(ref cb) = on_progress {
            cb(msg);
        }
    };

    report("loading_skills");

    // Load skills context
    let skills = SkillsContext::new(&config.project_root)?;
    let available_refs = skills.list_references();
    info!(
        "[Agent] Skills loaded. {} references available: {:?}",
        available_refs.len(),
        available_refs
    );

    report("building_agent");

    // Build the OpenAI-compatible client (CompletionsClient for /chat/completions API)
    let client = providers::openai::CompletionsClient::builder()
        .api_key(&config.api_key)
        .base_url(&config.api_endpoint)
        .build()
        .map_err(|e| format!("Failed to build LLM client: {}", e))?;

    // Build the agent with tools
    let agent = client
        .agent(&config.model)
        .preamble(&skills.system_prompt)
        .temperature(0.9)
        .tool(ReadReferenceTool::new(skills.skills_dir.clone()))
        .tool(ValidateHtmlTool)
        .build();

    // Build the user prompt with timeline data
    let total_duration: f64 = entries
        .iter()
        .map(|e| e.start_time + e.duration)
        .fold(0.0_f64, f64::max);

    let timeline_json = build_timeline_prompt(entries, total_duration);

    let user_prompt = format!(
        r#"请为以下有声书时间轴创作一个完整的 Hyperframes 视觉作品。

工作流程：
1. 先用 read_reference 加载 "beat-direction.md" 和 "techniques.md"
2. 根据内容情绪规划场景节奏（哪些快、哪些慢、哪里是高潮）
3. 为每个场景选择不同的视觉技术
4. 生成完整的 HTML composition（从 <!DOCTYPE html> 开始）
5. 用 validate_html 验证你的输出
6. 如果有错误，修复后重新输出
{user_section}
时间轴数据：
{timeline}

总时长：{duration:.1} 秒
条目数：{count}

关键约束：
- composition-id 使用 "ai-generated"
- 尺寸 1920x1080
- 所有动画挂在 paused timeline 上
- 不要用 Math.random()、repeat: -1
- 最终输出必须是完整的、通过 validate_html 验证的 HTML"#,
        user_section = match user_instructions {
            Some(instructions) if !instructions.is_empty() =>
                format!("\n用户额外要求（请务必遵循）：\n{}\n", instructions),
            _ => String::new(),
        },
        timeline = timeline_json,
        duration = total_duration,
        count = entries.len(),
    );

    report("agent_generating");

    // Run the agent with multi-turn (allow up to 5 tool call rounds)
    let response = agent
        .prompt(&user_prompt)
        .max_turns(5)
        .await
        .map_err(|e| format!("Agent execution failed: {}", e))?;

    report("extracting_html");

    // Extract HTML from the agent's final response
    let html = super::ai_generate::extract_html(&response)?;

    // Final validation
    match validate_composition(&html) {
        Ok(()) => {
            info!("[Agent] Generation complete, validation passed");
            report("agent_done");
            Ok(html)
        }
        Err(errors) => {
            info!(
                "[Agent] Generation complete with {} validation warnings",
                errors.len()
            );
            report("agent_done_with_warnings");
            Ok(html)
        }
    }
}

/// Build the timeline data as a JSON string for the user prompt.
fn build_timeline_prompt(entries: &[TimelineEntry], total_duration: f64) -> String {
    #[derive(Serialize)]
    struct TimelineData {
        total_duration: f64,
        entries: Vec<EntryData>,
    }

    #[derive(Serialize)]
    struct EntryData {
        text: String,
        start: f64,
        duration: f64,
        character: String,
        section: String,
    }

    let data = TimelineData {
        total_duration,
        entries: entries
            .iter()
            .map(|e| EntryData {
                text: e.text.clone(),
                start: e.start_time,
                duration: e.duration,
                character: e.character_name.clone().unwrap_or_default(),
                section: e.section_title.clone().unwrap_or_default(),
            })
            .collect(),
    };

    serde_json::to_string_pretty(&data).unwrap_or_default()
}
