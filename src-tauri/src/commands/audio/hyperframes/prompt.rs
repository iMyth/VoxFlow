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

    let mut prompt =
        format!("请根据以下有声书时间轴数据，为每个片段创作对应的视觉画面。\n\n{json}");

    if entries.len() > 50 {
        prompt.push_str(
            "\n\n注意：本项目台词较多（超过 50 行），请使用 sub-composition 模式，按 section 拆分为多个独立的视觉段落。",
        );
    }

    prompt
}

/// Build a user prompt for a single chunk (section) of the timeline.
///
/// Used in chunked generation mode: each section is generated independently,
/// then merged into a final composition. The prompt tells the LLM to generate
/// only the clip elements and styles for this specific time range.
pub fn build_chunk_user_prompt(
    entries: &[TimelineEntry],
    chunk_index: usize,
    total_chunks: usize,
    section_title: &str,
) -> String {
    let chunk_entries: Vec<EntryJson> = entries
        .iter()
        .map(|e| EntryJson {
            text: e.text.clone(),
            start: e.start_time,
            duration: e.duration,
            character: e.character_name.clone().unwrap_or_default(),
        })
        .collect();

    let chunk_start = entries
        .iter()
        .map(|e| e.start_time)
        .fold(f64::INFINITY, f64::min);
    let chunk_end = entries
        .iter()
        .map(|e| e.start_time + e.duration)
        .fold(0.0_f64, f64::max);

    let chunk_data = ChunkJson {
        chunk_index,
        total_chunks,
        section_title: section_title.to_string(),
        time_range: TimeRange {
            start: chunk_start,
            end: chunk_end,
        },
        entries: chunk_entries,
    };

    let json = serde_json::to_string_pretty(&chunk_data).unwrap_or_default();

    format!(
        "这是一个分段生成任务（第 {}/{} 段，段落标题：「{}」）。\n\
         请只为这个时间段生成视觉画面。输出完整 HTML 文件，但只包含这个时间段的 clip 元素。\n\
         composition 的 data-start 应为 \"0\"，data-duration 应为 \"{}\"。\n\
         GSAP timeline 中所有动画的时间偏移使用绝对时间（从 {} 秒开始）。\n\n\
         这个段落时长 {:.0} 秒。尽情发挥——创造一个让人过目不忘的视觉段落。\n\
         不要重复上一段的视觉语言，每段都应该是一次新的视觉冒险。\n\n\
         {json}",
        chunk_index + 1,
        total_chunks,
        section_title,
        chunk_end - chunk_start,
        chunk_start,
        chunk_end - chunk_start,
    )
}

/// Determine whether chunked generation should be used based on entry count.
///
/// Returns the threshold: if entries exceed this count, use chunked mode.
/// Set to 15 to avoid unnecessary orchestration overhead for medium-length scripts
/// (a typical audiobook chapter with 15 entries is ~60-90 seconds, manageable in one shot).
pub const CHUNK_THRESHOLD: usize = 15;

/// Split timeline entries into chunks by section for independent generation.
///
/// Returns a Vec of (section_title, entries_slice_indices) tuples.
pub fn split_into_chunks(entries: &[TimelineEntry]) -> Vec<(String, Vec<usize>)> {
    let mut chunks: Vec<(String, Vec<usize>)> = Vec::new();
    let mut current_title: Option<String> = None;
    let mut current_indices: Vec<usize> = Vec::new();

    for (i, entry) in entries.iter().enumerate() {
        let title = entry
            .section_title
            .clone()
            .unwrap_or_else(|| "默认".to_string());

        if current_title.as_deref() != Some(&title) {
            if !current_indices.is_empty() {
                chunks.push((
                    current_title.unwrap_or_else(|| "默认".to_string()),
                    std::mem::take(&mut current_indices),
                ));
            }
            current_title = Some(title);
        }
        current_indices.push(i);
    }

    if !current_indices.is_empty() {
        chunks.push((
            current_title.unwrap_or_else(|| "默认".to_string()),
            current_indices,
        ));
    }

    chunks
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

#[derive(serde::Serialize)]
struct ChunkJson {
    chunk_index: usize,
    total_chunks: usize,
    section_title: String,
    time_range: TimeRange,
    entries: Vec<EntryJson>,
}

#[derive(serde::Serialize)]
struct TimeRange {
    start: f64,
    end: f64,
}

const ROLE_DEFINITION: &str = "\
你是一位顶尖的动态视觉艺术家，用 HTML/CSS/SVG/GSAP 作为创作媒介。\
你正在为一段有声书创作配套的全屏视觉体验。\
观众会在黑暗中全屏观看这个画面，同时聆听音频。\
你的目标：让观众觉得这不是「自动生成的配图」，而是一件精心制作的视觉作品。";

const CREATIVE_FREEDOM: &str = "\
[用户观感标准 — 这是你唯一需要优化的目标]\n\
\n\
想象观众刷到这个视频时的反应。以下是从「差」到「好」的光谱：\n\
❌ 「这就是个字幕视频加了点粒子」— 纯文字 + 几个小光点漂浮\n\
❌ 「AI 生成的吧，千篇一律」— 深蓝背景 + 发光圆环 + 渐显文字（太常见了）\n\
⚠️ 「还行，有点氛围感」— 有层次但缺乏惊喜\n\
✅ 「哇，这个画面好有感觉」— 视觉与内容产生了情感共鸣，有独特的美学\n\
✅✅ 「这是怎么做到的？」— 观众想截图分享，画面本身就是内容\n\
\n\
[如何达到 ✅✅ 级别]\n\
\n\
1. 视觉要有「主角」\n\
   - 每个场景需要一个占画面 30%+ 的核心视觉元素（不是一堆小点）\n\
   - 这个主角要有动态：生长、变形、呼吸、旋转、流动\n\
   - 它应该与文案内容产生意象关联（不需要字面对应）\n\
\n\
2. 画面要有「呼吸」\n\
   - 不是静态海报，每一秒都应该有微妙的运动\n\
   - 背景层：缓慢的色彩漂移、噪点纹理流动、光斑脉动\n\
   - 前景层：元素的入场/退场要有戏剧性（不是简单 fade）\n\
\n\
3. 色彩要有「态度」\n\
   - 不要默认深蓝+白色。根据内容情绪选择大胆的色彩方案\n\
   - 暖色系（琥珀、珊瑚、金）、冷色系（青、靛、薄荷）、对比系（暗底+亮色冲击）都可以\n\
   - 色彩应该随叙事推进而变化\n\
\n\
4. 技术要「到位」\n\
   - 大量元素（20-50个）通过 stagger 产生群体运动感\n\
   - SVG 滤镜（feTurbulence, feDisplacementMap）创造有机质感\n\
   - mix-blend-mode 让层次之间产生化学反应\n\
   - clip-path 做非矩形的揭示和遮罩\n\
   - perspective + transform3d 创造空间纵深\n\
   - 多层 box-shadow 做光晕和景深效果\n\
\n\
5. 场景切换要有「节奏」\n\
   - 每个叙事段落应该有不同的视觉主题，不要全程一个风格\n\
   - 利用 clip 的自动显示/隐藏实现干净的场景切换\n\
   - 场景之间的视觉语言可以有延续性（色彩渐变过渡）也可以有对比（突然切换）\n\
\n\
[你的工具箱]\n\
CSS: gradient 多层叠加 | backdrop-filter | mix-blend-mode | clip-path | \n\
     box-shadow 多层 | border-radius 有机形状 | transform 3D | filter: hue-rotate/blur\n\
SVG: path 描边动画 | filter(feTurbulence/feDisplacementMap/feGaussianBlur) | \n\
     pattern | clipPath/mask | polyline/polygon\n\
GSAP: stagger | motionPath | ease(elastic/back/expo/power4) | \n\
      yoyo+repeat(呼吸效果) | keyframes 数组 | fromTo 精确控制\n\
\n\
[绝对自由]\n\
- 你可以用任何视觉风格：有机/几何/极简/繁复/抽象/半具象\n\
- 你可以用任何色彩：暗色/亮色/单色/撞色/渐变\n\
- 文字可以出现也可以不出现，如果出现要融入画面设计\n\
- 唯一的约束是下面的技术格式规范";

const HYPERFRAMES_SPEC: &str = "\
[Hyperframes 渲染引擎工作原理 — 你必须理解这个]

渲染器的工作方式（非常重要）：
1. 渲染器逐帧截图，每帧对应一个时间点 t
2. 对于每个 clip 元素，渲染器检查 t 是否在 [data-start, data-start + data-duration] 范围内
   - 在范围内 → clip 可见（mounted）
   - 不在范围内 → clip 被隐藏（unmounted）
3. 渲染器调用 tl.seek(t) 将 GSAP timeline 跳转到时间 t
4. 截图当前画面作为该帧

这意味着：
- clip 是天然的「场景开关」！不需要用 opacity 动画来手动显示/隐藏元素
- 一个 clip 在 data-start 之前和 data-start+data-duration 之后完全不存在
- 你应该为每个叙事段落创建独立的 clip，而不是把所有东西塞进一个全时长 clip
- GSAP 动画的时间偏移必须是绝对时间（与 data-start 对齐），因为 tl.seek() 用的是绝对时间

正确用法示例：
  第一段（0-30秒）的星空 → <div id=\"scene-1\" class=\"clip\" data-start=\"0\" data-duration=\"30\">
  第二段（30-60秒）的网络 → <div id=\"scene-2\" class=\"clip\" data-start=\"30\" data-duration=\"30\">
  渲染器到 t=35 时，第一段自动消失，第二段自动出现，无需任何 opacity 动画

错误用法：
  ❌ 一个 clip data-start=\"0\" data-duration=\"765\" 包含所有元素，靠 GSAP opacity 切换
  ✅ 多个 clip 各自有精确的时间窗口，元素在窗口外自动隐藏

[技术格式]

1. 根元素属性（必须）：
   id=\"root\"
   data-composition-id=\"ai-generated\"
   data-width=\"1920\"
   data-height=\"1080\"
   data-start=\"0\"

2. Clip 元素（每个可见元素必须有）：
   <div id=\"<唯一ID>\" class=\"clip\" data-start=\"<秒>\" data-duration=\"<秒>\" data-track-index=\"<层级>\">
   - id: 每个 clip 必须有唯一 id（用于 GSAP 定位，如 id=\"bg-1\"、id=\"text-intro\"）
   - class=\"clip\": 必须包含，框架用它管理可见性
   - data-start: 该 clip 开始显示的绝对时间（秒）
   - data-duration: 该 clip 显示持续时间（秒）
   - data-track-index: 层级索引（从 0 开始，数字越大越靠前/上层）
   - 同一 track-index 的多个 clip 不能在时间上重叠

3. Clip 使用策略：
   - 贯穿全程的背景层：一个 clip，data-start=0，data-duration=总时长
   - 分段出现的场景元素：每段一个 clip，精确的 data-start 和 data-duration
   - 文字/标题：每条文字一个独立 clip，显示 8-15 秒后自动消失
   - 利用 clip 的自动隐藏特性，不需要手动 fade-out 到 opacity:0

4. GSAP 动画（关键规则）：
   - 引入: <script src=\"https://cdn.jsdelivr.net/npm/gsap@3/dist/gsap.min.js\"></script>
   - 创建: const tl = gsap.timeline({ paused: true });
   - 注册: window.__timelines = window.__timelines || {}; window.__timelines[\"ai-generated\"] = tl;
   - 时间定位：tl.to(\"#element-id\", {...}, 绝对时间) — 用 id 选择器定位元素
   - 动画应在 clip 的时间窗口内完成（clip 隐藏后动画状态无意义）
   - 只使用 tl.to()、tl.from()、tl.fromTo()、tl.set() 四种方法
   - position 参数（第三个参数）必须是绝对时间数字

5. GSAP 选择器规则（非常重要）：
   - 优先使用 id 选择器：tl.to(\"#my-element\", ...)
   - 如果用 class 选择器，确保对应的 HTML 元素确实存在
   - 禁止使用 document.querySelectorAll() 或 gsap.utils.toArray()
   - 禁止使用 forEach 循环动态创建动画
   - 禁止在 GSAP 回调中使用 this.index 或 this.target
   - 如果需要 stagger 效果，直接用 stagger 属性：tl.to(\".particle\", {stagger: 0.1, ...}, 0)

6. 禁止使用：
   - Math.random()（破坏确定性渲染）
   - Date.now()（破坏确定性渲染）
   - repeat: -1（无限循环会阻塞渲染）
   - async/await 操作 timeline
   - requestAnimationFrame 自行驱动动画
   - document.querySelectorAll() 或 document.getElementById()
   - gsap.utils.toArray() 配合 forEach
   - 函数作为 GSAP 属性值（如 x: function() {...}）
   - animation-iteration-count: infinite

7. CSS/SVG 动画规则：
   - CSS @keyframes 动画可以使用，但时长必须有限
   - 所有视觉元素必须在 composition 根元素内部
   - SVG filter 的 id 必须唯一（不同场景不要复用同一个 filter id）
   - 动画 SVG 元素时，使用 GSAP 的 x/y/rotation/scale 属性，不要用 transform 字符串
   - 不要使用 @import url() 引入外部字体（渲染器可能无网络访问）
   - 用 CSS font-family 指定字体时，始终包含 fallback（如 serif、sans-serif）";

const MINIMAL_EXAMPLE: &str = "\
[格式骨架 — 仅展示 Hyperframes 结构，不要模仿视觉风格]
<!DOCTYPE html>
<html>
<head>
  <meta charset=\"UTF-8\">
  <style>
    * { margin: 0; padding: 0; box-sizing: border-box; }
    [data-composition-id] { overflow: hidden; position: relative; width: 1920px; height: 1080px; }
    .clip { position: absolute; width: 100%; height: 100%; top: 0; left: 0; }
    /* 在这里定义你的视觉元素样式 */
  </style>
</head>
<body>
  <div id=\"root\" data-composition-id=\"ai-generated\" data-width=\"1920\" data-height=\"1080\" data-start=\"0\">
    <!-- Track 0: 贯穿全程的背景层 -->
    <div id=\"bg-main\" class=\"clip\" data-start=\"0\" data-duration=\"总时长\" data-track-index=\"0\">
      <!-- 背景视觉元素 -->
    </div>
    <!-- Track 1: 第一个场景 (0-N秒，N秒后自动消失) -->
    <div id=\"scene-1\" class=\"clip\" data-start=\"0\" data-duration=\"N\" data-track-index=\"1\">
      <!-- 场景A的视觉元素 -->
    </div>
    <!-- Track 1: 第二个场景 (N-M秒，N秒时自动出现) -->
    <div id=\"scene-2\" class=\"clip\" data-start=\"N\" data-duration=\"M-N\" data-track-index=\"1\">
      <!-- 场景B的视觉元素 -->
    </div>
    <!-- Track 2: 文字层 (精确时间窗口，结束后自动消失) -->
    <div id=\"text-1\" class=\"clip\" data-start=\"5\" data-duration=\"10\" data-track-index=\"2\">
      <p class=\"title-text\">关键文字</p>
    </div>
    <script src=\"https://cdn.jsdelivr.net/npm/gsap@3/dist/gsap.min.js\"></script>
    <script>
      window.__timelines = window.__timelines || {};
      const tl = gsap.timeline({ paused: true });

      // 用 id 选择器定位元素，position 参数是绝对时间
      tl.fromTo('#scene-1 .title-text', {opacity:0, y:30}, {opacity:1, y:0, duration:1.5, ease:'power2.out'}, 0);
      tl.to('#scene-1 .title-text', {opacity:0, y:-20, duration:0.8, ease:'power2.in'}, 4);

      // stagger 示例：大量元素错开动画（不要用 forEach）
      tl.fromTo('#scene-1 .particle', {scale:0}, {scale:1, duration:1, stagger:0.1, ease:'back.out(1.7)'}, 1);

      // yoyo 示例：呼吸/脉动效果（repeat 必须是有限数字）
      tl.to('#bg-main .glow', {opacity:0.8, scale:1.1, duration:4, yoyo:true, repeat:3, ease:'power1.inOut'}, 0);

      window.__timelines[\"ai-generated\"] = tl;
    </script>
  </div>
</body>
</html>

关键点：
- 每个 clip 必须有唯一 id（用于 GSAP 选择器定位）
- GSAP 用 '#id .class' 组合选择器精确定位元素
- clip 的 data-start/data-duration 控制元素的生命周期
- tl.to/tl.from/tl.fromTo 的最后一个数字参数是绝对时间（秒）
- 用 data-track-index 叠加层次（0=最底层背景，数字越大越靠前）
- 用 stagger 属性让大量相似元素产生群体动画效果
- yoyo+repeat 做持续的呼吸效果，但 repeat 必须是有限数字
- 不要用 document.querySelectorAll 或 gsap.utils.toArray";

const OUTPUT_REQUIREMENTS: &str = "\
[输出要求]
- 直接输出完整 HTML 文件内容，不要用 ```html 代码块包裹
- HTML 必须是完整的（从 <!DOCTYPE html> 开始）
- 所有样式内联在 <style> 标签中
- GSAP 使用 CDN 引入，不要使用其他外部依赖
- composition-id 使用 \"ai-generated\"
- <script> 标签放在 composition 根元素内部（</div> 之前）

[GSAP 代码质量要求 — 严格遵守]
- opacity 值范围 0-1，不能超过 1
- repeat 必须是正整数（如 1, 2, 3），不能是小数
- duration 保留最多 1 位小数（如 2.5，不要 2.5384615）
- data-track-index 必须是非负整数（0, 1, 2, 3...）
- 每个 tl.to/tl.from/tl.fromTo 调用必须语法完整（括号匹配）
- 不要在 GSAP 属性中使用超过 2 位小数的数字
- position 参数（绝对时间）保留最多 2 位小数
- 确保所有 JavaScript 语法正确（引号闭合、括号匹配）";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_system_prompt_contains_role() {
        let prompt = build_system_prompt();
        assert!(prompt.contains("视觉艺术家"));
        assert!(prompt.contains("Hyperframes"));
    }

    #[test]
    fn test_build_system_prompt_contains_creative_freedom() {
        let prompt = build_system_prompt();
        assert!(prompt.contains("用户观感标准"));
        assert!(prompt.contains("工具箱"));
        assert!(prompt.contains("绝对自由"));
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
        assert!(prompt.contains("cdn.jsdelivr.net/npm/gsap@3"));
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
        assert!(prompt.contains("data-composition-id=\"ai-generated\""));
        assert!(prompt.contains("window.__timelines[\"ai-generated\"] = tl"));
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
        let spec_section = HYPERFRAMES_SPEC;
        let spec_chars = spec_section.len();
        assert!(
            spec_chars < 6000,
            "Spec section too long: {} chars",
            spec_chars
        );
        // Full prompt includes creative guidance, spec, example, and output requirements
        assert!(
            prompt.len() < 28000,
            "Full prompt too long: {} chars",
            prompt.len()
        );
    }

    // --- build_user_prompt tests ---

    fn make_entry(
        text: &str,
        start: f64,
        duration: f64,
        section: Option<&str>,
        character: Option<&str>,
    ) -> TimelineEntry {
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
        let entries = vec![make_entry("Hello", 0.0, 2.0, None, None)];

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
            make_entry("Second", 2.5, 3.0, None, None), // ends at 5.5
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
        let entries = vec![make_entry("No section line", 0.0, 2.0, None, Some("旁白"))];

        let prompt = build_user_prompt(&entries);
        assert!(prompt.contains("默认"));
    }

    // --- split_into_chunks tests ---

    #[test]
    fn test_split_into_chunks_by_section() {
        let entries = vec![
            make_entry("Line 1", 0.0, 2.0, Some("第一章"), None),
            make_entry("Line 2", 2.5, 3.0, Some("第一章"), None),
            make_entry("Line 3", 6.0, 1.5, Some("第二章"), None),
            make_entry("Line 4", 8.0, 2.0, Some("第二章"), None),
            make_entry("Line 5", 10.5, 1.0, Some("第三章"), None),
        ];

        let chunks = split_into_chunks(&entries);
        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0].0, "第一章");
        assert_eq!(chunks[0].1, vec![0, 1]);
        assert_eq!(chunks[1].0, "第二章");
        assert_eq!(chunks[1].1, vec![2, 3]);
        assert_eq!(chunks[2].0, "第三章");
        assert_eq!(chunks[2].1, vec![4]);
    }

    #[test]
    fn test_split_into_chunks_no_sections() {
        let entries = vec![
            make_entry("Line 1", 0.0, 2.0, None, None),
            make_entry("Line 2", 2.5, 3.0, None, None),
        ];

        let chunks = split_into_chunks(&entries);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].0, "默认");
        assert_eq!(chunks[0].1, vec![0, 1]);
    }

    #[test]
    fn test_split_into_chunks_empty() {
        let entries: Vec<TimelineEntry> = vec![];
        let chunks = split_into_chunks(&entries);
        assert!(chunks.is_empty());
    }

    // --- build_chunk_user_prompt tests ---

    #[test]
    fn test_build_chunk_user_prompt_contains_chunk_info() {
        let entries = vec![
            make_entry("量子纠缠", 5.0, 3.0, Some("第二章"), Some("旁白")),
            make_entry("概率云", 8.5, 2.0, Some("第二章"), Some("旁白")),
        ];

        let prompt = build_chunk_user_prompt(&entries, 1, 3, "第二章");
        assert!(prompt.contains("第 2/3 段"));
        assert!(prompt.contains("第二章"));
        assert!(prompt.contains("量子纠缠"));
        assert!(prompt.contains("概率云"));
        assert!(prompt.contains("分段生成"));
    }
}
