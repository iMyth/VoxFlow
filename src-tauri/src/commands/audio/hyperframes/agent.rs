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

    // Retry the entire extraction / post-processing / validation pipeline.
    // A failed validation must never be published as a successful composition.
    let mut last_error = String::new();
    for attempt in 1..=2 {
        let prompt = if attempt == 1 {
            user_prompt.clone()
        } else {
            report("retrying");
            format!("{user_prompt}\n\nThe previous attempt failed: {last_error}\nGenerate the entire corrected HTML document, including </body> and </html>. Simplify the visuals if needed to fit the output limit. Return HTML only.")
        };
        info!("[Agent] Generating composition, attempt {}", attempt);
        let response = agent
            .prompt(&prompt)
            .max_turns(1)
            .await
            .map_err(|e| format!("Agent execution failed: {e}"))?;

        report("extracting_html");
        match prepare_composition(&response, entries, total_duration) {
            Ok(html) => {
                report("agent_done");
                return Ok(html);
            }
            Err(error) => {
                info!("[Agent] Attempt {} rejected: {}", attempt, error);
                last_error = error;
            }
        }
    }
    Err(format!(
        "Composition generation failed after 2 attempts: {last_error}"
    ))
}

fn prepare_composition(
    response: &str,
    entries: &[TimelineEntry],
    duration: f64,
) -> Result<String, String> {
    let mut html = extract_html(response)?;
    html = fix_css_font_variables(&html);
    html = sanitize_unsupported_fonts(&html);
    html = ensure_hyperframes_interfaces(&html, duration);
    html = ensure_root_duration(&html, duration);
    html = ensure_clip_timing(&html, entries);
    html = clamp_overflow_clips(&html, duration);
    validate_composition(&html).map_err(|errors| errors.join("\n"))?;
    Ok(html)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_complete_document_without_composition() {
        assert!(prepare_composition(
            "<!DOCTYPE html><html><head><meta charset=\"UTF-8\"></head><body>hello</body></html>",
            &[],
            5.0
        )
        .is_err());
    }

    #[test]
    fn rejects_truncation_before_injecting_safety_net() {
        assert!(
            prepare_composition("<!DOCTYPE html><html><head><style>.scene{top:", &[], 5.0)
                .unwrap_err()
                .contains("incomplete")
        );
    }
}
