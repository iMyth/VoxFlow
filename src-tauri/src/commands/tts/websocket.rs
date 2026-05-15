use crate::core::error::AppError;
use crate::core::models::VoiceConfig;
use futures_util::{SinkExt, StreamExt};
use log::{debug, error, info, warn};
use tokio_tungstenite::tungstenite::Message;

// ---------------------------------------------------------------------------
// Qwen TTS Realtime WebSocket Protocol
// ---------------------------------------------------------------------------
// Uses wss://dashscope.aliyuncs.com/api-ws/v1/inference
// Protocol: session.update → text.input(s) → text.complete → audio.delta(s) → audio.completed
// ---------------------------------------------------------------------------

/// Connect to the DashScope WebSocket endpoint for Qwen TTS Realtime.
pub(crate) async fn ws_realtime_connect(
    api_key: &str,
    model: &str,
) -> Result<
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
    AppError,
> {
    let url = format!(
        "wss://dashscope.aliyuncs.com/api-ws/v1/realtime?model={}",
        model
    );
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
pub(crate) async fn ws_realtime_run_task<S>(
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
    info!(
        "[TTS][ws-realtime] model={}, voice={}, texts={}",
        model,
        voice_config.voice_name,
        texts.len()
    );

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

    debug!(
        "[TTS][ws-realtime] session.update: {}",
        serde_json::to_string(&session_update).unwrap_or_default()
    );
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
                info!(
                    "[TTS][ws-realtime] no more responses, finishing: {} bytes",
                    audio.len()
                );
                break;
            }
            Err(_) => return Err(AppError::TtsService("WS task timed out (600s)".into())),
            Ok(None) => {
                // Connection closed — if we have audio, treat as success
                if !audio.is_empty() {
                    info!(
                        "[TTS][ws-realtime] WS closed, returning {} bytes",
                        audio.len()
                    );
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
                                if let Ok(decoded) =
                                    base64::engine::general_purpose::STANDARD.decode(delta)
                                {
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
                                idle_deadline = Some(
                                    tokio::time::Instant::now()
                                        + tokio::time::Duration::from_secs(5),
                                );
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
                            return Err(AppError::TtsService(format!(
                                "TTS error: {} - {}",
                                code, msg
                            )));
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
