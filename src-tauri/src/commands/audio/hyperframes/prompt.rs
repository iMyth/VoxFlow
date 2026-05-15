//! Prompt construction for AI-powered Hyperframes composition generation.
//!
//! Builds the system prompt that instructs the LLM to act as a video visual
//! creation expert, with creative freedom to generate animations/scenes
//! (not just subtitles) while respecting Hyperframes technical constraints.

use super::timeline::TimelineEntry;

/// Build the system prompt for the LLM.
///
/// The prompt defines:
/// 1. AI's role as a video visual creation expert using Hyperframes
/// 2. Creative freedom: generate visual scenes related to script content
/// 3. Condensed Hyperframes specification (technical constraints only)
/// 4. A minimal working example for reference
/// 5. Output format requirements
pub fn build_system_prompt() -> String {
    format!(
        "{role}\n\n{creative}\n\n{spec}\n\n{example}\n\n{output}",
        role = ROLE_DEFINITION,
        creative = CREATIVE_FREEDOM,
        spec = HYPERFRAMES_SPEC,
        example = MINIMAL_EXAMPLE,
        output = OUTPUT_REQUIREMENTS,
    )
}

/// Build the user prompt containing timeline data as structured JSON.
///
/// The prompt includes:
/// 1. Timeline data grouped by section, formatted as JSON
/// 2. A Chinese instruction telling the AI to create visuals for this content
/// 3. For projects with >50 lines, an instruction about sub-composition mode
pub fn build_user_prompt(entries: &[TimelineEntry]) -> String {
    let total_duration: f64 = entries
        .iter()
        .map(|e| e.start_time + e.duration)
        .fold(0.0_f64, f64::max);

    // Group entries by section_title
    let mut sections: Vec<Section> = Vec::new();
    let mut current_title: Option<String> = None;
    let mut current_entries: Vec<EntryJson> = Vec::new();

    for entry in entries {
        let title = entry
            .section_title
            .clone()
            .unwrap_or_else(|| "默认".to_string());

        if current_title.as_deref() != Some(&title) {
            if !current_entries.is_empty() {
                sections.push(Section {
                    title: current_title.unwrap_or_else(|| "默认".to_string()),
                    entries: std::mem::take(&mut current_entries),
                });
            }
            current_title = Some(title.clone());
        }

        current_entries.push(EntryJson {
            text: entry.text.clone(),
            start: entry.start_time,
            duration: entry.duration,
            character: entry.character_name.clone().unwrap_or_default(),
        });
    }

    // Push the last section
    if !current_entries.is_empty() {
        sections.push(Section {
            title: current_title.unwrap_or_else(|| "默认".to_string()),
            entries: current_entries,
        });
    }

    let timeline_data = TimelineJson {
        total_duration,
        sections,
    };

    let json = serde_json::to_string_pretty(&timeline_data).unwrap_or_default();

    let mut prompt = format!(
        "请根据以下有声书时间轴数据，为每个片段创作对应的视觉画面。\n\n{json}"
    );

    if entries.len() > 50 {
        prompt.push_str(
            "\n\n注意：本项目台词较多（超过 50 行），请使用 sub-composition 模式，按 section 拆分为多个独立的视觉段落。",
        );
    }

    prompt
}

#[derive(serde::Serialize)]
struct TimelineJson {
    total_duration: f64,
    sections: Vec<Section>,
}

#[derive(serde::Serialize)]
struct Section {
    title: String,
    entries: Vec<EntryJson>,
}

#[derive(serde::Serialize)]
struct EntryJson {
    text: String,
    start: f64,
    duration: f64,
    character: String,
}

const ROLE_DEFINITION: &str = "\
你是一个顶级视频视觉创作专家和动效设计师，使用 Hyperframes 框架为有声书创作震撼的视觉画面。\
你的作品以视觉冲击力强、元素丰富、层次感强著称。";

const CREATIVE_FREEDOM: &str = "\
你的任务：根据有声书文案内容，创作视觉冲击力强的画面。\n\
\n\
[核心原则 — 画面必须丰富]\n\
- 每个片段至少使用 3-5 个视觉层次（背景层 + 装饰层 + 主体层 + 前景粒子层 + 文字层）\n\
- 使用多个 track（data-track-index）叠加不同层次的视觉元素\n\
- 背景永远不要只是纯色！使用渐变、网格、噪点纹理、径向光晕等\n\
- 大量使用 CSS 动画：浮动粒子、脉冲光圈、扫描线、呼吸效果\n\
- 文字要大、要有存在感，配合发光、阴影、描边等效果\n\
- 颜色要饱和、对比要强烈，善用霓虹色、渐变色\n\
\n\
[技术手段 — 充分利用]\n\
- CSS: radial-gradient, conic-gradient, backdrop-filter, mix-blend-mode, clip-path\n\
- SVG: 路径动画、滤镜（feGaussianBlur, feTurbulence）、图案填充\n\
- GSAP: stagger 动画、运动路径、弹性缓动、序列编排\n\
- 伪元素 ::before/::after 增加装饰层\n\
- box-shadow 多层发光、text-shadow 霓虹效果\n\
- CSS Grid/Flexbox 创建复杂布局\n\
\n\
[视觉风格参考]\n\
- 文案讲\u{201c}暴风雨来临\u{201d} → 全屏雨滴粒子（50+个 div）+ 闪电 SVG 路径动画 + 乌云渐变背景 + 风吹树影剪影 + 大字标题带抖动\n\
- 文案讲\u{201c}两人对话\u{201d} → 分屏布局 + 角色轮廓 SVG + 对话气泡弹入 + 背景波纹扩散 + 情绪色彩渐变\n\
- 文案讲\u{201c}宁静夜晚\u{201d} → 星空粒子背景（30+颗星）+ 月亮光晕 + 萤火虫浮动 + 文字淡入带柔光\n\
- 文案讲\u{201c}激烈战斗\u{201d} → 红色脉冲波 + 碎片爆炸 + 画面震动 + 速度线 + 冲击波扩散 + 大字体碎裂\n\
\n\
[禁止]\n\
- 禁止只放一个小元素在画面中央，画面必须饱满\n\
- 禁止大面积空白/纯黑，每个区域都要有视觉内容\n\
- 文字可以出现也可以不出现，但如果出现必须有设计感（不是简单居中白字）";

const HYPERFRAMES_SPEC: &str = "\
[Hyperframes 技术约束]

1. 根元素属性（必须）：
   data-composition-id=\"<id>\"
   data-width=\"1920\"
   data-height=\"1080\"
   data-start=\"0\"
   data-duration=\"<总时长秒数>\"

2. 定时元素（Clip）：
   每个视觉片段用 <div class=\"clip\" data-start=\"<秒>\" data-duration=\"<秒>\" data-track-index=\"<层级>\">
   - data-start: 该片段开始时间（秒）
   - data-duration: 该片段持续时间（秒）
   - data-track-index: 层级索引（从 1 开始，数字越大越靠前）

3. GSAP 动画：
   - 引入: <script src=\"https://cdn.jsdelivr.net/npm/gsap@3.12.5/dist/gsap.min.js\"></script>
   - 创建: const tl = gsap.timeline({ paused: true });
   - 注册: window.__timelines = window.__timelines || {}; window.__timelines[\"<composition-id>\"] = tl;
   - 所有动画必须添加到这个 timeline 上，使用绝对时间定位

4. 禁止使用：
   - Math.random()（破坏确定性渲染）
   - Date.now()（破坏确定性渲染）
   - repeat: -1（无限循环会阻塞渲染）
   - async/await 操作 timeline
   - requestAnimationFrame 自行驱动动画

5. CSS/SVG 动画规则：
   - CSS @keyframes 动画可以使用，但时长必须有限
   - animation-iteration-count 不能为 infinite
   - 所有视觉元素必须在 composition 根元素内部";

const MINIMAL_EXAMPLE: &str = "\
[丰富示例 — 注意多层叠加]
<!DOCTYPE html>
<html>
<head>
  <meta charset=\"UTF-8\">
  <style>
    [data-composition-id] { background: linear-gradient(135deg, #0a0a2e 0%, #1a0a3e 50%, #0a1a2e 100%); overflow: hidden; position: relative; font-family: sans-serif; }
    .bg-layer { position: absolute; width: 100%; height: 100%; }
    .particle { position: absolute; width: 4px; height: 4px; background: rgba(99,102,241,0.6); border-radius: 50%; box-shadow: 0 0 6px rgba(99,102,241,0.8); }
    .glow-ring { position: absolute; top: 50%; left: 50%; width: 300px; height: 300px; margin: -150px; border: 2px solid rgba(139,92,246,0.3); border-radius: 50%; box-shadow: 0 0 40px rgba(139,92,246,0.2), inset 0 0 40px rgba(139,92,246,0.1); }
    .title { position: absolute; top: 50%; left: 50%; transform: translate(-50%,-50%); color: #f0f0f5; font-size: 72px; font-weight: 700; text-shadow: 0 0 20px rgba(99,102,241,0.8), 0 0 60px rgba(99,102,241,0.4); }
    .scan-line { position: absolute; width: 100%; height: 2px; background: linear-gradient(90deg, transparent, rgba(99,102,241,0.4), transparent); }
  </style>
</head>
<body>
  <div data-composition-id=\"demo\" data-width=\"1920\" data-height=\"1080\" data-start=\"0\" data-duration=\"5\">
    <div class=\"clip bg-layer\" data-start=\"0\" data-duration=\"5\" data-track-index=\"1\">
      <div class=\"particle\" style=\"top:10%;left:20%\"></div>
      <div class=\"particle\" style=\"top:30%;left:70%\"></div>
      <div class=\"particle\" style=\"top:60%;left:40%\"></div>
      <div class=\"particle\" style=\"top:80%;left:85%\"></div>
      <div class=\"particle\" style=\"top:45%;left:15%\"></div>
      <div class=\"glow-ring\"></div>
      <div class=\"scan-line\" style=\"top:30%\"></div>
    </div>
    <div class=\"clip\" data-start=\"0\" data-duration=\"5\" data-track-index=\"2\">
      <h1 class=\"title\">暴风雨来临</h1>
    </div>
    <script src=\"https://cdn.jsdelivr.net/npm/gsap@3.12.5/dist/gsap.min.js\"></script>
    <script>
      window.__timelines = window.__timelines || {};
      const tl = gsap.timeline({ paused: true });
      tl.from(\".title\", { scale: 0.8, opacity: 0, duration: 1.2, ease: \"power3.out\" }, 0.3);
      tl.from(\".particle\", { opacity: 0, y: 20, duration: 0.8, stagger: 0.15, ease: \"power2.out\" }, 0);
      tl.from(\".glow-ring\", { scale: 0.5, opacity: 0, duration: 1.5, ease: \"power2.out\" }, 0.2);
      tl.to(\".scan-line\", { y: 800, duration: 3, ease: \"none\" }, 0.5);
      tl.to(\".glow-ring\", { rotation: 360, duration: 4, ease: \"none\" }, 0);
      window.__timelines[\"demo\"] = tl;
    </script>
  </div>
</body>
</html>";

const OUTPUT_REQUIREMENTS: &str = "\
[输出要求]
- 直接输出完整 HTML 文件内容，不要用 ```html 代码块包裹
- HTML 必须是完整的（从 <!DOCTYPE html> 开始）
- 所有样式内联在 <style> 标签中
- GSAP 使用 CDN 引入，不要使用其他外部依赖
- composition-id 使用 \"ai-generated\"";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_system_prompt_contains_role() {
        let prompt = build_system_prompt();
        assert!(prompt.contains("视频视觉创作专家"));
        assert!(prompt.contains("Hyperframes"));
    }

    #[test]
    fn test_build_system_prompt_contains_creative_freedom() {
        let prompt = build_system_prompt();
        assert!(prompt.contains("视觉冲击力"));
        assert!(prompt.contains("多个 track"));
        assert!(prompt.contains("禁止只放一个小元素"));
    }

    #[test]
    fn test_build_system_prompt_contains_spec_constraints() {
        let prompt = build_system_prompt();
        assert!(prompt.contains("data-composition-id"));
        assert!(prompt.contains("data-width=\"1920\""));
        assert!(prompt.contains("data-height=\"1080\""));
        assert!(prompt.contains("class=\"clip\""));
        assert!(prompt.contains("data-start"));
        assert!(prompt.contains("data-duration"));
        assert!(prompt.contains("data-track-index"));
    }

    #[test]
    fn test_build_system_prompt_contains_gsap_rules() {
        let prompt = build_system_prompt();
        assert!(prompt.contains("gsap.timeline({ paused: true })"));
        assert!(prompt.contains("window.__timelines"));
        assert!(prompt.contains("cdn.jsdelivr.net/npm/gsap@3.12.5"));
    }

    #[test]
    fn test_build_system_prompt_contains_forbidden_patterns() {
        let prompt = build_system_prompt();
        assert!(prompt.contains("Math.random()"));
        assert!(prompt.contains("Date.now()"));
        assert!(prompt.contains("repeat: -1"));
    }

    #[test]
    fn test_build_system_prompt_contains_minimal_example() {
        let prompt = build_system_prompt();
        assert!(prompt.contains("<!DOCTYPE html>"));
        assert!(prompt.contains("data-composition-id=\"demo\""));
        assert!(prompt.contains("window.__timelines[\"demo\"] = tl"));
        assert!(prompt.contains("stagger"));
    }

    #[test]
    fn test_build_system_prompt_contains_output_requirements() {
        let prompt = build_system_prompt();
        assert!(prompt.contains("直接输出完整 HTML"));
        assert!(prompt.contains("不要用"));
        assert!(prompt.contains("ai-generated"));
    }

    #[test]
    fn test_build_system_prompt_reasonable_length() {
        let prompt = build_system_prompt();
        // Rough token estimate: ~4 chars per token for mixed CJK/English
        // Should be well under 2000 tokens for the spec portion
        // Total prompt (including examples) can be longer, but spec section alone should be concise
        let spec_section = HYPERFRAMES_SPEC;
        let spec_chars = spec_section.len();
        // CJK chars ≈ 1-2 tokens each, ASCII ≈ 4 chars/token
        // Conservative estimate: spec_chars / 2 < 2000 tokens
        assert!(
            spec_chars < 4000,
            "Spec section too long: {} chars",
            spec_chars
        );
        // Full prompt should be reasonable (not excessively long)
        assert!(prompt.len() < 12000, "Full prompt too long: {} chars", prompt.len());
    }

    // --- build_user_prompt tests ---

    fn make_entry(text: &str, start: f64, duration: f64, section: Option<&str>, character: Option<&str>) -> TimelineEntry {
        TimelineEntry {
            line_id: format!("line_{}", start as u32),
            text: text.to_string(),
            character_name: character.map(|s| s.to_string()),
            section_title: section.map(|s| s.to_string()),
            start_time: start,
            duration,
        }
    }

    #[test]
    fn test_build_user_prompt_basic_structure() {
        let entries = vec![
            make_entry("在一个风雨交加的夜晚", 0.0, 3.2, Some("开篇"), Some("旁白")),
            make_entry("雷声轰鸣", 3.2, 2.0, Some("开篇"), Some("旁白")),
        ];

        let prompt = build_user_prompt(&entries);
        assert!(prompt.contains("total_duration"));
        assert!(prompt.contains("sections"));
        assert!(prompt.contains("开篇"));
        assert!(prompt.contains("在一个风雨交加的夜晚"));
        assert!(prompt.contains("旁白"));
    }

    #[test]
    fn test_build_user_prompt_contains_instruction() {
        let entries = vec![
            make_entry("Hello", 0.0, 2.0, None, None),
        ];

        let prompt = build_user_prompt(&entries);
        assert!(prompt.contains("请根据以下有声书时间轴数据"));
    }

    #[test]
    fn test_build_user_prompt_groups_by_section() {
        let entries = vec![
            make_entry("Line 1", 0.0, 2.0, Some("第一章"), Some("Alice")),
            make_entry("Line 2", 2.5, 3.0, Some("第一章"), Some("Bob")),
            make_entry("Line 3", 6.0, 1.5, Some("第二章"), Some("Alice")),
        ];

        let prompt = build_user_prompt(&entries);
        assert!(prompt.contains("第一章"));
        assert!(prompt.contains("第二章"));
    }

    #[test]
    fn test_build_user_prompt_calculates_total_duration() {
        let entries = vec![
            make_entry("First", 0.0, 2.0, None, None),
            make_entry("Second", 2.5, 3.0, None, None),  // ends at 5.5
        ];

        let prompt = build_user_prompt(&entries);
        assert!(prompt.contains("5.5"));
    }

    #[test]
    fn test_build_user_prompt_no_sub_composition_for_small_projects() {
        let entries: Vec<TimelineEntry> = (0..50)
            .map(|i| make_entry(&format!("Line {}", i), i as f64 * 2.0, 1.5, None, None))
            .collect();

        let prompt = build_user_prompt(&entries);
        assert!(!prompt.contains("sub-composition"));
    }

    #[test]
    fn test_build_user_prompt_sub_composition_for_large_projects() {
        let entries: Vec<TimelineEntry> = (0..51)
            .map(|i| make_entry(&format!("Line {}", i), i as f64 * 2.0, 1.5, None, None))
            .collect();

        let prompt = build_user_prompt(&entries);
        assert!(prompt.contains("sub-composition"));
        assert!(prompt.contains("超过 50 行"));
    }

    #[test]
    fn test_build_user_prompt_empty_entries() {
        let entries: Vec<TimelineEntry> = vec![];
        let prompt = build_user_prompt(&entries);
        assert!(prompt.contains("total_duration"));
        assert!(prompt.contains("0.0"));
    }

    #[test]
    fn test_build_user_prompt_default_section_for_entries_without_section() {
        let entries = vec![
            make_entry("No section line", 0.0, 2.0, None, Some("旁白")),
        ];

        let prompt = build_user_prompt(&entries);
        assert!(prompt.contains("默认"));
    }
}
