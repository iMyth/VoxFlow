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
         ⚠️ 视觉密度要求：\n\
         - 这个段落时长 {:.0} 秒，你有充足的空间来创作丰富的画面\n\
         - 每 10-15 秒应该有一次明显的视觉变化（新元素入场、颜色渐变、形态变化）\n\
         - 至少使用 15-30 个视觉元素（粒子、线条、几何体、光效等）\n\
         - 背景不能只是纯色渐变，要有动态纹理（网格线、噪点、光斑漂移）\n\
         - 文字/关键词至少出现 2-3 次，每次展示不同的金句\n\n\
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
pub const CHUNK_THRESHOLD: usize = 8;

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
你是一个顶级视频视觉创作专家和动效设计师，使用 Hyperframes 框架为有声书创作沉浸式视觉画面。\
你擅长将抽象概念（哲学、科学、情感）转化为具象的视觉隐喻，\
作品以氛围感强、层次丰富、情绪递进著称，风格介于科学纪录片与艺术装置之间。";

const CREATIVE_FREEDOM: &str = "\
你的任务：根据有声书文案内容，创作与叙事情绪和概念深度匹配的视觉画面。\n\
\n\
[核心原则 — 概念可视化]\n\
- 有声书通常是深度叙事（哲学、科学、情感），画面要服务于「理解」和「感受」\n\
- 将抽象概念转化为视觉隐喻：量子纠缠→光线交织网络，概率云→弥散粒子雾，时间→琥珀/胶片\n\
- 每个片段使用 3-5 个视觉层次（氛围背景层 + 隐喻主体层 + 粒子/纹理层 + 可选文字层）\n\
- 使用多个 track（data-track-index）叠加不同层次的视觉元素\n\
- 情绪递进：随叙事推进，画面的色调、密度、运动节奏应逐步变化\n\
- 留白是设计语言的一部分——深沉的叙事需要呼吸空间，但留白区域要有微妙纹理或光效\n\
\n\
[视觉密度要求 — 极其重要]\n\
- 每 10-15 秒必须有一次明显的视觉变化（新元素入场/退场、颜色渐变、形态变化）\n\
- 背景层：不能只是纯色或简单渐变！必须有动态元素（缓慢漂移的粒子群、脉动的光晕、扫描线）\n\
- 粒子/装饰层：每个场景至少 15-30 个小元素（光点、线段、几何碎片），用 stagger 动画错开\n\
- 主体层：核心隐喻图形要有足够的尺寸和存在感（占画面 30-50%），不要只是一个小点\n\
- 文字层：关键金句要大（40-72px），有设计感（发光、渐显、字间距动画）\n\
- 动画要持续：不要只在入场时有动画，元素在场期间也要有微妙的持续运动（呼吸、漂移、旋转）\n\
\n\
[视觉隐喻设计思路]\n\
- 讲「渺小/宏大」→ 微小光点在巨大深空中缓慢漂移，径向光晕暗示无限\n\
- 讲「连接/关系/纠缠」→ 光线网络、节点脉冲、线条交汇处发光\n\
- 讲「不确定性/概率」→ 弥散粒子云、半透明叠影、模糊与清晰的交替\n\
- 讲「时间/永恒」→ 琥珀色调、胶片帧叠加、缓慢旋转的几何体\n\
- 讲「意识/觉醒」→ 从暗到亮的渐变、瞳孔/眼睛意象、光束聚焦\n\
- 讲「循环/自指」→ 衔尾蛇、莫比乌斯环、递归图形\n\
- 讲「诞生/起源」→ 中心爆发的光、从一点扩散的涟漪、粒子凝聚\n\
- 讲「寂静/虚无」→ 极简深色背景、单一微弱光源、缓慢消散的元素\n\
\n\
[技术手段 — 充分利用]\n\
- CSS: radial-gradient, conic-gradient, backdrop-filter, mix-blend-mode, clip-path\n\
- SVG: 路径动画、滤镜（feGaussianBlur, feTurbulence）、图案填充、线条网络\n\
- GSAP: stagger 动画、运动路径、缓慢优雅的缓动（power1/power2）、序列编排\n\
- 伪元素 ::before/::after 增加氛围层\n\
- box-shadow 多层柔光、text-shadow 微妙发光\n\
- CSS Grid 创建对称/几何布局\n\
- 用循环生成大量相似元素（如 30 个粒子 div，通过 style 属性设置不同位置）\n\
\n\
[色彩与氛围指导]\n\
- 深色系为主基调（深蓝、深紫、墨黑），用高光点缀（星光白、量子蓝、琥珀金）\n\
- 避免过度饱和的霓虹色，优先使用有深度感的渐变\n\
- 关键概念出现时可以用对比色强调（如暗背景中突然出现的暖光）\n\
- 整体节奏偏沉稳，动画速度中等偏慢，营造思考的空间感\n\
\n\
[禁止]\n\
- 禁止画面与文案内容无关的纯装饰（不要为了花哨而花哨）\n\
- 禁止过于具象的插画风格（不要画卡通人物、写实场景）\n\
- 禁止快速闪烁或过度运动（有声书节奏偏慢，画面要配合）\n\
- 禁止偷懒：不要只放 5-8 个粒子就完事，不要只用一个小几何体代表整个场景\n\
- 文字可以出现也可以不出现，如果出现应是关键词/金句，要有设计感\n\
\n\
[文字排版规则]\n\
- 每段文字放在独立的 clip 中，利用 data-start/data-duration 控制显示时段\n\
- clip 结束后文字自动消失，不需要手动 fade-out 到 opacity:0\n\
- 同一时间只显示一组文字（一个主标题 + 可选副标题）\n\
- 主标题位置统一在 top: 38-42%，副标题在 top: 55-60%";

const HYPERFRAMES_SPEC: &str = "\
[Hyperframes 渲染引擎工作原理 — 你必须理解这个]

渲染器的工作方式（非常重要）：
1. 渲染器逐帧截图，每帧对应一个时间点 t
2. 对于每个 clip 元素，渲染器检查 t 是否在 [data-start, data-start + data-duration] 范围内
   - 在范围内 → clip 可见（display: block）
   - 不在范围内 → clip 被隐藏（display: none）
3. 渲染器调用 tl.seek(t) 将 GSAP timeline 跳转到时间 t
4. 截图当前画面作为该帧

这意味着：
- clip 是天然的「场景开关」！不需要用 opacity 动画来手动显示/隐藏元素
- 一个 clip 在 data-start 之前和 data-start+data-duration 之后完全不存在
- 你应该为每个叙事段落创建独立的 clip，而不是把所有东西塞进一个全时长 clip
- GSAP 动画的时间偏移必须是绝对时间（与 data-start 对齐），因为 tl.seek() 用的是绝对时间

正确用法示例：
  第一段（0-30秒）的星空 → <div class=\"clip\" data-start=\"0\" data-duration=\"30\">
  第二段（30-60秒）的网络 → <div class=\"clip\" data-start=\"30\" data-duration=\"30\">
  渲染器到 t=35 时，第一段自动消失，第二段自动出现，无需任何 opacity 动画

错误用法：
  ❌ 一个 clip data-start=\"0\" data-duration=\"765\" 包含所有元素，靠 GSAP opacity 切换
  ✅ 多个 clip 各自有精确的时间窗口，元素在窗口外自动隐藏

[技术格式]

1. 根元素属性（必须）：
   data-composition-id=\"<id>\"
   data-width=\"1920\"
   data-height=\"1080\"
   data-start=\"0\"
   data-duration=\"<总时长秒数>\"

2. Clip 元素：
   <div class=\"clip\" data-start=\"<秒>\" data-duration=\"<秒>\" data-track-index=\"<层级>\">
   - data-start: 该 clip 开始显示的绝对时间（秒）
   - data-duration: 该 clip 显示持续时间（秒）
   - data-track-index: 层级索引（从 1 开始，数字越大越靠前/上层）
   - 同一 track-index 的多个 clip 可以在不同时间段交替出现

3. Clip 使用策略：
   - 贯穿全程的背景层：一个 clip，data-start=0，data-duration=总时长
   - 分段出现的场景元素：每段一个 clip，精确的 data-start 和 data-duration
   - 文字/标题：每条文字一个独立 clip，显示 8-15 秒后自动消失
   - 利用 clip 的自动隐藏特性，不需要手动 fade-out 到 opacity:0

4. GSAP 动画：
   - 引入: <script src=\"https://cdn.jsdelivr.net/npm/gsap@3.12.5/dist/gsap.min.js\"></script>
   - 创建: const tl = gsap.timeline({ paused: true });
   - 注册: window.__timelines = window.__timelines || {}; window.__timelines[\"<composition-id>\"] = tl;
   - 时间定位：tl.from(\".el\", {...}, 绝对时间) — 这里的时间必须与 clip 的 data-start 对齐
   - 动画应在 clip 的时间窗口内完成（clip 隐藏后动画状态无意义）

5. 禁止使用：
   - Math.random()（破坏确定性渲染）
   - Date.now()（破坏确定性渲染）
   - repeat: -1（无限循环会阻塞渲染）
   - async/await 操作 timeline
   - requestAnimationFrame 自行驱动动画

6. CSS/SVG 动画规则：
   - CSS @keyframes 动画可以使用，但时长必须有限
   - animation-iteration-count 不能为 infinite
   - 所有视觉元素必须在 composition 根元素内部";

const MINIMAL_EXAMPLE: &str = "\
[示例 — 注意：多个 clip 分时段出现，利用 clip 自动隐藏特性]
<!DOCTYPE html>
<html>
<head>
  <meta charset=\"UTF-8\">
  <style>
    [data-composition-id] { background: radial-gradient(ellipse at 50% 50%, #0d0d2b 0%, #050510 70%, #000 100%); overflow: hidden; position: relative; font-family: 'Georgia', serif; }
    .layer { position: absolute; width: 100%; height: 100%; }
    .node { position: absolute; width: 5px; height: 5px; background: rgba(180,200,255,0.8); border-radius: 50%; box-shadow: 0 0 12px rgba(140,160,255,0.6); }
    .dust { position: absolute; width: 2px; height: 2px; background: rgba(255,255,255,0.4); border-radius: 50%; }
    .conn { position: absolute; height: 1px; background: linear-gradient(90deg, transparent, rgba(140,160,255,0.3), transparent); transform-origin: left center; }
    .ring { position: absolute; top: 50%; left: 50%; border: 1px solid rgba(100,150,220,0.2); border-radius: 50%; transform: translate(-50%,-50%); }
    .cloud { position: absolute; top: 50%; left: 50%; width: 500px; height: 500px; margin: -250px; border-radius: 50%; background: radial-gradient(circle, rgba(100,140,255,0.06) 0%, transparent 70%); filter: blur(30px); }
    .grid-line { position: absolute; background: rgba(80,120,180,0.06); }
    .grid-h { width: 100%; height: 1px; }
    .keyword { position: absolute; top: 40%; left: 50%; transform: translateX(-50%); color: rgba(220,230,255,0.9); font-size: 48px; font-weight: 300; letter-spacing: 6px; text-shadow: 0 0 30px rgba(100,140,255,0.4); }
    .sub-text { position: absolute; top: 58%; left: 50%; transform: translateX(-50%); color: rgba(160,180,210,0.7); font-size: 22px; letter-spacing: 3px; }
    .amber-geo { position: absolute; top: 50%; left: 50%; width: 300px; height: 300px; transform: translate(-50%,-50%); border: 1px solid rgba(200,170,80,0.3); clip-path: polygon(50% 0%, 100% 38%, 82% 100%, 18% 100%, 0% 38%); }
  </style>
</head>
<body>
  <div data-composition-id=\"demo\" data-width=\"1920\" data-height=\"1080\" data-start=\"0\" data-duration=\"60\">
    <!-- Track 1: Background (全程可见) -->
    <div class=\"clip layer\" data-start=\"0\" data-duration=\"60\" data-track-index=\"1\">
      <div class=\"grid-line grid-h\" style=\"top:25%\"></div>
      <div class=\"grid-line grid-h\" style=\"top:50%\"></div>
      <div class=\"grid-line grid-h\" style=\"top:75%\"></div>
      <div class=\"cloud\"></div>
      <div class=\"dust\" style=\"top:12%;left:8%\"></div><div class=\"dust\" style=\"top:18%;left:22%\"></div>
      <div class=\"dust\" style=\"top:25%;left:45%\"></div><div class=\"dust\" style=\"top:30%;left:72%\"></div>
      <div class=\"dust\" style=\"top:38%;left:15%\"></div><div class=\"dust\" style=\"top:55%;left:35%\"></div>
      <div class=\"dust\" style=\"top:60%;left:62%\"></div><div class=\"dust\" style=\"top:68%;left:20%\"></div>
      <div class=\"dust\" style=\"top:72%;left:78%\"></div><div class=\"dust\" style=\"top:80%;left:50%\"></div>
      <div class=\"dust\" style=\"top:85%;left:30%\"></div><div class=\"dust\" style=\"top:42%;left:88%\"></div>
      <div class=\"dust\" style=\"top:15%;left:55%\"></div><div class=\"dust\" style=\"top:90%;left:85%\"></div>
      <div class=\"dust\" style=\"top:45%;left:5%\"></div><div class=\"dust\" style=\"top:35%;left:92%\"></div>
    </div>
    <!-- Track 2: Scene A - 网络节点 (0-30秒，30秒后自动消失！) -->
    <div class=\"clip layer\" data-start=\"0\" data-duration=\"30\" data-track-index=\"2\">
      <div class=\"node\" style=\"top:40%;left:30%\"></div>
      <div class=\"node\" style=\"top:50%;left:55%\"></div>
      <div class=\"node\" style=\"top:35%;left:65%\"></div>
      <div class=\"node\" style=\"top:60%;left:40%\"></div>
      <div class=\"conn\" style=\"top:45%;left:30%;width:250px;transform:rotate(8deg)\"></div>
      <div class=\"conn\" style=\"top:48%;left:55%;width:180px;transform:rotate(-15deg)\"></div>
      <div class=\"ring\" style=\"width:200px;height:200px\"></div>
      <div class=\"ring\" style=\"width:350px;height:350px\"></div>
    </div>
    <!-- Track 2: Scene B - 琥珀几何 (30-60秒，30秒时自动出现！) -->
    <div class=\"clip layer\" data-start=\"30\" data-duration=\"30\" data-track-index=\"2\">
      <div class=\"amber-geo\"></div>
      <div class=\"ring\" style=\"width:400px;height:400px;border-color:rgba(200,170,80,0.2)\"></div>
      <div class=\"ring\" style=\"width:600px;height:600px;border-color:rgba(200,170,80,0.1)\"></div>
    </div>
    <!-- Track 3: Text A (5-20秒，20秒后自动消失，无需 fade-out) -->
    <div class=\"clip\" data-start=\"5\" data-duration=\"15\" data-track-index=\"3\">
      <p class=\"keyword\">存在 即是 关系</p>
    </div>
    <!-- Track 3: Text B (35-50秒，35秒时自动出现) -->
    <div class=\"clip\" data-start=\"35\" data-duration=\"15\" data-track-index=\"3\">
      <p class=\"keyword\" style=\"color:rgba(220,200,140,0.9)\">时间是意识的幻觉</p>
      <p class=\"sub-text\">宇宙是一块静止的四维琥珀</p>
    </div>
    <script src=\"https://cdn.jsdelivr.net/npm/gsap@3.12.5/dist/gsap.min.js\"></script>
    <script>
      window.__timelines = window.__timelines || {};
      const tl = gsap.timeline({ paused: true });
      // Background: dust drifts throughout
      tl.to('.dust', { y: -60, x: 15, duration: 55, stagger: 0.2, ease: 'none' }, 2);
      tl.fromTo('.cloud', {scale:0.8, opacity:0.3}, {scale:1.2, opacity:0.6, duration:25, ease:'power1.inOut', yoyo:true, repeat:1}, 0);
      // Scene A (0-30s): nodes and connections animate in
      tl.fromTo('.node', {opacity:0, scale:0}, {opacity:1, scale:1, duration:1.5, stagger:0.4, ease:'back.out(1.7)'}, 2);
      tl.fromTo('.conn', {scaleX:0}, {scaleX:1, opacity:0.6, duration:2, stagger:0.5, ease:'power2.out'}, 4);
      tl.to('.ring', {scale:1.1, opacity:0.4, duration:8, yoyo:true, repeat:1, ease:'power1.inOut'}, 5);
      tl.to('.node', {boxShadow:'0 0 20px rgba(180,200,255,1)', duration:2, stagger:0.3, yoyo:true, repeat:3}, 8);
      // Text A (5-20s): fade in within its clip window
      tl.fromTo('.keyword', {opacity:0, letterSpacing:'2px'}, {opacity:1, letterSpacing:'8px', duration:2.5, ease:'power2.out'}, 6);
      // Scene B (30-60s): amber geometry appears (clip auto-shows at t=30)
      tl.fromTo('.amber-geo', {opacity:0, scale:0.7, rotation:-5}, {opacity:0.8, scale:1, rotation:5, duration:5, ease:'power1.inOut'}, 31);
      tl.to('.amber-geo', {rotation:-3, duration:20, ease:'power1.inOut', yoyo:true, repeat:1}, 36);
      // Text B (35-50s): auto-visible at t=35
      tl.fromTo('.keyword', {opacity:0}, {opacity:1, duration:2, ease:'power1.out'}, 36);
      tl.fromTo('.sub-text', {opacity:0, y:10}, {opacity:0.8, y:0, duration:2.5, ease:'power1.out'}, 39);
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
        assert!(prompt.contains("视觉创作专家"));
        assert!(prompt.contains("Hyperframes"));
    }

    #[test]
    fn test_build_system_prompt_contains_creative_freedom() {
        let prompt = build_system_prompt();
        assert!(prompt.contains("概念可视化"));
        assert!(prompt.contains("多个 track"));
        assert!(prompt.contains("禁止"));
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
        let spec_section = HYPERFRAMES_SPEC;
        let spec_chars = spec_section.len();
        assert!(
            spec_chars < 4000,
            "Spec section too long: {} chars",
            spec_chars
        );
        // Full prompt can be longer now due to richer creative guidance
        assert!(
            prompt.len() < 16000,
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
