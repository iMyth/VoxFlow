//! Agentic Hyperframes composition generation using rig.
//!
//! Orchestrates the generation pipeline:
//! 1. Load skills context (system prompt from embedded skill files)
//! 2. Build user prompt (timeline data + visual direction)
//! 3. Call LLM to generate HTML
//! 4. Extract HTML from the response
//! 5. Apply post-processing fixes
//! 6. Validate the result
//!
//! All implementation details are delegated to sibling modules:
//! - `prompt`: SkillsContext, build_user_prompt
//! - `extract`: extract_html
//! - `html_fix`: font fixes, interface injection, duration/clip corrections

use log::info;
use rig_core::client::CompletionClient;
use rig_core::completion::Prompt;
use rig_core::providers;
use serde_json::json;

use super::extract::extract_html;
use super::html_fix::{
    clamp_overflow_clips, ensure_clip_timing, ensure_hyperframes_interfaces, ensure_root_duration,
    fix_css_font_variables, sanitize_unsupported_fonts,
};
use super::prompt::{build_user_prompt, SkillsContext};
use super::timeline::TimelineEntry;
use super::validation::validate_composition;

/// Configuration for the agent-based generation.
pub struct AgentConfig {
    pub api_endpoint: String,
    pub api_key: String,
    pub model: String,
}

/// Run the agent-based Hyperframes generation pipeline.
///
/// `actual_duration_secs` overrides the timeline-computed duration when provided.
/// This should be the ffprobe-measured duration of the merged audio file, ensuring
/// the HTML composition's total duration exactly matches the audio.
pub async fn generate_with_agent(
    entries: &[TimelineEntry],
    config: &AgentConfig,
    on_progress: Option<Box<dyn Fn(&str) + Send + Sync>>,
    user_instructions: Option<&str>,
    actual_duration_secs: Option<f64>,
) -> Result<String, String> {
    let report = |msg: &str| {
        if let Some(ref cb) = on_progress {
            cb(msg);
        }
    };

    report("loading_skills");

    let skills = SkillsContext::new()?;
    info!(
        "[Agent] Skills loaded (system prompt: {} chars)",
        skills.system_prompt.len(),
    );

    report("building_agent");

    let client = providers::openai::CompletionsClient::builder()
        .api_key(&config.api_key)
        .base_url(&config.api_endpoint)
        .build()
        .map_err(|e| format!("Failed to build LLM client: {e}"))?;

    // Single-turn generation — all references are already in the system prompt.
    // Tool calls would waste turns on read_reference instead of generating.
    let agent = client
        .agent(&config.model)
        .preamble(&skills.system_prompt)
        // Temperature 0.4: prioritize format compliance and structural correctness.
        // Visual creativity comes from the rich reference docs in the system prompt,
        // not from high randomness which causes broken GSAP/HTML output.
        .temperature(0.4)
        // Thinking mode enabled: allows the model to reason about complex layouts.
        // The extract_html function strips <think> blocks from the response.
        .additional_params(json!({ "enable_thinking": true }))
        .build();

    // Compute total duration — prefer ffprobe-measured over timeline-computed.
    let timeline_computed_duration: f64 = entries
        .iter()
        .map(|e| e.start_time + e.duration)
        .fold(0.0_f64, f64::max);
    let total_duration = actual_duration_secs.unwrap_or(timeline_computed_duration);

    if let Some(actual) = actual_duration_secs {
        let drift = actual - timeline_computed_duration;
        if drift.abs() > 0.05 {
            info!(
                "[Agent] Using actual audio duration: {:.3}s (timeline computed: {:.3}s, drift: {:.3}s)",
                actual, timeline_computed_duration, drift
            );
        }
    }

    let user_prompt = build_user_prompt(entries, total_duration, user_instructions);

    report("agent_generating");

    // Retry up to 2 times on extraction failure (LLM may return malformed output).
    let max_attempts = 2;
    let mut last_error: Option<String> = None;
    let mut html = String::new();

    for attempt in 1..=max_attempts {
        let start = std::time::Instant::now();
        info!(
            "[Agent] Sending prompt to LLM (model: {}, attempt {}/{})...",
            config.model, attempt, max_attempts
        );

        let response = agent
            .prompt(&user_prompt)
            .max_turns(1)
            .await
            .map_err(|e| format!("Agent execution failed: {e}"))?;
        info!("[Agent] LLM response received in {:?}", start.elapsed());

        report("extracting_html");

        // Log the response for debugging — use char boundary safe truncation
        info!("[Agent] Response length: {} chars", response.len());
        let preview_end = response
            .char_indices()
            .take_while(|(i, _)| *i < 200)
            .last()
            .map(|(i, c)| i + c.len_utf8())
            .unwrap_or(0);
        info!("[Agent] Response preview: {}", &response[..preview_end]);

        match extract_html(&response) {
            Ok(extracted) => {
                html = extracted;
                last_error = None;
                break;
            }
            Err(e) => {
                info!(
                    "[Agent] HTML extraction failed on attempt {}: {}",
                    attempt, e
                );
                last_error = Some(e);
                if attempt < max_attempts {
                    info!("[Agent] Retrying LLM generation...");
                    report("retrying");
                }
            }
        }
    }

    if let Some(err) = last_error {
        return Err(err);
    }

    // Post-process: apply safety-net fixes in dependency order
    html = fix_css_font_variables(&html);
    html = sanitize_unsupported_fonts(&html);
    html = ensure_hyperframes_interfaces(&html, total_duration);
    html = ensure_root_duration(&html, total_duration);
    html = ensure_clip_timing(&html, entries);
    html = clamp_overflow_clips(&html, total_duration);

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
