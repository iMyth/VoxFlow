use std::sync::Mutex;

use tauri::Emitter;
use tauri::Manager;

use crate::core::db::Database;
use crate::core::error::AppError;
use crate::core::models::{AudioFragment, TtsBatchProgress, VoiceConfig};
use futures_util::{SinkExt, StreamExt};
use log::{debug, error, info, warn};
use tokio_tungstenite::tungstenite::Message;

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

/// Maximum characters per TTS chunk.
/// The API limit for qwen3-tts-instruct-flash is 600 (likely byte-counted).
/// Chinese chars are 3 bytes in UTF-8, so 600 bytes ≈ 200 Chinese chars.
/// Use 200 as a safe threshold.
const TTS_CHUNK_MAX_CHARS: usize = 200;

/// Chunk with metadata about the pause to insert after it (in ms).
struct TtsChunk {
    text: String,
    /// Pause duration in ms to insert AFTER this chunk, before the next one.
    /// 0 for the last chunk.
    pause_ms: u32,
}

/// Split text into TTS-friendly chunks (≤ max_len chars each).
/// Returns TtsChunk vector with pause metadata.
/// Priority: paragraph breaks → sentence boundaries → hard cut.
fn split_text_for_tts(text: &str) -> Vec<TtsChunk> {
    const SENTENCE_PAUSE: u32 = 200;  // Short pause between sentences
    const PARAGRAPH_PAUSE: u32 = 600; // Longer pause between paragraphs

    let text = text.trim();
    if text.is_empty() {
        return vec![];
    }
    let char_count = text.chars().count();
    if char_count <= TTS_CHUNK_MAX_CHARS {
        return vec![TtsChunk { text: text.to_string(), pause_ms: 0 }];
    }

    // Step 1: split by paragraph (newlines)
    let paragraphs: Vec<&str> = text
        .split(|c: char| c == '\n')
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
            split_sentence_with_boundaries(para, SENTENCE_PAUSE, &mut chunk_texts, &mut chunk_boundaries);
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
            buf.push('\n');  // Preserve paragraph boundary for TTS
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
    chunk_texts.into_iter().zip(chunk_boundaries.into_iter())
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
fn group_chunks_into_sessions(chunks: &[TtsChunk], limit: usize) -> Vec<Vec<&TtsChunk>> {
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
            let split_at = current.iter().rposition(|c| c.pause_ms >= 600)
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
async fn merge_audio_with_silence(
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
        list_content.push_str(&format!("file '{}'\n", path_str));
    }
    std::fs::write(&concat_list, &list_content)
        .map_err(|e| format!("write concat list: {}", e))?;

    let output = output.to_path_buf();
    let concat_list_for_cleanup = concat_list.clone();
    let result = tokio::task::spawn_blocking(move || {
        std::process::Command::new("ffmpeg")
            .args([
                "-y",
                "-f", "concat",
                "-safe", "0",
                "-i", &concat_list.to_string_lossy(),
                "-c", "copy",
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
async fn generate_silence(path: &std::path::Path, duration_ms: u32) -> Result<(), String> {
    let duration_sec = duration_ms as f64 / 1000.0;
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || {
        let result = std::process::Command::new("ffmpeg")
            .args([
                "-y",
                "-f", "lavfi",
                "-i", "anullsrc=r=22050:cl=mono",
                "-t", &format!("{:.3}", duration_sec),
                "-c:a", "libmp3lame",
                "-b:a", "192k",
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

// ---------------------------------------------------------------------------
// Qwen TTS Realtime WebSocket Protocol
// ---------------------------------------------------------------------------
// Uses wss://dashscope.aliyuncs.com/api-ws/v1/inference
// Protocol: session.update → text.input(s) → text.complete → audio.delta(s) → audio.completed
// ---------------------------------------------------------------------------

/// Connect to the DashScope WebSocket endpoint for Qwen TTS Realtime.
async fn ws_realtime_connect(
    api_key: &str,
    model: &str,
) -> Result<
    tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    AppError,
> {
    let url = format!("wss://dashscope.aliyuncs.com/api-ws/v1/realtime?model={}", model);
    let request = http::Request::builder()
        .uri(url)
        .header("Authorization", format!("bearer {}", api_key))
        .header("X-DashScope-DataInspection", "enable")
        .header("Host", "dashscope.aliyuncs.com")
        .header("Connection", "Upgrade")
        .header("Upgrade", "websocket")
        .header("Sec-WebSocket-Version", "13")
        .header(
            "Sec-WebSocket-Key",
            tokio_tungstenite::tungstenite::handshake::client::generate_key(),
        )
        .body(())
        .map_err(|e| AppError::TtsService(format!("WS request build error: {}", e)))?;

    let (ws, _) = tokio_tungstenite::connect_async(request)
        .await
        .map_err(|e| AppError::TtsService(format!("WS connect failed: {}", e)))?;

    info!("[TTS][ws-realtime] Connected");
    Ok(ws)
}

/// Run TTS on a WebSocket connection using the Qwen Realtime protocol.
/// Sends session.update → text.input(s) → text.complete, collects audio.
async fn ws_realtime_run_task<S>(
    ws: &mut S,
    texts: &[&str],
    voice_config: &VoiceConfig,
    instructions: Option<&str>,
    model: &str,
) -> Result<Vec<u8>, AppError>
where
    S: futures_util::Sink<Message, Error = tokio_tungstenite::tungstenite::Error>
        + futures_util::Stream<Item = Result<Message, tokio_tungstenite::tungstenite::Error>>
        + Unpin,
{
    use serde_json::json;

    let has_instr = instructions.map(|i| !i.trim().is_empty()).unwrap_or(false);
    let is_vc = model.starts_with("qwen3-tts-vc");
    info!("[TTS][ws-realtime] model={}, voice={}, texts={}", model, voice_config.voice_name, texts.len());

    // Step 1: session.update
    let mut session_obj = serde_json::Map::new();
    session_obj.insert("mode".into(), json!("server_commit"));
    session_obj.insert("model".into(), json!(model));
    session_obj.insert("voice".into(), json!(voice_config.voice_name));
    session_obj.insert("response_format".into(), json!("mp3"));
    session_obj.insert("sample_rate".into(), json!(24000));
    if has_instr && !is_vc {
        // VC models don't support instructions
        session_obj.insert("instructions".into(), json!(instructions.unwrap()));
    }

    let session_update = json!({
        "type": "session.update",
        "session": session_obj,
    });

    debug!("[TTS][ws-realtime] session.update: {}", serde_json::to_string(&session_update).unwrap_or_default());
    ws.send(Message::Text(session_update.to_string()))
        .await
        .map_err(|e| AppError::TtsService(format!("send session.update: {}", e)))?;

    // Step 2: input_text_buffer.append for each text chunk
    for text in texts {
        let text_input = json!({
            "type": "input_text_buffer.append",
            "text": text,
        });
        ws.send(Message::Text(text_input.to_string()))
            .await
            .map_err(|e| AppError::TtsService(format!("send input_text_buffer.append: {}", e)))?;
    }

    // Step 3: session.finish to signal no more text input
    let session_finish = json!({
        "type": "session.finish",
    });
    ws.send(Message::Text(session_finish.to_string()))
        .await
        .map_err(|e| AppError::TtsService(format!("send session.finish: {}", e)))?;

    // Step 4: collect audio deltas (audio arrives as base64 in text frames)
    let mut audio = Vec::<u8>::new();
    let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_secs(600);
    let finished_sending = true; // session.finish already sent above
    let mut idle_deadline: Option<tokio::time::Instant> = None;

    loop {
        // Use a short idle timeout after response.done to detect completion
        let effective_deadline = idle_deadline.unwrap_or(deadline);
        let msg = tokio::time::timeout_at(effective_deadline, ws.next()).await;
        match msg {
            Err(_) if idle_deadline.is_some() => {
                // Idle timeout after last response.done — no more responses coming
                info!("[TTS][ws-realtime] no more responses, finishing: {} bytes", audio.len());
                break;
            }
            Err(_) => return Err(AppError::TtsService("WS task timed out (600s)".into())),
            Ok(None) => {
                // Connection closed — if we have audio, treat as success
                if !audio.is_empty() {
                    info!("[TTS][ws-realtime] WS closed, returning {} bytes", audio.len());
                    break;
                }
                return Err(AppError::TtsService("WS closed unexpectedly".into()));
            }
            Ok(Some(Err(e))) => {
                if !audio.is_empty() {
                    warn!("[TTS][ws-realtime] WS error after receiving audio: {}", e);
                    break;
                }
                return Err(AppError::TtsService(format!("WS error: {}", e)));
            }
            Ok(Some(Ok(Message::Text(t)))) => {
                if let Ok(evt) = serde_json::from_str::<serde_json::Value>(&t) {
                    let event_type = evt["type"].as_str().unwrap_or("");
                    match event_type {
                        "session.created" => {
                            debug!("[TTS][ws-realtime] session.created");
                        }
                        "session.updated" => {
                            debug!("[TTS][ws-realtime] session.updated");
                        }
                        "response.created" => {
                            debug!("[TTS][ws-realtime] response.created");
                            idle_deadline = None; // new response started, cancel idle timer
                        }
                        "response.audio.delta" => {
                            if let Some(delta) = evt["delta"].as_str() {
                                use base64::Engine;
                                if let Ok(decoded) = base64::engine::general_purpose::STANDARD.decode(delta) {
                                    audio.extend_from_slice(&decoded);
                                }
                            }
                        }
                        "response.audio.done" => {
                            debug!("[TTS][ws-realtime] response.audio.done");
                        }
                        "response.done" => {
                            info!("[TTS][ws-realtime] response.done: {} bytes", audio.len());
                            // Start idle timer — if no new response within 5s, we're done
                            if finished_sending {
                                idle_deadline = Some(tokio::time::Instant::now() + tokio::time::Duration::from_secs(5));
                            }
                        }
                        "session.finished" => {
                            info!("[TTS][ws-realtime] session.finished: {} bytes", audio.len());
                            break;
                        }
                        "error" => {
                            let code = evt["error"]["code"].as_str().unwrap_or("");
                            let msg = evt["error"]["message"].as_str().unwrap_or("");
                            error!("[TTS][ws-realtime] error: {} - {}", code, msg);
                            return Err(AppError::TtsService(format!("TTS error: {} - {}", code, msg)));
                        }
                        _ => {
                            debug!("[TTS][ws-realtime] event: {}", event_type);
                        }
                    }
                }
            }
            Ok(Some(Ok(_))) => {} // binary/ping/pong
        }
    }

    if audio.is_empty() {
        return Err(AppError::TtsService("No audio received".into()));
    }
    Ok(audio)
}

/// Re-encode audio with FFmpeg to fix VBR headers.
async fn reencode_with_ffmpeg(audio_path: &std::path::Path, label: &str) {
    let audio_path = audio_path.to_path_buf();
    let tmp = audio_path.with_extension("tmp.mp3");
    let label = label.to_string();
    let _ = tokio::task::spawn_blocking(move || {
        let r = std::process::Command::new("ffmpeg")
            .args(["-y", "-i", &audio_path.to_string_lossy(), "-codec:a", "libmp3lame", "-b:a", "192k", &tmp.to_string_lossy()])
            .stderr(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .output();
        match r {
            Ok(o) if o.status.success() => { let _ = std::fs::rename(&tmp, &audio_path); }
            _ => { let _ = std::fs::remove_file(&tmp); warn!("[TTS] FFmpeg failed for {}", label); }
        }
    }).await;
}

/// Get audio duration in ms via FFprobe, fallback to rodio.
pub(crate) async fn get_audio_duration(path: &std::path::Path) -> Option<i64> {
    let path = path.to_path_buf();
    let path_for_ffprobe = path.clone();
    let ffprobe_result = tokio::task::spawn_blocking(move || {
        std::process::Command::new("ffprobe")
            .args(["-v", "error", "-show_entries", "format=duration", "-of", "default=noprint_wrappers=1:nokey=1", &path_for_ffprobe.to_string_lossy()])
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
            if sr > 0.0 { return Some((n as f64 / sr * 1000.0).round() as i64); }
        }
    }
    None
}

/// Determine model name. Default to Qwen TTS Realtime.
/// All models use WebSocket streaming.
fn resolve_model(voice_config: &VoiceConfig, has_instructions: bool) -> String {
    let cfg = &voice_config.tts_model;
    let voice = &voice_config.voice_name;

    // Cloned voices (voice cloning) must use the VC model
    if voice.starts_with("qwen-tts-vc-voice-") {
        return "qwen3-tts-vc-realtime-2026-01-15".to_string();
    }

    if has_instructions && cfg.starts_with("qwen") {
        if cfg.ends_with("-realtime") {
            return "qwen3-tts-instruct-flash-realtime".to_string();
        }
        // Non-realtime instruct falls back to HTTP
        return "qwen3-tts-instruct-flash".to_string();
    }

    if cfg.is_empty() || cfg.starts_with("cosyvoice") {
        // Default to Qwen TTS Realtime (CosyVoice no longer supported)
        return "qwen3-tts-instruct-flash-realtime".to_string();
    }

    // If user configured a non-realtime qwen model, use it as-is
    cfg.clone()
}

/// Returns true if the model should use the HTTP REST API
/// (only non-realtime qwen models without -realtime in the name).
/// All other models use WebSocket streaming.
fn is_http_model(model: &str) -> bool {
    // VC models and other realtime models use WebSocket
    if model.contains("-realtime") || model.contains("-vc-") {
        return false;
    }
    model.starts_with("qwen")
}

// ---------------------------------------------------------------------------
// HTTP REST API for Qwen-TTS models
// ---------------------------------------------------------------------------
// POST https://dashscope.aliyuncs.com/api/v1/services/aigc/multimodal-generation/generation
// These models don't work on the CosyVoice WS endpoint.
// ---------------------------------------------------------------------------

async fn call_http_tts(
    text: &str,
    voice_config: &VoiceConfig,
    instructions: Option<&str>,
    api_key: &str,
    model: &str,
) -> Result<Vec<u8>, AppError> {
    use reqwest::header::CONTENT_TYPE;
    use serde_json::json;

    let has_instr = instructions.map(|i| !i.trim().is_empty()).unwrap_or(false);

    info!("[TTS][http] model={}, voice={}, text_len={}, has_instr={}", model, voice_config.voice_name, text.len(), has_instr);

    let mut input = serde_json::Map::new();
    input.insert("text".to_string(), json!(text));
    input.insert("voice".to_string(), json!(voice_config.voice_name));
    if has_instr {
        input.insert("instructions".to_string(), json!(instructions.unwrap()));
        input.insert("optimize_instructions".to_string(), json!(true));
    }

    let body = json!({ "model": model, "input": input });
    debug!("[TTS][http] Request body:\n{}", serde_json::to_string_pretty(&body).unwrap_or_default());

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|e| AppError::TtsService(format!("HTTP client error: {}", e)))?;

    let response = client
        .post("https://dashscope.aliyuncs.com/api/v1/services/aigc/multimodal-generation/generation")
        .header(CONTENT_TYPE, "application/json")
        .header("Authorization", format!("Bearer {}", api_key))
        .body(body.to_string())
        .send()
        .await
        .map_err(|e| AppError::TtsService(format!("百炼 TTS 请求失败: {}", e)))?;

    let status = response.status();
    info!("[TTS][http] Response status: {}", status);
    if !status.is_success() {
        let body_text = response.text().await.unwrap_or_default();
        error!("[TTS][http] API error {}: {}", status, body_text);
        return Err(AppError::TtsService(format!("百炼 TTS API 错误 {}: {}", status, body_text)));
    }

    let resp_body: serde_json::Value = response.json().await
        .map_err(|e| AppError::TtsService(format!("解析响应失败: {}", e)))?;

    debug!("[TTS][http] Response:\n{}", serde_json::to_string_pretty(&resp_body).unwrap_or_default());

    // Try audio URL first
    if let Some(url) = resp_body["output"]["audio"]["url"].as_str() {
        let url = url.replacen("http://", "https://", 1);
        info!("[TTS][http] Downloading audio from: {}", url);

        // Retry download up to 3 times (OSS can be flaky)
        let mut last_err = String::new();
        for attempt in 1..=3 {
            let dl_client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .map_err(|e| AppError::TtsService(format!("DL client error: {}", e)))?;

            match dl_client.get(&url).send().await {
                Ok(resp) if resp.status().is_success() => {
                    match resp.bytes().await {
                        Ok(b) => {
                            info!("[TTS][http] Downloaded {} bytes (attempt {})", b.len(), attempt);
                            return Ok(b.to_vec());
                        }
                        Err(e) => { last_err = format!("读取音频失败: {}", e); }
                    }
                }
                Ok(resp) => { last_err = format!("下载状态 {}", resp.status()); }
                Err(e) => { last_err = format!("下载失败: {}", e); }
            }
            if attempt < 3 {
                warn!("[TTS][http] Download attempt {} failed: {}, retrying...", attempt, last_err);
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            }
        }
        return Err(AppError::TtsService(format!("下载百炼 TTS 音频失败 (3次重试): {}", last_err)));
    }

    // Fallback: base64
    if let Some(data) = resp_body["output"]["audio"]["data"].as_str() {
        use base64::Engine;
        let bytes = base64::engine::general_purpose::STANDARD.decode(data)
            .map_err(|e| AppError::TtsService(format!("解码音频失败: {}", e)))?;
        info!("[TTS][http] Decoded base64 audio: {} bytes", bytes.len());
        return Ok(bytes);
    }

    Err(AppError::TtsService(format!("响应中未找到音频: {}", resp_body)))
}

/// Unified TTS call: routes to HTTP or WebSocket based on model.
/// If text exceeds TTS_CHUNK_MAX_CHARS, it is split into chunks.
/// For WS mode (Qwen TTS Realtime), all chunks are sent in a SINGLE session
/// to preserve tonal consistency — the model handles natural pauses.
async fn call_tts(
    text: &str,
    voice_config: &VoiceConfig,
    instructions: Option<&str>,
    api_key: &str,
    model: &str,
) -> Result<Vec<u8>, AppError> {
    let chunks = split_text_for_tts(text);
    if chunks.len() <= 1 {
        // Short text: direct call
        if is_http_model(model) {
            call_http_tts(text, voice_config, instructions, api_key, model).await
        } else {
            let mut ws = ws_realtime_connect(api_key, model).await?;
            ws_realtime_run_task(&mut ws, &[text], voice_config, instructions, model).await
        }
    } else {
        info!("[TTS][split] Text split into {} chunks (total {} chars)", chunks.len(), text.len());

        if is_http_model(model) {
            // HTTP: each chunk separate, merge with silence gaps
            let tmp_dir = std::env::temp_dir();
            let mut merged: Vec<(std::path::PathBuf, u32)> = Vec::new();
            for (i, chunk) in chunks.iter().enumerate() {
                let audio = call_http_tts(&chunk.text, voice_config, instructions, api_key, model).await?;
                let path = tmp_dir.join(format!("tts_chunk_{}.mp3", i));
                std::fs::write(&path, &audio)
                    .map_err(|e| AppError::FileSystem(format!("write chunk: {}", e)))?;
                merged.push((path, chunk.pause_ms));
            }
            let merged_path = tmp_dir.join("tts_merged.mp3");
            merge_audio_with_silence(&merged, &merged_path)
                .await
                .map_err(|e| AppError::TtsService(format!("merge audio: {}", e)))?;
            let audio = std::fs::read(&merged_path)
                .map_err(|e| AppError::FileSystem(format!("read merged: {}", e)))?;
            for (p, _) in &merged {
                let _ = std::fs::remove_file(p);
            }
            let _ = std::fs::remove_file(&merged_path);
            Ok(audio)
        } else {
            // WS (Qwen TTS Realtime): split into sessions of ≤9500 chars to stay under 10000 limit.
            // Prefer splitting at paragraph boundaries (higher pause_ms) for natural transitions.
            let session_groups = group_chunks_into_sessions(&chunks, 9500);
            info!("[TTS][split] {} sessions for {} chunks", session_groups.len(), chunks.len());

            let mut all_audio = Vec::<u8>::new();
            for (i, group) in session_groups.iter().enumerate() {
                let refs: Vec<&str> = group.iter().map(|c| c.text.as_str()).collect();
                let mut ws = ws_realtime_connect(api_key, model).await?;
                let part = ws_realtime_run_task(&mut ws, &refs, voice_config, instructions, model).await?;
                info!("[TTS][split] session {}/{}: {} bytes", i + 1, session_groups.len(), part.len());
                all_audio.extend_from_slice(&part);
            }
            Ok(all_audio)
        }
    }
}

/// Process a single line of text for batch TTS.
/// Creates a fresh WS connection per line for Qwen TTS Realtime.
async fn batch_tts_one(
    text: &str,
    vc: &VoiceConfig,
    instr: Option<&str>,
    model: &str,
    api_key: &str,
) -> Result<Vec<u8>, AppError> {
    let chunks = split_text_for_tts(text);
    if chunks.len() <= 1 {
        // Short text: direct call
        if is_http_model(model) {
            call_http_tts(text, vc, instr, api_key, model).await
        } else {
            let mut ws = ws_realtime_connect(api_key, model).await?;
            ws_realtime_run_task(&mut ws, &[text], vc, instr, model).await
        }
    } else {
        info!("[TTS][split][batch] Text split into {} chunks (total {} chars)", chunks.len(), text.len());

        if is_http_model(model) {
            // HTTP: each chunk separate, merge with silence gaps
            let tmp_dir = std::env::temp_dir();
            let mut merged: Vec<(std::path::PathBuf, u32)> = Vec::new();
            for (i, chunk) in chunks.iter().enumerate() {
                let audio = call_http_tts(&chunk.text, vc, instr, api_key, model).await?;
                let path = tmp_dir.join(format!("tts_chunk_{}.mp3", i));
                std::fs::write(&path, &audio)
                    .map_err(|e| AppError::FileSystem(format!("write chunk: {}", e)))?;
                merged.push((path, chunk.pause_ms));
            }
            let merged_path = tmp_dir.join("tts_merged.mp3");
            merge_audio_with_silence(&merged, &merged_path)
                .await
                .map_err(|e| AppError::TtsService(format!("merge audio: {}", e)))?;
            let audio = std::fs::read(&merged_path)
                .map_err(|e| AppError::FileSystem(format!("read merged: {}", e)))?;
            for (p, _) in &merged {
                let _ = std::fs::remove_file(p);
            }
            let _ = std::fs::remove_file(&merged_path);
            Ok(audio)
        } else {
            // WS (Qwen TTS Realtime): split into sessions of ≤9500 chars to stay under 10000 limit.
            let session_groups = group_chunks_into_sessions(&chunks, 9500);
            info!("[TTS][split][batch] {} sessions for {} chunks", session_groups.len(), chunks.len());

            let mut all_audio = Vec::<u8>::new();
            for (i, group) in session_groups.iter().enumerate() {
                let refs: Vec<&str> = group.iter().map(|c| c.text.as_str()).collect();
                let mut ws = ws_realtime_connect(api_key, model).await?;
                let part = ws_realtime_run_task(&mut ws, &refs, vc, instr, model).await?;
                info!("[TTS][split][batch] session {}/{}: {} bytes", i + 1, session_groups.len(), part.len());
                all_audio.extend_from_slice(&part);
            }
            Ok(all_audio)
        }
    }
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn generate_tts(
    app: tauri::AppHandle,
    db: tauri::State<'_, Mutex<Database>>,
    project_id: String,
    line_id: String,
    text: String,
    voice_config: VoiceConfig,
    instructions: Option<String>,
    api_key: String,
) -> Result<AudioFragment, AppError> {
    info!("[TTS] generate_tts: project={}, line={}, voice={}", project_id, line_id, voice_config.voice_name);

    let app_data_dir = app.path().app_data_dir()
        .map_err(|e| AppError::FileSystem(e.to_string()))?;
    let audio_path = build_audio_path(&app_data_dir, &project_id, &line_id);
    if let Some(p) = audio_path.parent() {
        std::fs::create_dir_all(p).map_err(|e| AppError::FileSystem(format!("mkdir: {}", e)))?;
    }

    let has_instr = instructions.as_ref().map(|i| !i.trim().is_empty()).unwrap_or(false);
    let model = resolve_model(&voice_config, has_instr);

    let audio_bytes = call_tts(
        &text,
        &voice_config,
        instructions.as_deref(),
        &api_key,
        &model,
    ).await?;

    info!("[TTS] Got {} bytes for line={}", audio_bytes.len(), line_id);

    std::fs::write(&audio_path, &audio_bytes)
        .map_err(|e| AppError::FileSystem(format!("write: {}", e)))?;
    reencode_with_ffmpeg(&audio_path, &line_id).await;

    let duration_ms = get_audio_duration(&audio_path).await;
    let fragment = AudioFragment {
        id: uuid::Uuid::new_v4().to_string(),
        project_id: project_id.clone(),
        line_id: line_id.clone(),
        file_path: audio_path.to_string_lossy().to_string(),
        duration_ms,
        source: "tts".to_string(),
    };

    let db = db.lock().map_err(|e| AppError::Database(e.to_string()))?;
    db.upsert_audio_fragment(&fragment)?;
    info!("[TTS] generate_tts done: line={}", line_id);
    Ok(fragment)
}

#[tauri::command]
pub async fn generate_all_tts(
    app: tauri::AppHandle,
    db: tauri::State<'_, Mutex<Database>>,
    project_id: String,
    api_key: String,
) -> Result<usize, AppError> {
    info!("[TTS] generate_all_tts: project={}", project_id);

    let (script_lines, fragments, characters) = {
        let db = db.lock().map_err(|e| AppError::Database(e.to_string()))?;
        let lines = db.load_script(&project_id)?;
        let frags = db.list_audio_fragments(&project_id)?;
        let chars = db.list_characters(&project_id)?;
        info!("[TTS] {} lines, {} existing, {} chars", lines.len(), frags.len(), chars.len());
        (lines, frags, chars)
    };

    let existing: std::collections::HashSet<String> =
        fragments.iter().map(|f| f.line_id.clone()).collect();

    let char_map: std::collections::HashMap<String, VoiceConfig> = characters
        .iter()
        .map(|c| (c.id.clone(), VoiceConfig {
            voice_name: c.voice_name.clone(),
            tts_model: c.tts_model.clone(),
            speed: c.speed,
            pitch: c.pitch,
        }))
        .collect();

    let default_vc = VoiceConfig { voice_name: String::new(), tts_model: String::new(), speed: 1.0, pitch: 1.0 };

    struct LineInfo { id: String, text: String, instructions: String, vc: VoiceConfig }

    let missing: Vec<LineInfo> = script_lines.iter()
        .filter(|l| !existing.contains(&l.id) && !l.text.trim().is_empty())
        .map(|l| {
            let vc = l.character_id.as_ref()
                .and_then(|cid| char_map.get(cid))
                .cloned()
                .unwrap_or_else(|| default_vc.clone());
            LineInfo { id: l.id.clone(), text: l.text.clone(), instructions: l.instructions.clone(), vc }
        })
        .collect();

    if missing.is_empty() {
        info!("[TTS] Nothing to generate");
        return Ok(0);
    }

    let total = missing.len();
    info!("[TTS] {} lines to generate", total);

    let app_data_dir = app.path().app_data_dir()
        .map_err(|e| AppError::FileSystem(e.to_string()))?;

    let mut success_count = 0usize;

    for line in &missing {
        let has_instr = !line.instructions.is_empty();
        let model = resolve_model(&line.vc, has_instr);
        let instr: Option<&str> = if has_instr { Some(&line.instructions) } else { None };

        info!("[TTS][batch] line={}, voice={}, model={}", line.id, line.vc.voice_name, model);

        let task_result = tokio::time::timeout(
            std::time::Duration::from_secs(600),
            batch_tts_one(&line.text, &line.vc, instr, &model, &api_key),
        ).await;

        let audio = match task_result {
            Ok(Ok(bytes)) => bytes,
            Ok(Err(e)) => {
                error!("[TTS][batch] line {} failed: {}", line.id, e);
                let _ = app.emit("tts-batch-progress", TtsBatchProgress {
                    current: missing.iter().position(|l| l.id == line.id).unwrap_or(0) + 1,
                    total,
                    line_id: line.id.clone(),
                    success: false, error: Some(e.to_string()),
                });
                continue;
            }
            Err(_) => {
                error!("[TTS][batch] line {} timed out", line.id);
                let _ = app.emit("tts-batch-progress", TtsBatchProgress {
                    current: missing.iter().position(|l| l.id == line.id).unwrap_or(0) + 1,
                    total,
                    line_id: line.id.clone(),
                    success: false, error: Some("Timeout".into()),
                });
                continue;
            }
        };

        let audio_path = build_audio_path(&app_data_dir, &project_id, &line.id);
        if let Some(p) = audio_path.parent() { let _ = std::fs::create_dir_all(p); }

        if let Err(e) = std::fs::write(&audio_path, &audio) {
            error!("[TTS][batch] write failed {}: {}", line.id, e);
            let _ = app.emit("tts-batch-progress", TtsBatchProgress {
                current: missing.iter().position(|l| l.id == line.id).unwrap_or(0) + 1,
                total,
                line_id: line.id.clone(),
                success: false, error: Some(format!("Write: {}", e)),
            });
            continue;
        }

        reencode_with_ffmpeg(&audio_path, &line.id).await;
        let duration_ms = get_audio_duration(&audio_path).await;

        let fragment = AudioFragment {
            id: uuid::Uuid::new_v4().to_string(),
            project_id: project_id.clone(),
            line_id: line.id.clone(),
            file_path: audio_path.to_string_lossy().to_string(),
            duration_ms,
            source: "tts".to_string(),
        };

        let db_result = {
            let db = db.lock().map_err(|e| AppError::Database(e.to_string()))?;
            db.upsert_audio_fragment(&fragment)
        };

        let completed = missing.iter().position(|l| l.id == line.id).unwrap_or(0) + 1;
        match db_result {
            Ok(()) => {
                success_count += 1;
                info!("[TTS][batch] line {} ok ({} bytes)", line.id, audio.len());
                let _ = app.emit("tts-batch-progress", TtsBatchProgress {
                    current: completed, total,
                    line_id: line.id.clone(),
                    success: true, error: None,
                });
            }
            Err(e) => {
                error!("[TTS][batch] DB error {}: {}", line.id, e);
                let _ = app.emit("tts-batch-progress", TtsBatchProgress {
                    current: completed, total,
                    line_id: line.id.clone(),
                    success: false, error: Some(format!("DB: {}", e)),
                });
            }
        }
    }

    info!("[TTS] generate_all_tts done: {}/{}", success_count, total);
    Ok(success_count)
}

#[tauri::command]
pub async fn clear_audio_fragments(
    _app: tauri::AppHandle,
    db: tauri::State<'_, Mutex<Database>>,
    project_id: String,
) -> Result<(), AppError> {
    info!("[TTS] clear_audio_fragments: project={}", project_id);
    let paths = {
        let db = db.lock().map_err(|e| AppError::Database(e.to_string()))?;
        db.clear_audio_fragments(&project_id)?
    };
    // Delete audio files from disk
    for path in &paths {
        if let Err(e) = std::fs::remove_file(path) {
            warn!("[TTS] Failed to delete audio file {}: {}", path, e);
        }
    }
    info!("[TTS] Cleared {} audio fragments", paths.len());
    // Reload project in frontend by re-fetching
    Ok(())
}

#[tauri::command]
pub async fn clear_tts_fragments(
    _app: tauri::AppHandle,
    db: tauri::State<'_, Mutex<Database>>,
    project_id: String,
) -> Result<(), AppError> {
    info!("[TTS] clear_tts_fragments: project={}", project_id);
    let paths = {
        let db = db.lock().map_err(|e| AppError::Database(e.to_string()))?;
        db.clear_tts_fragments(&project_id)?
    };
    // Delete audio files from disk
    for path in &paths {
        if let Err(e) = std::fs::remove_file(path) {
            warn!("[TTS] Failed to delete audio file {}: {}", path, e);
        }
    }
    info!("[TTS] Cleared {} TTS audio fragments", paths.len());
    Ok(())
}

// ---------------------------------------------------------------------------
// Voice Cloning (声音复刻)
// ---------------------------------------------------------------------------

/// Response from the voice enrollment API.
#[derive(Debug, serde::Deserialize)]
struct VoiceEnrollmentOutput {
    output: VoiceEnrollmentResult,
}

#[derive(Debug, serde::Deserialize)]
struct VoiceEnrollmentResult {
    voice: String,
}

/// Create a cloned voice from uploaded/recorded audio via DashScope voice enrollment API.
#[tauri::command]
pub async fn create_voice(
    app: tauri::AppHandle,
    project_id: String,
    audio_data_base64: String,
    preferred_name: String,
    target_model: String,
) -> Result<String, AppError> {
    use base64::{Engine as _, engine::general_purpose::STANDARD};

    info!(
        "[VoiceClone] create_voice: project={}, name={}, model={}",
        project_id, preferred_name, target_model
    );

    let api_key = {
        let config = crate::core::config::ConfigManager::new(app.clone());
        config
            .load_api_key("dashscope")
            .map_err(|e| AppError::Config(format!("Failed to load API key: {}", e)))?
            .ok_or_else(|| AppError::Config("DashScope API key not configured".into()))?
    };

    // Decode base64 audio
    let audio_bytes = STANDARD
        .decode(&audio_data_base64)
        .map_err(|e| AppError::FileSystem(format!("base64 decode: {}", e)))?;

    // Re-encode as data URI (the audio can be any common format: mp3, wav, webm)
    let data_uri = format!("data:audio/mpeg;base64,{}", STANDARD.encode(&audio_bytes));

    let url = "https://dashscope.aliyuncs.com/api/v1/services/audio/tts/customization";
    let payload = serde_json::json!({
        "model": "qwen-voice-enrollment",
        "input": {
            "action": "create",
            "target_model": target_model,
            "preferred_name": preferred_name,
            "audio": { "data": data_uri }
        }
    });

    let client = reqwest::Client::new();
    let resp = client
        .post(url)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&payload)
        .send()
        .await
        .map_err(|e| AppError::TtsService(format!("HTTP request failed: {}", e)))?;

    let status = resp.status();
    let body = resp
        .text()
        .await
        .map_err(|e| AppError::TtsService(format!("Read response body failed: {}", e)))?;

    if !status.is_success() {
        return Err(AppError::TtsService(format!(
            "Voice enrollment failed ({}): {}",
            status, body
        )));
    }

    let output: VoiceEnrollmentOutput = serde_json::from_str(&body)
        .map_err(|e| AppError::TtsService(format!("Parse response failed: {}, body: {}", e, body)))?;

    info!("[VoiceClone] Voice created: {}", output.output.voice);
    Ok(output.output.voice)
}

/// Preview a cloned voice by synthesizing a short test sentence via WS realtime.
/// Returns the path to the generated preview audio file.
#[tauri::command]
pub async fn preview_voice(
    app: tauri::AppHandle,
    project_id: String,
    voice: String,
    target_model: String,
) -> Result<String, AppError> {
    info!(
        "[VoiceClone] preview_voice: project={}, voice={}, model={}",
        project_id, voice, target_model
    );

    let api_key = {
        let config = crate::core::config::ConfigManager::new(app.clone());
        config
            .load_api_key("dashscope")
            .map_err(|e| AppError::Config(format!("Failed to load API key: {}", e)))?
            .ok_or_else(|| AppError::Config("DashScope API key not configured".into()))?
    };

    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::FileSystem(e.to_string()))?;

    let preview_dir = app_data_dir
        .join("projects")
        .join(&project_id)
        .join("previews");
    std::fs::create_dir_all(&preview_dir)
        .map_err(|e| AppError::FileSystem(format!("mkdir previews: {}", e)))?;

    let preview_id = uuid::Uuid::new_v4().to_string();
    let file_path = preview_dir
        .join(format!("{}.mp3", &preview_id))
        .to_string_lossy()
        .to_string();

    // Use WS realtime to synthesize a short preview sentence
    let voice_config = VoiceConfig {
        voice_name: voice,
        tts_model: target_model.clone(),
        speed: 1.0,
        pitch: 1.0,
    };

    let preview_text = "你好，这是我的专属声音";
    let audio_bytes = {
        let ws = ws_realtime_connect(&api_key, &target_model).await?;
        let mut ws = ws;
        ws_realtime_run_task(
            &mut ws,
            &[preview_text],
            &voice_config,
            None,
            &target_model,
        )
        .await?
    };

    std::fs::write(&file_path, &audio_bytes)
        .map_err(|e| AppError::FileSystem(format!("write preview: {}", e)))?;

    info!("[VoiceClone] Preview audio saved: {}", file_path);
    Ok(file_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_audio_path() {
        let p = build_audio_path(std::path::Path::new("/data"), "proj-1", "line-42");
        assert_eq!(p, std::path::PathBuf::from("/data/projects/proj-1/audio/line-42.mp3"));
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
            assert!(c.text.len() <= TTS_CHUNK_MAX_CHARS, "chunk len={} exceeds {}", c.text.len(), TTS_CHUNK_MAX_CHARS);
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
        let text = "他今天去了公园。天气很好。很多人在跑步。小鸟在树上唱歌。孩子们很开心。".repeat(8); // 37 * 8 = 296 chars — still short
        let chunks = split_text_for_tts(&text);
        assert!(chunks.len() >= 1);
        for c in &chunks {
            assert!(c.text.chars().count() <= TTS_CHUNK_MAX_CHARS, "chunk len={} exceeds {}", c.text.chars().count(), TTS_CHUNK_MAX_CHARS);
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
            assert!(c.text.chars().count() <= TTS_CHUNK_MAX_CHARS, "chunk len={} exceeds {}", c.text.chars().count(), TTS_CHUNK_MAX_CHARS);
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
        let text = "第一句。第二句。第三句。第四句。第五句。第六句。第七句。第八句。第九句。第十句。";
        let chunks = split_text_for_tts(text);
        // Check that non-final chunks end with sentence-ending punctuation
        let boundaries: &[char] = &['.', '!', '?', '。', '！', '？', '；', ';'];
        for (i, c) in chunks.iter().enumerate() {
            if i < chunks.len() - 1 {
                let last_char = c.text.chars().last().unwrap();
                assert!(boundaries.contains(&last_char),
                    "chunk {} ends with '{}' not a sentence boundary", i, last_char);
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
