use std::sync::Mutex;

use tauri::Emitter;
use tauri::Manager;

use crate::core::db::Database;
use crate::core::error::AppError;
use crate::core::models::{AudioFragment, TtsBatchProgress, VoiceConfig};
use log::debug;

/// Build the audio file path for a given project and line.
/// Returns `{app_data_dir}/projects/{project_id}/audio/{line_id}.mp3`
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
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::FileSystem(e.to_string()))?;

    let audio_path = build_audio_path(&app_data_dir, &project_id, &line_id);

    if let Some(parent) = audio_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            AppError::FileSystem(format!("Failed to create audio directory: {}", e))
        })?;
    }

    let audio_bytes =
        call_dashscope_tts(&text, &voice_config, instructions.as_deref(), &api_key).await?;

    std::fs::write(&audio_path, &audio_bytes).map_err(|e| {
        AppError::FileSystem(format!("Failed to write audio file: {}", e))
    })?;

    // Re-encode with FFmpeg to fix VBR headers and ensure valid MP3 format.
    // TTS services may return WAV disguised as .mp3 or VBR MP3 with bad headers.
    let tmp_path = audio_path.with_extension("tmp.mp3");
    let ffmpeg_result = std::process::Command::new("ffmpeg")
        .args([
            "-y",
            "-i", &audio_path.to_string_lossy(),
            "-codec:a", "libmp3lame",
            "-b:a", "192k",
            &tmp_path.to_string_lossy(),
        ])
        .stderr(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .output();

    match ffmpeg_result {
        Ok(output) if output.status.success() => {
            // Replace original with re-encoded file
            std::fs::rename(&tmp_path, &audio_path).map_err(|e| {
                AppError::FileSystem(format!("Failed to replace audio file: {}", e))
            })?;
        }
        _ => {
            // FFmpeg failed or not found — keep original file, clean up tmp
            let _ = std::fs::remove_file(&tmp_path);
        }
    }

    let file_path = audio_path.to_string_lossy().to_string();
    let duration_ms = get_audio_duration(&audio_path);
    let fragment = AudioFragment {
        id: uuid::Uuid::new_v4().to_string(),
        project_id: project_id.clone(),
        line_id: line_id.clone(),
        file_path,
        duration_ms,
    };

    let db = db.lock().map_err(|e| AppError::Database(e.to_string()))?;
    db.upsert_audio_fragment(&fragment)?;

    Ok(fragment)
}

/// Get audio duration in milliseconds using FFprobe or rodio.
/// Returns None if duration cannot be determined.
fn get_audio_duration(path: &std::path::Path) -> Option<i64> {
    // Try FFprobe first (most reliable)
    if let Ok(output) = std::process::Command::new("ffprobe")
        .args([
            "-v", "error",
            "-show_entries", "format=duration",
            "-of", "default=noprint_wrappers=1:nokey=1",
            &path.to_string_lossy(),
        ])
        .output()
    {
        if output.status.success() {
            if let Ok(duration_str) = String::from_utf8(output.stdout) {
                if let Ok(seconds) = duration_str.trim().parse::<f64>() {
                    return Some((seconds * 1000.0).round() as i64);
                }
            }
        }
    }

    // Fallback: try rodio decoder
    if let Ok(file) = std::fs::File::open(path) {
        let reader = std::io::BufReader::new(file);
        use rodio::Source;
        if let Ok(decoder) = rodio::Decoder::new(reader) {
            let sample_rate = decoder.sample_rate() as f64;
            let total_samples = decoder.count();
            if sample_rate > 0.0 {
                return Some((total_samples as f64 / sample_rate * 1000.0).round() as i64);
            }
        }
    }

    None
}

/// 调用阿里百炼 (DashScope) Qwen-TTS / CosyVoice 服务生成音频。
///
/// API 端点（北京地域）：
/// POST https://dashscope.aliyuncs.com/api/v1/services/aigc/multimodal-generation/generation
///
/// voice_config.tts_model 指定模型名（如 "qwen3-tts-flash"、"cosyvoice-v3-flash"）
/// voice_config.voice_name 指定音色（如 "Cherry"、"longanyang"）
/// instructions 可选的导演指令，用于控制情绪、语速、语调等。如果提供，会自动切换到
/// qwen3-tts-instruct-flash 模型。
async fn call_dashscope_tts(
    text: &str,
    voice_config: &VoiceConfig,
    instructions: Option<&str>,
    api_key: &str,
) -> Result<Vec<u8>, AppError> {
    use reqwest::header::CONTENT_TYPE;
    use serde_json::json;

    let has_instructions = instructions.map(|i| !i.trim().is_empty()).unwrap_or(false);

    let model = if has_instructions {
        "qwen3-tts-instruct-flash"
    } else if voice_config.tts_model.is_empty() {
        "qwen3-tts-flash"
    } else {
        &voice_config.tts_model
    };

    let mut input = serde_json::Map::new();
    input.insert("text".to_string(), json!(text));
    input.insert("voice".to_string(), json!(voice_config.voice_name));
    if has_instructions {
        input.insert("instructions".to_string(), json!(instructions.unwrap()));
        input.insert("optimize_instructions".to_string(), json!(true));
    }

    let body = json!({
        "model": model,
        "input": input
    });

    // 使用 debug! 宏，只有在设置 RUST_LOG=debug 时才会显示，避免生产环境日志过多
    debug!("🚀 正在构造 TTS 请求:");
    // serde_json::to_string_pretty 可以格式化输出 JSON，方便阅读
    debug!("{}", serde_json::to_string_pretty(&body).unwrap()); 
    

    let client = reqwest::Client::new();
    let response = client
        .post("https://dashscope.aliyuncs.com/api/v1/services/aigc/multimodal-generation/generation")
        .header(CONTENT_TYPE, "application/json")
        .header("Authorization", format!("Bearer {}", api_key))
        .body(body.to_string())
        .send()
        .await
        .map_err(|e| AppError::TtsService(format!("百炼 TTS 请求失败: {}", e)))?;

    if !response.status().is_success() {
        let status = response.status();
        let body_text = response.text().await.unwrap_or_default();
        return Err(AppError::TtsService(format!(
            "百炼 TTS API 错误 {}: {}",
            status, body_text
        )));
    }

    let resp_body: serde_json::Value = response
        .json()
        .await
        .map_err(|e| AppError::TtsService(format!("解析百炼 TTS 响应失败: {}", e)))?;

    // Non-streaming: response contains audio URL in output.audio.url
    if let Some(url) = resp_body["output"]["audio"]["url"].as_str() {
        let audio_response = client
            .get(url)
            .send()
            .await
            .map_err(|e| AppError::TtsService(format!("下载百炼 TTS 音频失败: {}", e)))?;

        return audio_response
            .bytes()
            .await
            .map(|b| b.to_vec())
            .map_err(|e| AppError::TtsService(format!("读取百炼 TTS 音频失败: {}", e)));
    }

    // Fallback: base64-encoded audio data
    if let Some(data) = resp_body["output"]["audio"]["data"].as_str() {
        use base64::Engine;
        return base64::engine::general_purpose::STANDARD
            .decode(data)
            .map_err(|e| AppError::TtsService(format!("解码百炼 TTS 音频失败: {}", e)));
    }

    Err(AppError::TtsService(format!(
        "百炼 TTS 响应中未找到音频数据: {}",
        resp_body
    )))
}

#[tauri::command]
pub async fn generate_all_tts(
    app: tauri::AppHandle,
    db: tauri::State<'_, Mutex<Database>>,
    project_id: String,
    api_key: String,
) -> Result<usize, AppError> {
    let (script_lines, fragments, characters) = {
        let db = db.lock().map_err(|e| AppError::Database(e.to_string()))?;
        let lines = db.load_script(&project_id)?;
        let frags = db.list_audio_fragments(&project_id)?;
        let chars = db.list_characters(&project_id)?;
        (lines, frags, chars)
    };

    let existing_line_ids: std::collections::HashSet<String> =
        fragments.iter().map(|f| f.line_id.clone()).collect();

    // Build character lookup map
    let char_map: std::collections::HashMap<String, VoiceConfig> = characters
        .iter()
        .map(|c| {
            (
                c.id.clone(),
                VoiceConfig {
                    voice_name: c.voice_name.clone(),
                    tts_model: c.tts_model.clone(),
                    speed: c.speed,
                    pitch: c.pitch,
                },
            )
        })
        .collect();

    let missing: Vec<(String, String, Option<String>, String)> = script_lines
        .iter()
        .filter(|l| !existing_line_ids.contains(&l.id) && !l.text.trim().is_empty())
        .map(|l| (l.id.clone(), l.text.clone(), l.character_id.clone(), l.instructions.clone()))
        .collect();

    if missing.is_empty() {
        return Ok(0);
    }

    let total = missing.len();
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::FileSystem(e.to_string()))?;

    // Process with concurrency limit of 3 using futures::stream
    const MAX_CONCURRENT: usize = 3;
    let mut success_count = 0usize;
    let mut completed = 0usize;

    for chunk in missing.chunks(MAX_CONCURRENT) {
        let mut handles = Vec::with_capacity(chunk.len());

        for (line_id, text, character_id, instructions) in chunk {
            let app_h = app.clone();
            let proj_id = project_id.clone();
            let key = api_key.clone();
            let dir = app_data_dir.clone();
            let id = line_id.clone();
            let txt = text.clone();
            let char_id = character_id.clone();
            let instr = instructions.clone();
            let cfg = char_id.and_then(|cid| char_map.get(&cid).cloned()).unwrap_or_else(|| VoiceConfig {
                voice_name: String::new(),
                tts_model: String::new(),
                speed: 1.0,
                pitch: 1.0,
            });

            let handle = tokio::spawn(async move {
                let audio_path = build_audio_path(&dir, &proj_id, &id);
                if let Some(parent) = audio_path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }

                let vc = VoiceConfig {
                    voice_name: cfg.voice_name.clone(),
                    tts_model: cfg.tts_model.clone(),
                    speed: cfg.speed,
                    pitch: cfg.pitch,
                };

                // Wrap TTS call with timeout to prevent hanging on API issues
                let tts_result = tokio::time::timeout(
                    std::time::Duration::from_secs(60),
                    call_dashscope_tts(&txt, &vc, if instr.is_empty() { None } else { Some(&instr) }, &key),
                ).await;

                let result = match tts_result {
                    Ok(inner) => inner,
                    Err(_) => Err(AppError::TtsService(format!("TTS request timed out for line {}", id))),
                };

                match result {
                    Ok(audio_bytes) => {
                        if let Err(e) = std::fs::write(&audio_path, &audio_bytes) {
                            return (id, false, Some(format!("Write file failed: {}", e)));
                        }

                        // Re-encode with FFmpeg
                        let tmp_path = audio_path.with_extension("tmp.mp3");
                        let ffmpeg_result = std::process::Command::new("ffmpeg")
                            .args([
                                "-y",
                                "-i", &audio_path.to_string_lossy(),
                                "-codec:a", "libmp3lame",
                                "-b:a", "192k",
                                &tmp_path.to_string_lossy(),
                            ])
                            .stderr(std::process::Stdio::piped())
                            .stdout(std::process::Stdio::piped())
                            .output();

                        if let Ok(output) = ffmpeg_result {
                            if output.status.success() {
                                let _ = std::fs::rename(&tmp_path, &audio_path);
                            } else {
                                let _ = std::fs::remove_file(&tmp_path);
                            }
                        }

                        let duration_ms = get_audio_duration(&audio_path);
                        let fragment = AudioFragment {
                            id: uuid::Uuid::new_v4().to_string(),
                            project_id: proj_id.clone(),
                            line_id: id.clone(),
                            file_path: audio_path.to_string_lossy().to_string(),
                            duration_ms,
                        };

                        // Save to DB
                        let db_result = {
                            let db_guard = app_h.state::<Mutex<Database>>();
                            let result = match db_guard.lock() {
                                Ok(d) => d.upsert_audio_fragment(&fragment),
                                Err(e) => Err(AppError::Database(e.to_string())),
                            };
                            result
                        };

                        match db_result {
                            Ok(()) => (id, true, None),
                            Err(e) => (id, false, Some(format!("DB save failed: {}", e))),
                        }
                    }
                    Err(e) => (id, false, Some(e.to_string())),
                }
            });

            handles.push(handle);
        }

        // Wait for this chunk to complete
        for handle in handles {
            match handle.await {
                Ok((line_id, success, error)) => {
                    completed += 1;
                    if success {
                        success_count += 1;
                    }

                    let _ = app.emit(
                        "tts-batch-progress",
                        TtsBatchProgress {
                            current: completed,
                            total,
                            line_id,
                            success,
                            error,
                        },
                    );
                }
                Err(e) => {
                    completed += 1;
                    let _ = app.emit(
                        "tts-batch-progress",
                        TtsBatchProgress {
                            current: completed,
                            total,
                            line_id: String::new(),
                            success: false,
                            error: Some(format!("Task join failed: {}", e)),
                        },
                    );
                }
            }
        }
    }

    Ok(success_count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_audio_path() {
        let app_data = std::path::Path::new("/data");
        let path = build_audio_path(app_data, "proj-1", "line-42");
        assert_eq!(
            path,
            std::path::PathBuf::from("/data/projects/proj-1/audio/line-42.mp3")
        );
    }

    #[test]
    fn test_generate_all_tts_missing_lines() {
        let lines = vec!["l1".to_string(), "l2".to_string(), "l3".to_string()];
        let fragments = vec!["l1".to_string()];
        let missing: Vec<String> = lines
            .iter()
            .filter(|id| !fragments.contains(*id))
            .cloned()
            .collect();
        assert_eq!(missing, vec!["l2".to_string(), "l3".to_string()]);
    }
}
