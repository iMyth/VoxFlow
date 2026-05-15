use crate::core::error::AppError;
use crate::core::models::VoiceConfig;
use log::info;

use super::http::call_http_tts;
use super::utils::{group_chunks_into_sessions, is_http_model, split_text_for_tts};
use super::websocket::{ws_realtime_connect, ws_realtime_run_task};

/// Unified TTS call: routes to HTTP or WebSocket based on model.
/// If text exceeds TTS_CHUNK_MAX_CHARS, it is split into chunks.
/// For WS mode (Qwen TTS Realtime), all chunks are sent in a SINGLE session
/// to preserve tonal consistency — the model handles natural pauses.
pub(crate) async fn call_tts(
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
        info!(
            "[TTS][split] Text split into {} chunks (total {} chars)",
            chunks.len(),
            text.len()
        );

        if is_http_model(model) {
            // HTTP: each chunk separate, merge with silence gaps
            let tmp_dir = std::env::temp_dir();
            let session_id = uuid::Uuid::new_v4().to_string();
            let mut merged: Vec<(std::path::PathBuf, u32)> = Vec::new();
            for (i, chunk) in chunks.iter().enumerate() {
                // Retry each chunk up to 3 times with backoff
                let mut last_err = String::new();
                let mut audio_bytes: Option<Vec<u8>> = None;
                for attempt in 1..=3 {
                    match call_http_tts(&chunk.text, voice_config, instructions, api_key, model)
                        .await
                    {
                        Ok(bytes) => {
                            audio_bytes = Some(bytes);
                            break;
                        }
                        Err(e) => {
                            last_err = e.to_string();
                            log::warn!(
                                "[TTS][split] chunk {}/{} attempt {} failed: {}",
                                i + 1,
                                chunks.len(),
                                attempt,
                                last_err
                            );
                            if attempt < 3 {
                                tokio::time::sleep(std::time::Duration::from_secs(
                                    2 * attempt as u64,
                                ))
                                .await;
                            }
                        }
                    }
                }
                let audio = audio_bytes.ok_or_else(|| {
                    for (p, _) in &merged {
                        let _ = std::fs::remove_file(p);
                    }
                    AppError::TtsService(format!(
                        "chunk {}/{} failed after 3 retries: {}",
                        i + 1,
                        chunks.len(),
                        last_err
                    ))
                })?;
                let path = tmp_dir.join(format!("tts_chunk_{}_{}.mp3", session_id, i));
                std::fs::write(&path, &audio)
                    .map_err(|e| AppError::FileSystem(format!("write chunk: {}", e)))?;
                merged.push((path, chunk.pause_ms));

                // Small delay between chunks to avoid rate limiting
                if i < chunks.len() - 1 {
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                }
            }
            let merged_path = tmp_dir.join(format!("tts_merged_{}.mp3", session_id));
            let merge_result = super::utils::merge_audio_with_silence(&merged, &merged_path).await;
            if let Err(e) = merge_result {
                for (p, _) in &merged {
                    let _ = std::fs::remove_file(p);
                }
                return Err(AppError::TtsService(format!("merge audio: {}", e)));
            }
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
            info!(
                "[TTS][split] {} sessions for {} chunks",
                session_groups.len(),
                chunks.len()
            );

            let mut all_audio = Vec::<u8>::new();
            for (i, group) in session_groups.iter().enumerate() {
                let refs: Vec<&str> = group.iter().map(|c| c.text.as_str()).collect();
                let mut ws = ws_realtime_connect(api_key, model).await?;
                let part =
                    ws_realtime_run_task(&mut ws, &refs, voice_config, instructions, model).await?;
                info!(
                    "[TTS][split] session {}/{}: {} bytes",
                    i + 1,
                    session_groups.len(),
                    part.len()
                );
                all_audio.extend_from_slice(&part);
            }
            Ok(all_audio)
        }
    }
}

/// Process a single line of text for batch TTS.
/// Creates a fresh WS connection per line for Qwen TTS Realtime.
pub(crate) async fn batch_tts_one(
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
        info!(
            "[TTS][split][batch] Text split into {} chunks (total {} chars)",
            chunks.len(),
            text.len()
        );

        if is_http_model(model) {
            // HTTP: each chunk separate, merge with silence gaps
            let tmp_dir = std::env::temp_dir();
            let session_id = uuid::Uuid::new_v4().to_string();
            let mut merged: Vec<(std::path::PathBuf, u32)> = Vec::new();
            for (i, chunk) in chunks.iter().enumerate() {
                // Retry each chunk up to 3 times with backoff
                let mut last_err = String::new();
                let mut audio_bytes: Option<Vec<u8>> = None;
                for attempt in 1..=3 {
                    match call_http_tts(&chunk.text, vc, instr, api_key, model).await {
                        Ok(bytes) => {
                            audio_bytes = Some(bytes);
                            break;
                        }
                        Err(e) => {
                            last_err = e.to_string();
                            log::warn!(
                                "[TTS][split][batch] chunk {}/{} attempt {} failed: {}",
                                i + 1,
                                chunks.len(),
                                attempt,
                                last_err
                            );
                            if attempt < 3 {
                                tokio::time::sleep(std::time::Duration::from_secs(
                                    2 * attempt as u64,
                                ))
                                .await;
                            }
                        }
                    }
                }
                let audio = audio_bytes.ok_or_else(|| {
                    // Cleanup any previously written chunk files
                    for (p, _) in &merged {
                        let _ = std::fs::remove_file(p);
                    }
                    AppError::TtsService(format!(
                        "chunk {}/{} failed after 3 retries: {}",
                        i + 1,
                        chunks.len(),
                        last_err
                    ))
                })?;
                let path = tmp_dir.join(format!("tts_chunk_{}_{}.mp3", session_id, i));
                std::fs::write(&path, &audio)
                    .map_err(|e| AppError::FileSystem(format!("write chunk: {}", e)))?;
                merged.push((path, chunk.pause_ms));

                // Small delay between chunks to avoid rate limiting
                if i < chunks.len() - 1 {
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                }
            }
            let merged_path = tmp_dir.join(format!("tts_merged_{}.mp3", session_id));
            let merge_result = super::utils::merge_audio_with_silence(&merged, &merged_path).await;
            if let Err(e) = merge_result {
                // Cleanup chunk files on merge failure
                for (p, _) in &merged {
                    let _ = std::fs::remove_file(p);
                }
                return Err(AppError::TtsService(format!("merge audio: {}", e)));
            }
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
            info!(
                "[TTS][split][batch] {} sessions for {} chunks",
                session_groups.len(),
                chunks.len()
            );

            let mut all_audio = Vec::<u8>::new();
            for (i, group) in session_groups.iter().enumerate() {
                let refs: Vec<&str> = group.iter().map(|c| c.text.as_str()).collect();
                let mut ws = ws_realtime_connect(api_key, model).await?;
                let part = ws_realtime_run_task(&mut ws, &refs, vc, instr, model).await?;
                info!(
                    "[TTS][split][batch] session {}/{}: {} bytes",
                    i + 1,
                    session_groups.len(),
                    part.len()
                );
                all_audio.extend_from_slice(&part);
            }
            Ok(all_audio)
        }
    }
}
