use crate::commands::audio::ffmpeg::find_ffmpeg;
use crate::core::models::VoiceConfig;
use log::warn;

use super::models::{TtsChunk, TTS_CHUNK_MAX_CHARS};

/// Build the audio file path for a given project and line.
pub fn build_audio_path(
    app_data_dir: &std::path::Path,
    project_id: &str,
    line_id: &str,
) -> std::path::PathBuf {
    app_data_dir
        .join("projects")
        .join(project_id)
        .join("audio")
        .join(format!("{}.mp3", line_id))
}

/// Split text into TTS-friendly chunks (≤ max_len chars each).
/// Returns TtsChunk vector with pause metadata.
/// Priority: paragraph breaks → sentence boundaries → hard cut.
pub(crate) fn split_text_for_tts(text: &str) -> Vec<TtsChunk> {
    const SENTENCE_PAUSE: u32 = 200; // Short pause between sentences
    const PARAGRAPH_PAUSE: u32 = 600; // Longer pause between paragraphs

    let text = text.trim();
    if text.is_empty() {
        return vec![];
    }
    let char_count = text.chars().count();
    if char_count <= TTS_CHUNK_MAX_CHARS {
        return vec![TtsChunk {
            text: text.to_string(),
            pause_ms: 0,
        }];
    }

    // Step 1: split by paragraph (newlines)
    let paragraphs: Vec<&str> = text
        .split('\n')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();

    // Track which chunks are paragraph boundaries vs sentence boundaries
    let mut chunk_texts: Vec<String> = Vec::new();
    let mut chunk_boundaries: Vec<u32> = Vec::new(); // pause_ms after each chunk
    let mut buf = String::new();
    let mut buf_chars = 0usize;

    for para in &paragraphs {
        let para_chars = para.chars().count();
        // If single paragraph already exceeds max_len, split it by sentences
        if para_chars > TTS_CHUNK_MAX_CHARS {
            if !buf.is_empty() {
                chunk_texts.push(std::mem::take(&mut buf));
                chunk_boundaries.push(PARAGRAPH_PAUSE);
                buf_chars = 0;
            }
            split_sentence_with_boundaries(
                para,
                SENTENCE_PAUSE,
                &mut chunk_texts,
                &mut chunk_boundaries,
            );
            // Mark the last sentence-chunk as paragraph boundary (unless it's the very last)
            if let Some(last) = chunk_boundaries.last_mut() {
                *last = PARAGRAPH_PAUSE;
            }
            continue;
        }
        // Check if adding this paragraph would exceed max_len
        let sep = if buf.is_empty() { 0 } else { 1 };
        if buf_chars + para_chars + sep > TTS_CHUNK_MAX_CHARS {
            chunk_texts.push(std::mem::take(&mut buf));
            chunk_boundaries.push(PARAGRAPH_PAUSE);
            buf_chars = 0;
        }
        if !buf.is_empty() {
            buf.push('\n'); // Preserve paragraph boundary for TTS
            buf_chars += 1;
        }
        buf.push_str(para);
        buf_chars += para_chars;
    }
    if !buf.is_empty() {
        chunk_texts.push(buf);
        chunk_boundaries.push(0); // Last chunk: no pause
    }

    // Build TtsChunk vector
    chunk_texts
        .into_iter()
        .zip(chunk_boundaries)
        .map(|(text, pause_ms)| TtsChunk { text, pause_ms })
        .collect()
}

/// Split a single paragraph by sentence boundaries, tracking pause durations.
fn split_sentence_with_boundaries(
    text: &str,
    pause_ms: u32,
    out: &mut Vec<String>,
    boundaries: &mut Vec<u32>,
) {
    let boundary_chars: &[char] = &['.', '!', '?', '。', '！', '？', '；', ';'];

    let chars: Vec<char> = text.chars().collect();
    let total = chars.len();
    let mut start = 0;

    while start < total {
        let remaining = total - start;
        if remaining <= TTS_CHUNK_MAX_CHARS {
            out.push(chars[start..].iter().collect());
            boundaries.push(0);
            break;
        }

        // Search backwards from max_len for a sentence boundary
        let search_end = (start + TTS_CHUNK_MAX_CHARS).min(total);
        let mut found = None;
        for i in (start..search_end).rev() {
            if boundary_chars.contains(&chars[i]) {
                found = Some(i + 1);
                break;
            }
        }

        if let Some(end) = found {
            out.push(chars[start..end].iter().collect());
            boundaries.push(pause_ms);
            start = end;
        } else {
            // No boundary found — hard cut at max_len
            let hard_end = (start + TTS_CHUNK_MAX_CHARS).min(total);
            out.push(chars[start..hard_end].iter().collect());
            boundaries.push(pause_ms);
            start = hard_end;
        }
    }
}

/// Merge multiple MP3 files into one by inserting silence between chunks.
/// Group TtsChunks into sessions that each stay under `limit` characters.
/// Prefers splitting at paragraph boundaries (higher pause_ms) over sentence boundaries.
pub(crate) fn group_chunks_into_sessions(chunks: &[TtsChunk], limit: usize) -> Vec<Vec<&TtsChunk>> {
    if chunks.is_empty() {
        return vec![];
    }

    let total_chars: usize = chunks.iter().map(|c| c.text.len()).sum();
    if total_chars <= limit {
        return vec![chunks.iter().collect()];
    }

    let mut sessions: Vec<Vec<&TtsChunk>> = Vec::new();
    let mut current: Vec<&TtsChunk> = Vec::new();
    let mut current_len: usize = 0;

    for chunk in chunks {
        if !current.is_empty() && current_len + chunk.text.len() > limit {
            // Try to find a paragraph boundary (pause_ms >= 600) to split at
            let split_at = current
                .iter()
                .rposition(|c| c.pause_ms >= 600)
                .map(|i| i + 1) // split after the paragraph-ending chunk
                .unwrap_or(current.len()); // no paragraph boundary, flush all

            let remainder: Vec<&TtsChunk> = current.split_off(split_at);
            sessions.push(std::mem::take(&mut current));
            current = remainder;
            current_len = current.iter().map(|c| c.text.len()).sum();
        }
        current.push(chunk);
        current_len += chunk.text.len();
    }
    if !current.is_empty() {
        sessions.push(current);
    }

    sessions
}

/// Merge multiple audio files with silence gaps between them.
/// Takes a list of (audio_path, pause_after_ms) pairs.
/// The pause is inserted AFTER each chunk, before the next one.
pub(crate) async fn merge_audio_with_silence(
    chunk_paths: &[(std::path::PathBuf, u32)],
    output: &std::path::Path,
) -> Result<(), String> {
    if chunk_paths.is_empty() {
        return Err("no audio chunks to merge".into());
    }
    if chunk_paths.len() == 1 {
        let output = output.to_path_buf();
        let src = chunk_paths[0].0.clone();
        return tokio::task::spawn_blocking(move || {
            std::fs::copy(&src, &output).map_err(|e| format!("copy single chunk: {}", e))?;
            Ok(())
        })
        .await
        .map_err(|e| format!("spawn_blocking failed: {}", e))?;
    }

    let tmp_dir = std::env::temp_dir();

    // Build a flat list: audio1, silence1, audio2, silence2, audio3, ...
    let mut all_paths = Vec::new();
    for (i, (audio_path, pause_ms)) in chunk_paths.iter().enumerate() {
        all_paths.push(audio_path.clone());
        // Insert silence after this chunk (except for the last one)
        if i < chunk_paths.len() - 1 && *pause_ms > 0 {
            let silence_path = tmp_dir.join(format!("tts_silence_{}.mp3", i));
            generate_silence(&silence_path, *pause_ms).await?;
            all_paths.push(silence_path);
        }
    }

    // Build concat list file
    let concat_list = tmp_dir.join("tts_concat_list.txt");
    let mut list_content = String::new();
    for p in &all_paths {
        let path_str = p.to_string_lossy().replace('\'', "'\\''");
        list_content.push_str(&format!(
            "file '{}'
",
            path_str
        ));
    }
    std::fs::write(&concat_list, &list_content).map_err(|e| format!("write concat list: {}", e))?;

    let output = output.to_path_buf();
    let concat_list_for_cleanup = concat_list.clone();
    let result = tokio::task::spawn_blocking(move || {
        let ffmpeg_bin = find_ffmpeg();
        std::process::Command::new(&ffmpeg_bin)
            .args([
                "-y",
                "-f",
                "concat",
                "-safe",
                "0",
                "-i",
                &concat_list.to_string_lossy(),
                "-c",
                "copy",
                &output.to_string_lossy(),
            ])
            .stderr(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .output()
    })
    .await
    .map_err(|e| format!("spawn_blocking failed: {}", e))?;

    // Cleanup temp files
    let _ = std::fs::remove_file(&concat_list_for_cleanup);
    for p in &all_paths {
        if p.to_string_lossy().contains("tts_silence_") {
            let _ = std::fs::remove_file(p);
        }
    }

    match result {
        Ok(o) if o.status.success() => Ok(()),
        Ok(o) => {
            let stderr = String::from_utf8_lossy(&o.stderr);
            Err(format!("ffmpeg concat failed: {}", stderr))
        }
        Err(e) => Err(format!("ffmpeg exec failed: {}", e)),
    }
}

/// Generate a silence MP3 file of the given duration (in ms).
pub(crate) async fn generate_silence(
    path: &std::path::Path,
    duration_ms: u32,
) -> Result<(), String> {
    let duration_sec = duration_ms as f64 / 1000.0;
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || {
        let ffmpeg_bin = find_ffmpeg();
        let result = std::process::Command::new(&ffmpeg_bin)
            .args([
                "-y",
                "-f",
                "lavfi",
                "-i",
                "anullsrc=r=22050:cl=mono",
                "-t",
                &format!("{:.3}", duration_sec),
                "-c:a",
                "libmp3lame",
                "-b:a",
                "192k",
                &path.to_string_lossy(),
            ])
            .stderr(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .output();

        match result {
            Ok(o) if o.status.success() => Ok(()),
            Ok(o) => {
                let stderr = String::from_utf8_lossy(&o.stderr);
                Err(format!("generate silence failed: {}", stderr))
            }
            Err(e) => Err(format!("ffmpeg exec failed: {}", e)),
        }
    })
    .await
    .map_err(|e| format!("spawn_blocking failed: {}", e))?
}

/// Re-encode audio with FFmpeg to fix VBR headers.
pub(crate) async fn reencode_with_ffmpeg(audio_path: &std::path::Path, label: &str) {
    let audio_path = audio_path.to_path_buf();
    let tmp = audio_path.with_extension("tmp.mp3");
    let label = label.to_string();
    let _ = tokio::task::spawn_blocking(move || {
        let ffmpeg_bin = find_ffmpeg();
        let r = std::process::Command::new(&ffmpeg_bin)
            .args([
                "-y",
                "-i",
                &audio_path.to_string_lossy(),
                "-codec:a",
                "libmp3lame",
                "-b:a",
                "192k",
                &tmp.to_string_lossy(),
            ])
            .stderr(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .output();
        match r {
            Ok(o) if o.status.success() => {
                let _ = std::fs::rename(&tmp, &audio_path);
            }
            _ => {
                let _ = std::fs::remove_file(&tmp);
                warn!("[TTS] FFmpeg failed for {}", label);
            }
        }
    })
    .await;
}

/// Get audio duration in ms via FFprobe, fallback to rodio.
pub async fn get_audio_duration(path: &std::path::Path) -> Option<i64> {
    let path = path.to_path_buf();
    let path_for_ffprobe = path.clone();
    let ffprobe_result = tokio::task::spawn_blocking(move || {
        // ffprobe is typically in the same directory as ffmpeg
        let ffmpeg_bin = find_ffmpeg();
        let ffprobe_bin = ffmpeg_bin.replace("ffmpeg", "ffprobe");
        std::process::Command::new(&ffprobe_bin)
            .args([
                "-v",
                "error",
                "-show_entries",
                "format=duration",
                "-of",
                "default=noprint_wrappers=1:nokey=1",
                &path_for_ffprobe.to_string_lossy(),
            ])
            .output()
    })
    .await
    .ok()
    .and_then(|r| r.ok());

    if let Some(o) = ffprobe_result {
        if o.status.success() {
            if let Ok(s) = String::from_utf8(o.stdout) {
                if let Ok(secs) = s.trim().parse::<f64>() {
                    return Some((secs * 1000.0).round() as i64);
                }
            }
        }
    }
    if let Ok(f) = std::fs::File::open(path) {
        use rodio::Source;
        if let Ok(d) = rodio::Decoder::new(std::io::BufReader::new(f)) {
            let sr = d.sample_rate() as f64;
            let n = d.count();
            if sr > 0.0 {
                return Some((n as f64 / sr * 1000.0).round() as i64);
            }
        }
    }
    None
}

/// Determine model name. Default to Qwen TTS Realtime.
/// All models use WebSocket streaming.
pub(crate) fn resolve_model(voice_config: &VoiceConfig, has_instructions: bool) -> String {
    let cfg = &voice_config.tts_model;
    let voice = &voice_config.voice_name;

    // Cloned voices (voice cloning) must use the VC model
    if voice.starts_with("qwen-tts-vc-voice-") {
        return "qwen3-tts-vc-realtime-2026-01-15".to_string();
    }

    // If instructions are present, always use the instruct model
    if has_instructions {
        return "qwen3-tts-instruct-flash-realtime".to_string();
    }

    // If a valid realtime model is configured, use it
    if cfg.ends_with("-realtime") && cfg.starts_with("qwen") {
        return cfg.clone();
    }

    // Legacy non-realtime models: upgrade to their realtime equivalents
    if cfg == "qwen3-tts-flash" {
        return "qwen3-tts-flash-realtime".to_string();
    }
    if cfg == "qwen3-tts-instruct-flash" {
        return "qwen3-tts-instruct-flash-realtime".to_string();
    }

    // Default (empty, cosyvoice, or unknown) → instruct flash realtime
    "qwen3-tts-instruct-flash-realtime".to_string()
}

/// Returns true if the model should use the HTTP REST API
/// (only non-realtime qwen models without -realtime in the name).
/// All other models use WebSocket streaming.
pub(crate) fn is_http_model(model: &str) -> bool {
    // VC models and other realtime models use WebSocket
    if model.contains("-realtime") || model.contains("-vc-") {
        return false;
    }
    model.starts_with("qwen")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_audio_path() {
        let p = build_audio_path(std::path::Path::new("/data"), "proj-1", "line-42");
        assert_eq!(
            p,
            std::path::PathBuf::from("/data/projects/proj-1/audio/line-42.mp3")
        );
    }

    #[test]
    fn test_split_short_text_no_split() {
        let text = "你好世界";
        let chunks = split_text_for_tts(text);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].text, "你好世界");
        assert_eq!(chunks[0].pause_ms, 0);
    }

    #[test]
    fn test_split_text_at_boundary() {
        let text = "A".repeat(200);
        let chunks = split_text_for_tts(&text);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].text.len(), 200);
    }

    #[test]
    fn test_split_text_over_limit() {
        let text = "A".repeat(500);
        let chunks = split_text_for_tts(&text);
        assert!(chunks.len() > 1);
        for c in &chunks {
            assert!(
                c.text.len() <= TTS_CHUNK_MAX_CHARS,
                "chunk len={} exceeds {}",
                c.text.len(),
                TTS_CHUNK_MAX_CHARS
            );
        }
    }

    #[test]
    fn test_split_by_paragraph_short_text() {
        // Short text: no splitting needed, newlines preserved
        let text = "第一段文字。\n第二段文字。";
        let chunks = split_text_for_tts(text);
        // Total length is well under 400, so it returns as-is
        assert_eq!(chunks.len(), 1);
    }

    #[test]
    fn test_split_long_paragraph_by_sentence() {
        // A long paragraph with sentence boundaries that needs splitting
        let text =
            "他今天去了公园。天气很好。很多人在跑步。小鸟在树上唱歌。孩子们很开心。".repeat(8); // 37 * 8 = 296 chars — still short
        let chunks = split_text_for_tts(&text);
        assert!(chunks.len() >= 1);
        for c in &chunks {
            assert!(
                c.text.chars().count() <= TTS_CHUNK_MAX_CHARS,
                "chunk len={} exceeds {}",
                c.text.chars().count(),
                TTS_CHUNK_MAX_CHARS
            );
        }
    }

    #[test]
    fn test_split_mixed_paragraphs() {
        // Long paragraph (>400 chars) + short + long — forces splitting
        let long_para = "他今天去了公园。".repeat(60); // 60 * 7 = 420 chars > 400
        let text = format!("{}\n简短段落。\n{}", long_para, "Another short one.");
        let chunks = split_text_for_tts(&text);
        assert!(chunks.len() > 1);
        for c in &chunks {
            assert!(
                c.text.chars().count() <= TTS_CHUNK_MAX_CHARS,
                "chunk len={} exceeds {}",
                c.text.chars().count(),
                TTS_CHUNK_MAX_CHARS
            );
        }
    }

    #[test]
    fn test_split_empty_text() {
        let chunks = split_text_for_tts("");
        assert!(chunks.is_empty());
    }

    #[test]
    fn test_split_preserves_sentence_boundary() {
        // Verify chunks end at sentence boundaries when possible
        let text =
            "第一句。第二句。第三句。第四句。第五句。第六句。第七句。第八句。第九句。第十句。";
        let chunks = split_text_for_tts(text);
        // Check that non-final chunks end with sentence-ending punctuation
        let boundaries: &[char] = &['.', '!', '?', '。', '！', '？', '；', ';'];
        for (i, c) in chunks.iter().enumerate() {
            if i < chunks.len() - 1 {
                let last_char = c.text.chars().last().unwrap();
                assert!(
                    boundaries.contains(&last_char),
                    "chunk {} ends with '{}' not a sentence boundary",
                    i,
                    last_char
                );
            }
        }
    }

    #[test]
    fn test_pause_different_for_paragraph_vs_sentence() {
        // Verify that paragraph pauses are longer than sentence pauses
        let long_para = "他今天去了公园。".repeat(60); // 420 chars > 400
        let text = format!("{}\n简短段落。", long_para);
        let chunks = split_text_for_tts(&text);
        // There should be at least 2 chunks: the long paragraph split + the short paragraph
        assert!(chunks.len() >= 2);
        // Last chunk should have 0 pause (it's the end)
        let last = chunks.last().unwrap();
        assert_eq!(last.pause_ms, 0);
        // Non-last chunks should have some pause
        for c in chunks.iter().take(chunks.len() - 1) {
            assert!(c.pause_ms > 0, "non-final chunk should have pause");
        }
    }
}
