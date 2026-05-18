//! Worker module for the LLM orchestration pipeline.
//!
//! Responsible for executing individual chunk generation: building prompts with
//! visual directives, calling the LLM, validating output, and retrying on failure.

use futures_util::stream::{self, StreamExt};
use log::info;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use serde_json::json;
use std::time::Instant;

use super::ai_generate::{extract_html, LlmConfig};
use super::pipeline_types::{
    ChunkProgress, PipelineProgress, PipelineStage, TokenBudget, WorkerInput, WorkerOutput,
    WorkerResult,
};
use super::prompt::build_system_prompt;
use super::validation::validate_composition;

/// Build the Worker system prompt.
///
/// Reuses the existing Hyperframes creative prompt from `prompt.rs`,
/// which defines the LLM's role as a video visual creation expert.
pub fn build_worker_system_prompt() -> String {
    build_system_prompt()
}

/// Build the Worker user prompt for a specific chunk.
///
/// Includes:
/// - Visual directive (palette, style keywords, rhythm, concept, transitions)
/// - Previous chunk's ending palette (for transition context)
/// - Timeline entries as JSON
pub fn build_worker_user_prompt(input: &WorkerInput) -> String {
    let directive = &input.chunk_plan.visual_directive;
    let transition_in = &input.chunk_plan.transition_in;
    let transition_out = &input.chunk_plan.transition_out;

    // Build visual directive section
    let palette_str = directive.palette.join(", ");
    let keywords_str = directive.style_keywords.join(", ");

    let transition_in_str = if let Some(ref prev_palette) = input.prev_ending_palette {
        format!(
            "{}, 从上一段的 {} 渐入",
            transition_in.transition_type,
            prev_palette.last().unwrap_or(&"#000000".to_string())
        )
    } else {
        format!("{} (首段，无前序)", transition_in.transition_type)
    };

    let transition_out_str = format!(
        "{}, 向 {} 渐变",
        transition_out.transition_type,
        transition_out
            .colors
            .first()
            .unwrap_or(&"#000000".to_string())
    );

    // Build timeline entries as JSON
    let entries_json: Vec<serde_json::Value> = input
        .entries
        .iter()
        .map(|e| {
            json!({
                "text": e.text,
                "start": e.start_time,
                "duration": e.duration,
                "character": e.character_name.as_deref().unwrap_or("")
            })
        })
        .collect();

    let chunk_start = input
        .entries
        .iter()
        .map(|e| e.start_time)
        .fold(f64::INFINITY, f64::min);
    let chunk_end = input
        .entries
        .iter()
        .map(|e| e.start_time + e.duration)
        .fold(0.0_f64, f64::max);
    let chunk_duration = chunk_end - chunk_start;

    let entries_str = serde_json::to_string_pretty(&entries_json).unwrap_or_default();

    format!(
        r#"这是一个编排式分段生成任务（第 {}/{} 段）。

[视觉指令]
色彩方案: {palette}
风格关键词: {keywords}
动画节奏: {rhythm}
视觉概念: {concept}
过渡入场: {trans_in}
过渡退场: {trans_out}

[约束]
- 只使用上述色彩方案中的颜色（可以调整明度/透明度）
- 风格关键词必须体现在视觉设计中
- 动画节奏决定了动画速度和 stagger 间隔
- composition 的 data-start 应为 "0"，data-duration 应为 "{total_duration}"
- 本段 clip 的 data-start 从 {chunk_start:.1} 秒开始
- GSAP timeline 中所有动画的时间偏移使用绝对时间（从 {chunk_start:.1} 秒开始）
- 本段时长约 {chunk_duration:.0} 秒

[时间轴数据]
{entries}

请为这个片段创作视觉画面。输出完整 HTML 文件。"#,
        input.chunk_plan.index + 1,
        input.total_chunks,
        palette = palette_str,
        keywords = keywords_str,
        rhythm = directive.rhythm,
        concept = directive.concept,
        trans_in = transition_in_str,
        trans_out = transition_out_str,
        total_duration = input.total_duration,
        chunk_start = chunk_start,
        chunk_duration = chunk_duration,
        entries = entries_str,
    )
}

/// Detect if the LLM response appears truncated (missing closing tags).
fn is_truncated(html: &str) -> bool {
    let trimmed = html.trim();
    // Check for missing essential closing tags
    let has_html_close = trimmed.contains("</html>");
    let has_body_close = trimmed.contains("</body>");

    // If it starts like HTML but is missing closing tags, it's truncated
    if trimmed.contains("<!DOCTYPE") || trimmed.contains("<html") {
        return !has_html_close || !has_body_close;
    }

    false
}

/// Execute a single Worker LLM call with validation and retry logic.
///
/// Retry strategy:
/// - On validation failure: retry up to 2 times with error feedback appended
/// - On truncation: retry with halved chunk size (up to 2 attempts)
/// - Sets max_tokens to 90% of worker_output budget
pub async fn run_worker(
    input: &WorkerInput,
    config: &LlmConfig<'_>,
    token_budget: &TokenBudget,
) -> WorkerResult {
    let worker_start = Instant::now();
    let chunk_index = input.chunk_plan.index;
    let entry_count = input.entries.len();
    info!(
        "[Hyperframes Worker] Chunk {} starting: {} entries",
        chunk_index, entry_count
    );

    let system_prompt = build_worker_system_prompt();
    let user_prompt = build_worker_user_prompt(input);
    let max_tokens = (token_budget.worker_output as f64 * 0.9) as usize;

    const MAX_RETRIES: usize = 2;
    let mut errors: Vec<String> = Vec::new();
    let mut current_prompt = user_prompt.clone();
    let mut retries_used = 0;

    for attempt in 0..=MAX_RETRIES {
        // Call LLM
        let response =
            match call_worker_llm(&system_prompt, &current_prompt, config, max_tokens).await {
                Ok(text) => text,
                Err(e) => {
                    errors.push(format!("LLM call failed (attempt {}): {}", attempt + 1, e));
                    retries_used = attempt;
                    continue;
                }
            };

        // Check for truncation
        if is_truncated(&response) {
            errors.push(format!(
                "Response truncated (attempt {}): missing closing tags",
                attempt + 1
            ));
            retries_used = attempt;

            // On truncation, append a note about being more concise
            current_prompt = format!(
                "{}\n\n---\n\n注意：上次生成的内容被截断（缺少闭合标签）。请精简视觉元素数量，确保输出完整的 HTML。",
                user_prompt
            );
            continue;
        }

        // Extract HTML
        let html = match extract_html(&response) {
            Ok(h) => h,
            Err(e) => {
                errors.push(format!(
                    "HTML extraction failed (attempt {}): {}",
                    attempt + 1,
                    e
                ));
                retries_used = attempt;
                current_prompt = format!(
                    "{}\n\n---\n\n你上次的输出无法提取有效 HTML：{}。请直接输出完整 HTML 文件（从 <!DOCTYPE html> 开始）。",
                    user_prompt, e
                );
                continue;
            }
        };

        // Validate
        match validate_composition(&html) {
            Ok(()) => {
                info!(
                    "[Hyperframes Worker] Chunk {} succeeded: {:.1}s, retries={}",
                    chunk_index,
                    worker_start.elapsed().as_secs_f64(),
                    retries_used
                );
                return WorkerResult::Success(WorkerOutput {
                    chunk_index: input.chunk_plan.index,
                    html,
                    retries_used,
                });
            }
            Err(validation_errors) => {
                let error_list = validation_errors.join("\n- ");
                errors.push(format!(
                    "Validation failed (attempt {}): {}",
                    attempt + 1,
                    error_list
                ));
                retries_used = attempt;

                // Build retry prompt with error feedback
                current_prompt = format!(
                    "{}\n\n---\n\n你上次生成的 HTML 存在以下问题，请修正后重新输出完整 HTML：\n- {}",
                    user_prompt, error_list
                );
            }
        }
    }

    info!(
        "[Hyperframes Worker] Chunk {} failed: {:.1}s, {} errors",
        chunk_index,
        worker_start.elapsed().as_secs_f64(),
        errors.len()
    );

    WorkerResult::Failed {
        chunk_index: input.chunk_plan.index,
        errors,
        retries_exhausted: true,
    }
}

/// Default concurrency cap for parallel worker execution.
pub const DEFAULT_CONCURRENCY_CAP: usize = 5;

/// Execute all Workers concurrently with a configurable concurrency cap.
///
/// Uses `futures::stream::buffer_unordered` to limit parallel requests.
/// Emits `PipelineProgress` events after each worker completes or fails.
/// Continues processing remaining chunks when one fails.
/// Emits a summary event when all workers complete.
pub async fn run_workers_concurrent(
    inputs: Vec<WorkerInput>,
    config: &LlmConfig<'_>,
    token_budget: &TokenBudget,
    concurrency_cap: usize,
    on_progress: &(dyn Fn(PipelineProgress) + Send + Sync),
) -> Vec<WorkerResult> {
    let total_chunks = inputs.len();
    let chunks_completed = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let chunks_failed = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));

    // Emit initial progress
    on_progress(PipelineProgress {
        stage: PipelineStage::GeneratingChunk,
        message: format!(
            "开始并发生成 {} 个片段（并发上限 {}）...",
            total_chunks, concurrency_cap
        ),
        percent: 0.0,
        chunk_info: Some(ChunkProgress {
            chunk_index: 0,
            total_chunks,
            chunks_completed: 0,
            chunks_failed: 0,
        }),
    });

    let results: Vec<WorkerResult> = stream::iter(inputs)
        .map(|input| {
            let completed = chunks_completed.clone();
            let failed = chunks_failed.clone();
            async move {
                let chunk_index = input.chunk_plan.index;
                let result = run_worker(&input, config, token_budget).await;

                match &result {
                    WorkerResult::Success(_) => {
                        completed.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    }
                    WorkerResult::Failed { .. } => {
                        failed.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    }
                }

                (chunk_index, result)
            }
        })
        .buffer_unordered(concurrency_cap)
        .map(|(chunk_index, result)| {
            let completed = chunks_completed.load(std::sync::atomic::Ordering::Relaxed);
            let failed_count = chunks_failed.load(std::sync::atomic::Ordering::Relaxed);

            let progress_percent =
                ((completed + failed_count) as f32 / total_chunks as f32) * 100.0;

            match &result {
                WorkerResult::Success(_) => {
                    on_progress(PipelineProgress {
                        stage: PipelineStage::ChunkCompleted,
                        message: format!("第 {} 段生成完成", chunk_index + 1),
                        percent: progress_percent,
                        chunk_info: Some(ChunkProgress {
                            chunk_index,
                            total_chunks,
                            chunks_completed: completed,
                            chunks_failed: failed_count,
                        }),
                    });
                }
                WorkerResult::Failed { errors, .. } => {
                    let reason = errors.last().cloned().unwrap_or_default();
                    on_progress(PipelineProgress {
                        stage: PipelineStage::ChunkFailed,
                        message: format!("第 {} 段生成失败: {}", chunk_index + 1, reason),
                        percent: progress_percent,
                        chunk_info: Some(ChunkProgress {
                            chunk_index,
                            total_chunks,
                            chunks_completed: completed,
                            chunks_failed: failed_count,
                        }),
                    });
                }
            }

            result
        })
        .collect()
        .await;

    // Emit summary event
    let final_completed = chunks_completed.load(std::sync::atomic::Ordering::Relaxed);
    let final_failed = chunks_failed.load(std::sync::atomic::Ordering::Relaxed);

    on_progress(PipelineProgress {
        stage: PipelineStage::Complete,
        message: format!(
            "Worker 阶段完成：共 {} 段，成功 {}，失败 {}",
            total_chunks, final_completed, final_failed
        ),
        percent: 100.0,
        chunk_info: Some(ChunkProgress {
            chunk_index: 0,
            total_chunks,
            chunks_completed: final_completed,
            chunks_failed: final_failed,
        }),
    });

    results
}

/// Call the Worker LLM via SSE streaming.
///
/// Uses the same streaming pattern as `call_llm` in ai_generate.rs.
async fn call_worker_llm(
    system_prompt: &str,
    user_prompt: &str,
    config: &LlmConfig<'_>,
    max_tokens: usize,
) -> Result<String, String> {
    let call_start = Instant::now();

    let url = format!(
        "{}/chat/completions",
        config.api_endpoint.trim_end_matches('/')
    );

    let body = json!({
        "model": config.model,
        "messages": [
            { "role": "system", "content": system_prompt },
            { "role": "user", "content": user_prompt }
        ],
        "stream": true,
        "max_tokens": max_tokens,
        "temperature": 0.85
    });

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(180))
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {}", e))?;
    let response = client
        .post(&url)
        .header(CONTENT_TYPE, "application/json")
        .header(AUTHORIZATION, format!("Bearer {}", config.api_key))
        .body(body.to_string())
        .send()
        .await
        .map_err(|e| format!("LLM request failed: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let body_text = response.text().await.unwrap_or_default();
        return Err(format!("LLM API error {}: {}", status, body_text));
    }

    // Stream SSE response and accumulate content with line buffering
    let mut accumulated_text = String::new();
    let mut stream = response.bytes_stream();
    let mut line_buffer = String::new();

    while let Some(chunk_result) = stream.next().await {
        let chunk = chunk_result.map_err(|e| format!("Failed to read LLM response: {}", e))?;

        let body_str = String::from_utf8_lossy(&chunk);
        line_buffer.push_str(&body_str);

        // Process only complete lines (ending with \n)
        while let Some(newline_pos) = line_buffer.find('\n') {
            let line = line_buffer[..newline_pos].trim().to_string();
            line_buffer = line_buffer[newline_pos + 1..].to_string();

            if line.is_empty() || line == "data: [DONE]" {
                continue;
            }
            if let Some(data) = line.strip_prefix("data: ") {
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(data) {
                    if let Some(content) = parsed["choices"][0]["delta"]["content"].as_str() {
                        accumulated_text.push_str(content);
                    }
                }
            }
        }
    }

    // Process any remaining data in the buffer
    let remaining = line_buffer.trim().to_string();
    if !remaining.is_empty() && remaining != "data: [DONE]" {
        if let Some(data) = remaining.strip_prefix("data: ") {
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(data) {
                if let Some(content) = parsed["choices"][0]["delta"]["content"].as_str() {
                    accumulated_text.push_str(content);
                }
            }
        }
    }

    if accumulated_text.is_empty() {
        return Err("LLM returned empty response".to_string());
    }

    info!(
        "[Hyperframes Worker LLM] Call completed: {:.1}s, output={}chars",
        call_start.elapsed().as_secs_f64(),
        accumulated_text.len()
    );

    Ok(accumulated_text)
}

#[cfg(test)]
mod tests {
    use super::super::pipeline_types::{ChunkPlan, TransitionSpec, VisualDirective};
    use super::super::timeline::TimelineEntry;
    use super::*;

    fn make_test_input() -> WorkerInput {
        WorkerInput {
            chunk_plan: ChunkPlan {
                index: 0,
                entry_start: 0,
                entry_end: 3,
                visual_directive: VisualDirective {
                    palette: vec![
                        "#1a1a2e".to_string(),
                        "#16213e".to_string(),
                        "#0f3460".to_string(),
                        "#e94560".to_string(),
                    ],
                    style_keywords: vec![
                        "organic".to_string(),
                        "flowing".to_string(),
                        "luminescent".to_string(),
                    ],
                    rhythm: "moderate".to_string(),
                    concept: "深海中的生物发光，水母群缓慢漂浮".to_string(),
                },
                transition_in: TransitionSpec {
                    transition_type: "fade".to_string(),
                    colors: vec!["#000000".to_string()],
                },
                transition_out: TransitionSpec {
                    transition_type: "dissolve".to_string(),
                    colors: vec!["#e94560".to_string()],
                },
            },
            entries: vec![
                TimelineEntry {
                    line_id: "line_0".to_string(),
                    text: "在深海的最深处".to_string(),
                    character_name: Some("旁白".to_string()),
                    section_title: Some("第一章".to_string()),
                    start_time: 0.0,
                    duration: 3.0,
                },
                TimelineEntry {
                    line_id: "line_1".to_string(),
                    text: "有一群发光的水母".to_string(),
                    character_name: Some("旁白".to_string()),
                    section_title: Some("第一章".to_string()),
                    start_time: 3.5,
                    duration: 2.5,
                },
                TimelineEntry {
                    line_id: "line_2".to_string(),
                    text: "它们缓缓漂浮着".to_string(),
                    character_name: None,
                    section_title: Some("第一章".to_string()),
                    start_time: 6.5,
                    duration: 2.0,
                },
            ],
            total_duration: 120.0,
            prev_ending_palette: None,
            total_chunks: 5,
        }
    }

    #[test]
    fn test_build_worker_system_prompt_reuses_existing() {
        let prompt = build_worker_system_prompt();
        // Should contain the same content as prompt.rs build_system_prompt()
        assert!(prompt.contains("视觉艺术家"));
        assert!(prompt.contains("Hyperframes"));
        assert!(prompt.contains("data-composition-id"));
    }

    #[test]
    fn test_build_worker_user_prompt_includes_visual_directive() {
        let input = make_test_input();
        let prompt = build_worker_user_prompt(&input);

        // Should include palette colors
        assert!(prompt.contains("#1a1a2e"));
        assert!(prompt.contains("#16213e"));
        assert!(prompt.contains("#0f3460"));
        assert!(prompt.contains("#e94560"));

        // Should include style keywords
        assert!(prompt.contains("organic"));
        assert!(prompt.contains("flowing"));
        assert!(prompt.contains("luminescent"));

        // Should include rhythm
        assert!(prompt.contains("moderate"));

        // Should include concept
        assert!(prompt.contains("深海中的生物发光"));

        // Should include transition info
        assert!(prompt.contains("fade"));
        assert!(prompt.contains("dissolve"));
    }

    #[test]
    fn test_build_worker_user_prompt_includes_entries() {
        let input = make_test_input();
        let prompt = build_worker_user_prompt(&input);

        // Should include entry text
        assert!(prompt.contains("在深海的最深处"));
        assert!(prompt.contains("有一群发光的水母"));
        assert!(prompt.contains("它们缓缓漂浮着"));
    }

    #[test]
    fn test_build_worker_user_prompt_includes_chunk_info() {
        let input = make_test_input();
        let prompt = build_worker_user_prompt(&input);

        // Should include chunk index info
        assert!(prompt.contains("第 1/5 段"));
        // Should include total duration
        assert!(prompt.contains("120"));
    }

    #[test]
    fn test_build_worker_user_prompt_with_prev_palette() {
        let mut input = make_test_input();
        input.prev_ending_palette = Some(vec!["#0f3460".to_string(), "#16213e".to_string()]);
        input.chunk_plan.index = 1;

        let prompt = build_worker_user_prompt(&input);

        // Should reference previous palette in transition-in
        assert!(prompt.contains("从上一段的"));
        assert!(prompt.contains("#16213e")); // last color of prev palette
    }

    #[test]
    fn test_build_worker_user_prompt_first_chunk_no_prev() {
        let input = make_test_input();
        let prompt = build_worker_user_prompt(&input);

        // First chunk should indicate no previous context
        assert!(prompt.contains("首段"));
    }

    #[test]
    fn test_is_truncated_complete_html() {
        let html = "<!DOCTYPE html><html><head></head><body></body></html>";
        assert!(!is_truncated(html));
    }

    #[test]
    fn test_is_truncated_missing_html_close() {
        let html = "<!DOCTYPE html><html><head></head><body><div>content</div></body>";
        assert!(is_truncated(html));
    }

    #[test]
    fn test_is_truncated_missing_body_close() {
        let html = "<!DOCTYPE html><html><head></head><body><div>content</div></html>";
        assert!(is_truncated(html));
    }

    #[test]
    fn test_is_truncated_non_html_content() {
        let text = "This is just plain text without HTML structure";
        assert!(!is_truncated(text));
    }
}
