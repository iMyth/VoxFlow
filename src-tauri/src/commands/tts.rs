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

// ---------------------------------------------------------------------------
// WebSocket CosyVoice TTS
// ---------------------------------------------------------------------------
// Uses wss://dashscope.aliyuncs.com/api-ws/v1/inference
// Protocol: run-task → task-started → continue-task(s) → finish-task → audio → task-finished
//
// Advantages over the old HTTP approach:
// 1. Audio is streamed directly over WebSocket — no OSS download that can timeout
// 2. Multiple continue-task messages in one session maintain tonal context
// 3. Connection reuse: after task-finished, send new run-task on same connection
// ---------------------------------------------------------------------------

/// Connect to the DashScope WebSocket TTS endpoint.
async fn ws_connect(
    api_key: &str,
) -> Result<
    tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    AppError,
> {
    let url = "wss://dashscope.aliyuncs.com/api-ws/v1/inference";
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

    info!("[TTS][ws] Connected");
    Ok(ws)
}

/// Run a single TTS task on an existing WebSocket connection.
/// Sends run-task → continue-task(texts) → finish-task, collects binary audio.
/// After task-finished, the connection can be reused for another task.
async fn ws_run_task<S>(
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

    let task_id = uuid::Uuid::new_v4().to_string();

    let has_instr = instructions.map(|i| !i.trim().is_empty()).unwrap_or(false);

    let mut params = serde_json::Map::new();
    params.insert("text_type".into(), json!("PlainText"));
    params.insert("voice".into(), json!(voice_config.voice_name));
    params.insert("format".into(), json!("mp3"));
    params.insert("sample_rate".into(), json!(22050));
    params.insert("volume".into(), json!(50));
    params.insert("rate".into(), json!(voice_config.speed));
    params.insert("pitch".into(), json!(voice_config.pitch));
    if has_instr {
        params.insert("instruction".into(), json!(instructions.unwrap()));
    }

    let run_task = json!({
        "header": { "action": "run-task", "task_id": task_id, "streaming": "duplex" },
        "payload": {
            "task_group": "audio",
            "task": "tts",
            "function": "SpeechSynthesizer",
            "model": model,
            "parameters": params,
            "input": {}
        }
    });

    debug!("[TTS][ws] run-task: model={}, voice={}, texts={}", model, voice_config.voice_name, texts.len());
    ws.send(Message::Text(run_task.to_string()))
        .await
        .map_err(|e| AppError::TtsService(format!("send run-task: {}", e)))?;

    let mut audio = Vec::<u8>::new();
    let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_secs(120);

    loop {
        let msg = tokio::time::timeout_at(deadline, ws.next()).await;
        match msg {
            Err(_) => return Err(AppError::TtsService("WS task timed out (120s)".into())),
            Ok(None) => return Err(AppError::TtsService("WS closed unexpectedly".into())),
            Ok(Some(Err(e))) => return Err(AppError::TtsService(format!("WS error: {}", e))),
            Ok(Some(Ok(Message::Binary(data)))) => {
                audio.extend_from_slice(&data);
            }
            Ok(Some(Ok(Message::Text(t)))) => {
                if let Ok(evt) = serde_json::from_str::<serde_json::Value>(&t) {
                    let event = evt["header"]["event"].as_str().unwrap_or("");
                    match event {
                        "task-started" => {
                            // Send all texts
                            for text in texts {
                                let ct = json!({
                                    "header": { "action": "continue-task", "task_id": task_id, "streaming": "duplex" },
                                    "payload": { "input": { "text": text } }
                                });
                                ws.send(Message::Text(ct.to_string()))
                                    .await
                                    .map_err(|e| AppError::TtsService(format!("send continue-task: {}", e)))?;
                            }
                            // Send finish-task
                            let ft = json!({
                                "header": { "action": "finish-task", "task_id": task_id, "streaming": "duplex" },
                                "payload": { "input": {} }
                            });
                            ws.send(Message::Text(ft.to_string()))
                                .await
                                .map_err(|e| AppError::TtsService(format!("send finish-task: {}", e)))?;
                        }
                        "task-finished" => {
                            info!("[TTS][ws] task-finished: {} bytes", audio.len());
                            break;
                        }
                        "task-failed" => {
                            let code = evt["header"]["error_code"].as_str().unwrap_or("");
                            let msg = evt["header"]["error_message"].as_str().unwrap_or("");
                            error!("[TTS][ws] task-failed: {} - {}", code, msg);
                            return Err(AppError::TtsService(format!("TTS failed: {} - {}", code, msg)));
                        }
                        _ => {}
                    }
                }
            }
            Ok(Some(Ok(_))) => {} // ping/pong
        }
    }

    if audio.is_empty() {
        return Err(AppError::TtsService("No audio received".into()));
    }
    Ok(audio)
}

/// Re-encode audio with FFmpeg to fix VBR headers.
fn reencode_with_ffmpeg(audio_path: &std::path::Path, label: &str) {
    let tmp = audio_path.with_extension("tmp.mp3");
    let r = std::process::Command::new("ffmpeg")
        .args(["-y", "-i", &audio_path.to_string_lossy(), "-codec:a", "libmp3lame", "-b:a", "192k", &tmp.to_string_lossy()])
        .stderr(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .output();
    match r {
        Ok(o) if o.status.success() => { let _ = std::fs::rename(&tmp, audio_path); }
        _ => { let _ = std::fs::remove_file(&tmp); warn!("[TTS] FFmpeg failed for {}", label); }
    }
}

/// Get audio duration in ms via FFprobe, fallback to rodio.
fn get_audio_duration(path: &std::path::Path) -> Option<i64> {
    if let Ok(o) = std::process::Command::new("ffprobe")
        .args(["-v", "error", "-show_entries", "format=duration", "-of", "default=noprint_wrappers=1:nokey=1", &path.to_string_lossy()])
        .output()
    {
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

/// Determine model name. For CosyVoice WS, use the model from config.
fn resolve_model(voice_config: &VoiceConfig, has_instructions: bool) -> String {
    if has_instructions {
        if voice_config.tts_model.starts_with("qwen") {
            return "qwen3-tts-instruct-flash".to_string();
        }
    }
    if voice_config.tts_model.is_empty() {
        "cosyvoice-v3-flash".to_string()
    } else {
        voice_config.tts_model.clone()
    }
}

/// Returns true if the model should use the HTTP REST API (qwen-tts series),
/// false if it should use the CosyVoice WebSocket endpoint.
fn is_http_model(model: &str) -> bool {
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
async fn call_tts(
    text: &str,
    voice_config: &VoiceConfig,
    instructions: Option<&str>,
    api_key: &str,
    model: &str,
) -> Result<Vec<u8>, AppError> {
    if is_http_model(model) {
        call_http_tts(text, voice_config, instructions, api_key, model).await
    } else {
        let mut ws = ws_connect(api_key).await?;
        ws_run_task(&mut ws, &[text], voice_config, instructions, model).await
    }
}

/// Unified batch TTS: for WS models reuses connection, for HTTP models calls sequentially.
enum BatchConn {
    Ws(tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>),
    Http,
}

async fn batch_tts_one(
    conn: &mut BatchConn,
    text: &str,
    vc: &VoiceConfig,
    instr: Option<&str>,
    model: &str,
    api_key: &str,
) -> Result<Vec<u8>, AppError> {
    match conn {
        BatchConn::Http => {
            call_http_tts(text, vc, instr, api_key, model).await
        }
        BatchConn::Ws(ws) => {
            ws_run_task(ws, &[text], vc, instr, model).await
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
    reencode_with_ffmpeg(&audio_path, &line_id);

    let duration_ms = get_audio_duration(&audio_path);
    let fragment = AudioFragment {
        id: uuid::Uuid::new_v4().to_string(),
        project_id: project_id.clone(),
        line_id: line_id.clone(),
        file_path: audio_path.to_string_lossy().to_string(),
        duration_ms,
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

    // Group by voice config key so we can reuse WS connections per voice.
    // Within each group, lines are processed sequentially on the same connection.
    // The CosyVoice WS supports connection reuse: after task-finished, send new run-task.
    use std::collections::BTreeMap;
    type VKey = (String, String, i32, i32, String); // voice, model, speed*100, pitch*100, instructions

    let mut groups: BTreeMap<VKey, Vec<usize>> = BTreeMap::new();
    for (i, line) in missing.iter().enumerate() {
        let key = (
            line.vc.voice_name.clone(),
            line.vc.tts_model.clone(),
            (line.vc.speed * 100.0) as i32,
            (line.vc.pitch * 100.0) as i32,
            line.instructions.clone(),
        );
        groups.entry(key).or_default().push(i);
    }

    info!("[TTS] {} voice groups", groups.len());

    let mut success_count = 0usize;
    let mut completed = 0usize;

    for (_key, indices) in &groups {
        // Open one WS connection per voice group
        let first = &missing[indices[0]];
        let has_instr = !first.instructions.is_empty();
        let model = resolve_model(&first.vc, has_instr);
        let instr: Option<&str> = if has_instr { Some(&first.instructions) } else { None };

        info!("[TTS][group] voice={}, model={}, lines={}", first.vc.voice_name, model, indices.len());

        let mut conn = if is_http_model(&model) {
            BatchConn::Http
        } else {
            match ws_connect(&api_key).await {
                Ok(ws) => BatchConn::Ws(ws),
                Err(e) => {
                    error!("[TTS][group] WS connect failed: {}", e);
                    for &idx in indices {
                        completed += 1;
                        let _ = app.emit("tts-batch-progress", TtsBatchProgress {
                            current: completed, total,
                            line_id: missing[idx].id.clone(),
                            success: false, error: Some(e.to_string()),
                        });
                    }
                    continue;
                }
            }
        };

        // Process each line as a separate task on the same connection.
        // This reuses the connection (avoiding reconnect overhead) while
        // producing individual audio files per line.
        for &idx in indices {
            let line = &missing[idx];
            completed += 1;

            let task_result = tokio::time::timeout(
                std::time::Duration::from_secs(90),
                batch_tts_one(&mut conn, &line.text, &line.vc, instr, &model, &api_key),
            ).await;

            let audio = match task_result {
                Ok(Ok(bytes)) => bytes,
                Ok(Err(e)) => {
                    error!("[TTS][batch] line {} failed: {}", line.id, e);
                    let _ = app.emit("tts-batch-progress", TtsBatchProgress {
                        current: completed, total,
                        line_id: line.id.clone(),
                        success: false, error: Some(e.to_string()),
                    });
                    // For WS: connection may be broken after task-failed, reconnect
                    if let BatchConn::Ws(_) = &conn {
                        if let Ok(new_ws) = ws_connect(&api_key).await {
                            conn = BatchConn::Ws(new_ws);
                        }
                    }
                    continue;
                }
                Err(_) => {
                    error!("[TTS][batch] line {} timed out", line.id);
                    let _ = app.emit("tts-batch-progress", TtsBatchProgress {
                        current: completed, total,
                        line_id: line.id.clone(),
                        success: false, error: Some("Timeout".into()),
                    });
                    if let BatchConn::Ws(_) = &conn {
                        if let Ok(new_ws) = ws_connect(&api_key).await {
                            conn = BatchConn::Ws(new_ws);
                        }
                    }
                    continue;
                }
            };

            let audio_path = build_audio_path(&app_data_dir, &project_id, &line.id);
            if let Some(p) = audio_path.parent() { let _ = std::fs::create_dir_all(p); }

            if let Err(e) = std::fs::write(&audio_path, &audio) {
                error!("[TTS][batch] write failed {}: {}", line.id, e);
                let _ = app.emit("tts-batch-progress", TtsBatchProgress {
                    current: completed, total,
                    line_id: line.id.clone(),
                    success: false, error: Some(format!("Write: {}", e)),
                });
                continue;
            }

            reencode_with_ffmpeg(&audio_path, &line.id);
            let duration_ms = get_audio_duration(&audio_path);

            let fragment = AudioFragment {
                id: uuid::Uuid::new_v4().to_string(),
                project_id: project_id.clone(),
                line_id: line.id.clone(),
                file_path: audio_path.to_string_lossy().to_string(),
                duration_ms,
            };

            let db_result = {
                let db = db.lock().map_err(|e| AppError::Database(e.to_string()))?;
                db.upsert_audio_fragment(&fragment)
            };

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_audio_path() {
        let p = build_audio_path(std::path::Path::new("/data"), "proj-1", "line-42");
        assert_eq!(p, std::path::PathBuf::from("/data/projects/proj-1/audio/line-42.mp3"));
    }
}
