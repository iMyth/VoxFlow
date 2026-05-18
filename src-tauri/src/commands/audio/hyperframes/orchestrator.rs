//! Orchestrator module for the LLM orchestration pipeline.
//!
//! Responsible for building prompts that instruct the LLM to act as a "visual director",
//! planning how to split and style a multi-segment Hyperframes composition.

use std::collections::HashSet;
use std::time::Instant;

use log::info;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use serde_json::json;

use super::ai_generate::LlmConfig;
use super::pipeline_types::{OrchestrationPlan, PipelineError, TokenBudget};
use super::timeline::TimelineEntry;

/// Build the Orchestrator system prompt.
///
/// Defines the LLM's role as a "visual director" planning a multi-segment composition,
/// specifies the output format as strict JSON matching the OrchestrationPlan schema,
/// and lists constraints including token budget per chunk, visual continuity rules,
/// and the Hyperframes style vocabulary.
pub fn build_orchestrator_system_prompt() -> String {
    r##"你是一位视觉导演（Visual Director），负责规划一个多段式 Hyperframes 动态视觉作品的整体编排方案。

## 你的职责

分析完整的时间轴数据，制定分片策略和每个片段的视觉指令，确保最终作品在视觉上连贯统一。

## 输出格式

你必须输出严格的 JSON，符合以下 schema：

```json
{
  "global_theme": {
    "mood": ["string"],
    "shared_motifs": ["string"],
    "color_progression": {
      "start_palette": ["#hex"],
      "end_palette": ["#hex"]
    }
  },
  "chunks": [
    {
      "index": 0,
      "entry_start": 0,
      "entry_end": 5,
      "visual_directive": {
        "palette": ["#hex", "#hex", "#hex"],
        "style_keywords": ["keyword1", "keyword2"],
        "rhythm": "moderate",
        "concept": "描述这个片段的视觉概念"
      },
      "transition_in": {
        "transition_type": "fade",
        "colors": ["#hex"]
      },
      "transition_out": {
        "transition_type": "dissolve",
        "colors": ["#hex"]
      }
    }
  ]
}
```

## 约束条件

1. **Token 预算**：每个 chunk 的 entry 文本总量不应超过 Worker 输入预算的 80%。如果某段内容过长，需要进一步拆分。
2. **视觉连贯性规则**：
   - 相邻 chunk 的调色板必须共享至少 2 种颜色
   - 至少 50% 的 chunk 必须包含 shared_motifs 中的共同视觉母题
   - 全局色彩应从 start_palette 自然过渡到 end_palette
3. **调色板规范**：每个 chunk 的 palette 包含 3-6 个十六进制颜色值
4. **风格关键词**：每个 chunk 最多 5 个风格关键词
5. **节奏描述符**：必须是 "slow"、"moderate"、"fast" 或 "dynamic" 之一
6. **过渡类型**：可选值包括 "fade"、"wipe-left"、"wipe-right"、"dissolve"、"morph"、"slide-up"、"slide-down"、"zoom-in"、"zoom-out"
7. **Entry 范围**：所有 chunk 的 [entry_start, entry_end) 必须连续覆盖 [0, N)，不能有间隙或重叠
8. **Chunk 索引**：chunk.index 必须从 0 开始连续递增

## Hyperframes 风格词汇表

可用的风格关键词包括：organic, geometric, flowing, angular, luminescent, matte, translucent, textured, minimal, layered, kinetic, static, rhythmic, chaotic, symmetrical, asymmetrical, gradient, flat, dimensional, ethereal, bold, subtle, warm, cool, vibrant, muted, retro, futuristic, natural, synthetic

仅输出 JSON，不要包含任何其他文本或解释。"##.to_string()
}

/// Build the Orchestrator user prompt from timeline entries.
///
/// If the entries' total token cost exceeds 50% of the orchestrator_input budget,
/// the prompt summarizes the content (entry count, time ranges, section labels).
/// Otherwise, it includes the full text of all entries.
///
/// Always includes total duration, entry count, and token budget parameters.
pub fn build_orchestrator_user_prompt(
    entries: &[TimelineEntry],
    token_budget: &TokenBudget,
) -> String {
    let entry_count = entries.len();
    let total_duration = entries
        .iter()
        .map(|e| e.start_time + e.duration)
        .fold(0.0_f64, f64::max);

    let estimated_tokens = estimate_token_cost(entries);
    let budget_threshold = token_budget.orchestrator_input / 2;

    let timeline_content = if estimated_tokens > budget_threshold {
        // Summarize: entry count, time ranges, section labels
        build_summarized_timeline(entries, total_duration)
    } else {
        // Include full text
        build_full_timeline(entries)
    };

    format!(
        r#"## 时间轴信息

- 总条目数：{}
- 总时长：{:.1} 秒
- 预估 Token 数：{}

## Token 预算参数

- Orchestrator 输入预算：{} tokens
- Worker 输入预算：{} tokens（每个 chunk 的 entry 文本不应超过其 80% = {} tokens）
- Worker 输出预算：{} tokens

## 时间轴数据

{}

请根据以上时间轴数据，输出完整的编排计划 JSON。"#,
        entry_count,
        total_duration,
        estimated_tokens,
        token_budget.orchestrator_input,
        token_budget.worker_input,
        (token_budget.worker_input as f64 * 0.8) as usize,
        token_budget.worker_output,
        timeline_content,
    )
}

/// Estimate the token cost for a set of timeline entries.
///
/// Uses ~4 chars per token heuristic for mixed CJK/English content.
/// Sums: entry text length + character_name length + ~50 overhead per entry.
/// Returns total / 4.
pub fn estimate_token_cost(entries: &[TimelineEntry]) -> usize {
    let total_chars: usize = entries
        .iter()
        .map(|e| {
            e.text.len() + e.character_name.as_ref().map_or(0, |n| n.len()) + 50
            // overhead per entry (field names, formatting, etc.)
        })
        .sum();
    total_chars / 4
}

/// Build a summarized timeline representation for when entries exceed the budget threshold.
fn build_summarized_timeline(entries: &[TimelineEntry], total_duration: f64) -> String {
    let mut sections: Vec<String> = Vec::new();

    // Group entries by section_title
    let mut current_section: Option<&str> = None;
    let mut section_start_idx = 0;
    let mut section_start_time = 0.0_f64;

    for (i, entry) in entries.iter().enumerate() {
        let section = entry.section_title.as_deref().unwrap_or("（无标题）");

        if current_section != Some(section) {
            // Close previous section
            if let Some(prev_section) = current_section {
                let prev_end_time = entry.start_time;
                sections.push(format!(
                    "- 段落「{}」: entries[{}..{}], 时间 {:.1}s - {:.1}s",
                    prev_section, section_start_idx, i, section_start_time, prev_end_time
                ));
            }
            current_section = Some(section);
            section_start_idx = i;
            section_start_time = entry.start_time;
        }
    }

    // Close the last section
    if let Some(section) = current_section {
        sections.push(format!(
            "- 段落「{}」: entries[{}..{}], 时间 {:.1}s - {:.1}s",
            section,
            section_start_idx,
            entries.len(),
            section_start_time,
            total_duration
        ));
    }

    format!(
        "（内容已摘要化，因原始文本超出预算 50%）\n\n共 {} 条 entries，总时长 {:.1} 秒\n\n### 段落概览\n\n{}",
        entries.len(),
        total_duration,
        sections.join("\n")
    )
}

/// Build the full timeline representation including all entry text.
fn build_full_timeline(entries: &[TimelineEntry]) -> String {
    let entry_lines: Vec<String> = entries
        .iter()
        .enumerate()
        .map(|(i, e)| {
            let character = e.character_name.as_deref().unwrap_or("旁白");
            let section = e.section_title.as_deref().unwrap_or("");
            let section_info = if section.is_empty() {
                String::new()
            } else {
                format!(" [{}]", section)
            };
            format!(
                "[{}] ({}) {:.1}s-{:.1}s{}: {}",
                i,
                character,
                e.start_time,
                e.start_time + e.duration,
                section_info,
                e.text
            )
        })
        .collect();

    entry_lines.join("\n")
}

/// Valid rhythm values for chunk visual directives.
const VALID_RHYTHMS: &[&str] = &["slow", "moderate", "fast", "dynamic"];

/// Call the Orchestrator LLM and parse the response into a validated plan.
///
/// Steps:
/// 1. If entries is empty, return PipelineError::Other
/// 2. Build system + user prompts
/// 3. Call the LLM API (non-streaming, JSON mode)
/// 4. Check for context-length-exceeded errors
/// 5. Parse JSON response into OrchestrationPlan
/// 6. Validate the plan
/// 7. Return the validated plan
pub async fn run_orchestrator(
    entries: &[TimelineEntry],
    config: &LlmConfig<'_>,
    token_budget: &TokenBudget,
) -> Result<OrchestrationPlan, PipelineError> {
    let start_time = Instant::now();

    // Requirement 1.7: empty timeline returns error
    if entries.is_empty() {
        return Err(PipelineError::Other("Timeline is empty".to_string()));
    }

    info!(
        "[Hyperframes Orchestrator] Starting: {} entries, estimated {} tokens",
        entries.len(),
        estimate_token_cost(entries)
    );

    let system_prompt = build_orchestrator_system_prompt();
    let user_prompt = build_orchestrator_user_prompt(entries, token_budget);

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
        "temperature": 0.7,
        "response_format": { "type": "json_object" }
    });

    let client = reqwest::Client::new();
    let response = client
        .post(&url)
        .header(CONTENT_TYPE, "application/json")
        .header(AUTHORIZATION, format!("Bearer {}", config.api_key))
        .body(body.to_string())
        .send()
        .await
        .map_err(|e| PipelineError::OrchestratorFailed(format!("Request failed: {}", e)))?;

    if !response.status().is_success() {
        let status = response.status();
        let body_text = response.text().await.unwrap_or_default();

        // Requirement 7.2 / 5.3: detect context-length-exceeded errors
        if body_text.contains("context_length_exceeded")
            || body_text.contains("maximum context length")
            || body_text.contains("token limit")
        {
            return Err(PipelineError::ContextLengthExceeded(format!(
                "LLM API error {}: {}",
                status, body_text
            )));
        }

        return Err(PipelineError::OrchestratorFailed(format!(
            "LLM API error {}: {}",
            status, body_text
        )));
    }

    let response_text = response.text().await.map_err(|e| {
        PipelineError::OrchestratorFailed(format!("Failed to read response: {}", e))
    })?;

    // Parse the OpenAI-compatible response to extract the content
    let response_json: serde_json::Value = serde_json::from_str(&response_text).map_err(|e| {
        PipelineError::OrchestratorFailed(format!("Failed to parse API response: {}", e))
    })?;

    let content = response_json["choices"][0]["message"]["content"]
        .as_str()
        .ok_or_else(|| {
            PipelineError::OrchestratorFailed("No content in LLM response".to_string())
        })?;

    // Parse the JSON content into OrchestrationPlan
    let plan: OrchestrationPlan = serde_json::from_str(content)
        .map_err(|e| PipelineError::InvalidPlan(format!("Failed to parse plan JSON: {}", e)))?;

    // Validate the plan
    validate_plan(&plan, entries.len()).map_err(PipelineError::InvalidPlan)?;

    info!(
        "[Hyperframes Orchestrator] Completed: {:.1}s, {} chunks",
        start_time.elapsed().as_secs_f64(),
        plan.chunks.len()
    );

    Ok(plan)
}

/// Validate that the orchestration plan is internally consistent.
///
/// Checks:
/// - Chunks are not empty
/// - Chunk indices are sequential (0, 1, 2, ...)
/// - Entry ranges are contiguous: chunk[0].entry_start == 0, chunk[i].entry_start == chunk[i-1].entry_end, last chunk.entry_end == entry_count
/// - Each chunk's palette has 3-6 colors
/// - Each chunk's rhythm is one of: "slow", "moderate", "fast", "dynamic"
/// - Adjacent chunks share at least 2 colors in their palettes
pub fn validate_plan(plan: &OrchestrationPlan, entry_count: usize) -> Result<(), String> {
    // Verify chunks are not empty
    if plan.chunks.is_empty() {
        return Err("Plan contains no chunks".to_string());
    }

    // Verify chunk indices are sequential (0, 1, 2, ...)
    for (i, chunk) in plan.chunks.iter().enumerate() {
        if chunk.index != i {
            return Err(format!(
                "Chunk index mismatch: expected {}, got {}",
                i, chunk.index
            ));
        }
    }

    // Verify entry ranges are contiguous and cover [0, entry_count)
    if plan.chunks[0].entry_start != 0 {
        return Err(format!(
            "First chunk entry_start must be 0, got {}",
            plan.chunks[0].entry_start
        ));
    }

    for i in 1..plan.chunks.len() {
        if plan.chunks[i].entry_start != plan.chunks[i - 1].entry_end {
            return Err(format!(
                "Gap or overlap between chunk {} (entry_end={}) and chunk {} (entry_start={})",
                i - 1,
                plan.chunks[i - 1].entry_end,
                i,
                plan.chunks[i].entry_start
            ));
        }
    }

    let last_chunk = plan.chunks.last().unwrap();
    if last_chunk.entry_end != entry_count {
        return Err(format!(
            "Last chunk entry_end must be {}, got {}",
            entry_count, last_chunk.entry_end
        ));
    }

    // Verify each chunk's palette has 3-6 colors and rhythm is valid
    for chunk in &plan.chunks {
        let palette_len = chunk.visual_directive.palette.len();
        if !(3..=6).contains(&palette_len) {
            return Err(format!(
                "Chunk {} palette must have 3-6 colors, got {}",
                chunk.index, palette_len
            ));
        }

        if !VALID_RHYTHMS.contains(&chunk.visual_directive.rhythm.as_str()) {
            return Err(format!(
                "Chunk {} has invalid rhythm '{}', must be one of: slow, moderate, fast, dynamic",
                chunk.index, chunk.visual_directive.rhythm
            ));
        }
    }

    // Verify adjacent chunks share at least 2 colors in their palettes
    for i in 0..plan.chunks.len() - 1 {
        let palette_a: HashSet<&str> = plan.chunks[i]
            .visual_directive
            .palette
            .iter()
            .map(|s| s.as_str())
            .collect();
        let palette_b: HashSet<&str> = plan.chunks[i + 1]
            .visual_directive
            .palette
            .iter()
            .map(|s| s.as_str())
            .collect();

        let shared_count = palette_a.intersection(&palette_b).count();
        if shared_count < 2 {
            return Err(format!(
                "Adjacent chunks {} and {} share only {} color(s), need at least 2",
                i,
                i + 1,
                shared_count
            ));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::pipeline_types::{
        ChunkPlan, ColorProgression, GlobalTheme, OrchestrationPlan, TransitionSpec,
        VisualDirective,
    };
    use super::*;

    fn make_entry(
        text: &str,
        character_name: Option<&str>,
        start: f64,
        duration: f64,
    ) -> TimelineEntry {
        TimelineEntry {
            line_id: "test-id".to_string(),
            text: text.to_string(),
            character_name: character_name.map(|s| s.to_string()),
            section_title: None,
            start_time: start,
            duration,
        }
    }

    fn make_chunk(
        index: usize,
        entry_start: usize,
        entry_end: usize,
        palette: Vec<&str>,
        rhythm: &str,
    ) -> ChunkPlan {
        ChunkPlan {
            index,
            entry_start,
            entry_end,
            visual_directive: VisualDirective {
                palette: palette.into_iter().map(|s| s.to_string()).collect(),
                style_keywords: vec!["organic".to_string()],
                rhythm: rhythm.to_string(),
                concept: "test concept".to_string(),
            },
            transition_in: TransitionSpec {
                transition_type: "fade".to_string(),
                colors: vec!["#000000".to_string()],
            },
            transition_out: TransitionSpec {
                transition_type: "dissolve".to_string(),
                colors: vec!["#ffffff".to_string()],
            },
        }
    }

    fn make_valid_plan(entry_count: usize, chunk_count: usize) -> OrchestrationPlan {
        let entries_per_chunk = entry_count / chunk_count;
        let shared_colors = vec!["#aaaaaa", "#bbbbbb"];

        let chunks: Vec<ChunkPlan> = (0..chunk_count)
            .map(|i| {
                let entry_start = i * entries_per_chunk;
                let entry_end = if i == chunk_count - 1 {
                    entry_count
                } else {
                    (i + 1) * entries_per_chunk
                };
                // Each chunk has the shared colors plus one unique color
                let mut palette = shared_colors.clone();
                palette.push(if i % 2 == 0 { "#cc0000" } else { "#00cc00" });
                make_chunk(i, entry_start, entry_end, palette, "moderate")
            })
            .collect();

        OrchestrationPlan {
            global_theme: GlobalTheme {
                mood: vec!["epic".to_string()],
                shared_motifs: vec!["stars".to_string()],
                color_progression: ColorProgression {
                    start_palette: vec!["#000000".to_string()],
                    end_palette: vec!["#ffffff".to_string()],
                },
            },
            chunks,
        }
    }

    #[test]
    fn test_estimate_token_cost_empty() {
        let entries: Vec<TimelineEntry> = vec![];
        assert_eq!(estimate_token_cost(&entries), 0);
    }

    #[test]
    fn test_estimate_token_cost_single_entry() {
        // text = "Hello world" (11 bytes) + character_name = "Alice" (5 bytes) + 50 overhead = 66
        // 66 / 4 = 16
        let entries = vec![make_entry("Hello world", Some("Alice"), 0.0, 1.0)];
        assert_eq!(estimate_token_cost(&entries), 16);
    }

    #[test]
    fn test_estimate_token_cost_no_character_name() {
        // text = "Test" (4 bytes) + no character_name (0) + 50 overhead = 54
        // 54 / 4 = 13
        let entries = vec![make_entry("Test", None, 0.0, 1.0)];
        assert_eq!(estimate_token_cost(&entries), 13);
    }

    #[test]
    fn test_estimate_token_cost_multiple_entries() {
        // Entry 1: "Hello" (5) + "Bob" (3) + 50 = 58
        // Entry 2: "World" (5) + None (0) + 50 = 55
        // Total = 113, 113 / 4 = 28
        let entries = vec![
            make_entry("Hello", Some("Bob"), 0.0, 1.0),
            make_entry("World", None, 1.0, 1.0),
        ];
        assert_eq!(estimate_token_cost(&entries), 28);
    }

    #[test]
    fn test_estimate_token_cost_cjk_text() {
        // CJK characters are 3 bytes each in UTF-8
        // "你好世界" = 12 bytes + "旁白" = 6 bytes + 50 overhead = 68
        // 68 / 4 = 17
        let entries = vec![make_entry("你好世界", Some("旁白"), 0.0, 2.0)];
        assert_eq!(estimate_token_cost(&entries), 17);
    }

    #[test]
    fn test_build_orchestrator_system_prompt_contains_key_elements() {
        let prompt = build_orchestrator_system_prompt();

        // Should define the visual director role
        assert!(prompt.contains("视觉导演"));
        // Should specify JSON output format
        assert!(prompt.contains("JSON"));
        // Should mention visual continuity rules
        assert!(prompt.contains("相邻 chunk 的调色板必须共享至少 2 种颜色"));
        // Should mention shared motifs in 50%+ chunks
        assert!(prompt.contains("50%"));
        // Should reference the Hyperframes style vocabulary
        assert!(prompt.contains("organic"));
        assert!(prompt.contains("geometric"));
        // Should mention rhythm values
        assert!(prompt.contains("slow"));
        assert!(prompt.contains("moderate"));
        assert!(prompt.contains("fast"));
        assert!(prompt.contains("dynamic"));
    }

    #[test]
    fn test_build_orchestrator_user_prompt_full_text_below_threshold() {
        let entries = vec![
            make_entry("First line of text", Some("Alice"), 0.0, 2.0),
            make_entry("Second line of text", Some("Bob"), 2.5, 3.0),
        ];
        let budget = TokenBudget::default_for_model("gpt-4");

        let prompt = build_orchestrator_user_prompt(&entries, &budget);

        // Should include full text since entries are small
        assert!(prompt.contains("First line of text"));
        assert!(prompt.contains("Second line of text"));
        // Should include metadata
        assert!(prompt.contains("总条目数：2"));
        assert!(prompt.contains("总时长："));
        // Should include budget info
        assert!(prompt.contains("32000"));
        assert!(prompt.contains("24000"));
    }

    #[test]
    fn test_build_orchestrator_user_prompt_summarizes_when_over_budget() {
        // Create entries that exceed 50% of orchestrator_input budget (16000 tokens)
        // Need total_chars / 4 > 16000, so total_chars > 64000
        // Each entry: text_len + char_name_len + 50
        // With 200-char text + 5-char name + 50 = 255 chars per entry
        // Need 64000 / 255 ≈ 251 entries
        let entries: Vec<TimelineEntry> = (0..300)
            .map(|i| {
                let mut entry = make_entry(&"A".repeat(200), Some("Alice"), i as f64 * 2.0, 1.5);
                entry.section_title = Some("Chapter 1".to_string());
                entry
            })
            .collect();
        let budget = TokenBudget::default_for_model("gpt-4");

        let prompt = build_orchestrator_user_prompt(&entries, &budget);

        // Should contain summary indicators
        assert!(prompt.contains("内容已摘要化"));
        assert!(prompt.contains("段落概览"));
        // Should NOT contain the full repeated text
        assert!(!prompt.contains(&"A".repeat(200)));
    }

    #[test]
    fn test_build_orchestrator_user_prompt_includes_duration_and_count() {
        let entries = vec![
            make_entry("Line one", None, 0.0, 3.0),
            make_entry("Line two", None, 3.5, 2.0),
        ];
        let budget = TokenBudget::default_for_model("gpt-4");

        let prompt = build_orchestrator_user_prompt(&entries, &budget);

        assert!(prompt.contains("总条目数：2"));
        // Total duration = max(0+3, 3.5+2) = 5.5
        assert!(prompt.contains("5.5"));
    }

    // ─── validate_plan tests ────────────────────────────────────────────────

    #[test]
    fn test_validate_plan_valid_single_chunk() {
        let plan = make_valid_plan(10, 1);
        assert!(validate_plan(&plan, 10).is_ok());
    }

    #[test]
    fn test_validate_plan_valid_multiple_chunks() {
        let plan = make_valid_plan(20, 4);
        assert!(validate_plan(&plan, 20).is_ok());
    }

    #[test]
    fn test_validate_plan_empty_chunks() {
        let plan = OrchestrationPlan {
            global_theme: GlobalTheme {
                mood: vec![],
                shared_motifs: vec![],
                color_progression: ColorProgression {
                    start_palette: vec![],
                    end_palette: vec![],
                },
            },
            chunks: vec![],
        };
        let result = validate_plan(&plan, 10);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("no chunks"));
    }

    #[test]
    fn test_validate_plan_wrong_chunk_index() {
        let mut plan = make_valid_plan(10, 2);
        plan.chunks[1].index = 5; // Should be 1
        let result = validate_plan(&plan, 10);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("index mismatch"));
    }

    #[test]
    fn test_validate_plan_first_chunk_not_starting_at_zero() {
        let mut plan = make_valid_plan(10, 1);
        plan.chunks[0].entry_start = 2;
        let result = validate_plan(&plan, 10);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("entry_start must be 0"));
    }

    #[test]
    fn test_validate_plan_gap_between_chunks() {
        let mut plan = make_valid_plan(10, 2);
        // Create a gap: chunk 0 ends at 4, chunk 1 starts at 6
        plan.chunks[0].entry_end = 4;
        plan.chunks[1].entry_start = 6;
        let result = validate_plan(&plan, 10);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Gap or overlap"));
    }

    #[test]
    fn test_validate_plan_last_chunk_doesnt_cover_all_entries() {
        let mut plan = make_valid_plan(10, 1);
        plan.chunks[0].entry_end = 8; // Should be 10
        let result = validate_plan(&plan, 10);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("entry_end must be 10"));
    }

    #[test]
    fn test_validate_plan_palette_too_small() {
        let mut plan = make_valid_plan(10, 1);
        plan.chunks[0].visual_directive.palette = vec!["#aa".to_string(), "#bb".to_string()]; // Only 2
        let result = validate_plan(&plan, 10);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("3-6 colors"));
    }

    #[test]
    fn test_validate_plan_palette_too_large() {
        let mut plan = make_valid_plan(10, 1);
        plan.chunks[0].visual_directive.palette = vec![
            "#a".to_string(),
            "#b".to_string(),
            "#c".to_string(),
            "#d".to_string(),
            "#e".to_string(),
            "#f".to_string(),
            "#g".to_string(),
        ]; // 7 colors
        let result = validate_plan(&plan, 10);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("3-6 colors"));
    }

    #[test]
    fn test_validate_plan_invalid_rhythm() {
        let mut plan = make_valid_plan(10, 1);
        plan.chunks[0].visual_directive.rhythm = "turbo".to_string();
        let result = validate_plan(&plan, 10);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("invalid rhythm"));
    }

    #[test]
    fn test_validate_plan_valid_rhythms() {
        for rhythm in &["slow", "moderate", "fast", "dynamic"] {
            let mut plan = make_valid_plan(10, 1);
            plan.chunks[0].visual_directive.rhythm = rhythm.to_string();
            assert!(
                validate_plan(&plan, 10).is_ok(),
                "rhythm '{}' should be valid",
                rhythm
            );
        }
    }

    #[test]
    fn test_validate_plan_adjacent_chunks_insufficient_shared_colors() {
        let plan = OrchestrationPlan {
            global_theme: GlobalTheme {
                mood: vec!["epic".to_string()],
                shared_motifs: vec!["stars".to_string()],
                color_progression: ColorProgression {
                    start_palette: vec!["#000000".to_string()],
                    end_palette: vec!["#ffffff".to_string()],
                },
            },
            chunks: vec![
                make_chunk(0, 0, 5, vec!["#aa0000", "#bb0000", "#cc0000"], "slow"),
                make_chunk(1, 5, 10, vec!["#dd0000", "#ee0000", "#ff0000"], "fast"),
            ],
        };
        let result = validate_plan(&plan, 10);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("share only 0 color(s)"));
    }

    #[test]
    fn test_validate_plan_adjacent_chunks_exactly_two_shared_colors() {
        let plan = OrchestrationPlan {
            global_theme: GlobalTheme {
                mood: vec!["epic".to_string()],
                shared_motifs: vec!["stars".to_string()],
                color_progression: ColorProgression {
                    start_palette: vec!["#000000".to_string()],
                    end_palette: vec!["#ffffff".to_string()],
                },
            },
            chunks: vec![
                make_chunk(0, 0, 5, vec!["#shared1", "#shared2", "#unique1"], "slow"),
                make_chunk(1, 5, 10, vec!["#shared1", "#shared2", "#unique2"], "fast"),
            ],
        };
        assert!(validate_plan(&plan, 10).is_ok());
    }

    // ─── run_orchestrator tests ─────────────────────────────────────────────

    #[tokio::test]
    async fn test_run_orchestrator_empty_entries() {
        let config = LlmConfig {
            api_endpoint: "http://localhost:1234",
            api_key: "test-key",
            model: "test-model",
        };
        let budget = TokenBudget::default_for_model("gpt-4");

        let result = run_orchestrator(&[], &config, &budget).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            PipelineError::Other(msg) => assert!(msg.contains("Timeline is empty")),
            other => panic!("Expected PipelineError::Other, got {:?}", other),
        }
    }
}
