use tauri::Manager;

use crate::core::error::AppError;
use crate::core::models::VoiceConfig;
use log::{debug, info, warn};

use super::models::VoiceEnrollmentOutput;
use super::websocket::{ws_realtime_connect, ws_realtime_run_task};

/// Fixed VC model for voice cloning - user cannot select, it's always this model
const VC_TTS_MODEL: &str = "qwen3-tts-vc-realtime-2026-01-15";

/// Create a cloned voice from uploaded/recorded audio via DashScope voice enrollment API.
#[tauri::command]
pub async fn create_voice(
    app: tauri::AppHandle,
    project_id: String,
    audio_data_base64: String,
    preferred_name: String,
    _target_model: String, // Kept for API compatibility, but we use fixed model
) -> Result<String, AppError> {

    info!(
        "[VoiceClone] create_voice: project={}, name={}, received_model={}",
        project_id, preferred_name, _target_model
    );

    // Always use the fixed VC model, ignore the passed parameter
    let target_model = VC_TTS_MODEL;
    info!("[VoiceClone] Using fixed model: {}", target_model);

    let api_key = {
        let config = crate::core::config::ConfigManager::new(app.clone());
        config
            .load_api_key("dashscope")
            .map_err(|e| AppError::Config(format!("Failed to load API key: {}", e)))?
            .ok_or_else(|| AppError::Config("DashScope API key not configured".into()))?
    };
    debug!("[VoiceClone] API key loaded successfully");

    // The frontend sends a full data URI (e.g. "data:audio/mp4;base64,...")
    // Use it directly so the MIME type is preserved correctly.
    // Previously this was hardcoded to audio/mpeg which caused DashScope to
    // report "No audio data received" when the recording was in MP4/AAC format.
    info!("[VoiceClone] Data URI length: {} chars", audio_data_base64.len());

    // Validate it's actually a data URI
    if !audio_data_base64.starts_with("data:") {
        warn!("[VoiceClone] audio_data_base64 does not look like a data URI, proceeding anyway");
    }

    // Log preview for debugging
    let preview_end = audio_data_base64.len().min(150);
    info!("[VoiceClone] Data URI preview: {}...", &audio_data_base64[..preview_end]);

    let data_uri = audio_data_base64.clone();

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
    // Log the full payload for debugging
    let payload_str = payload.to_string();
    info!("[VoiceClone] Request payload length: {} chars", payload_str.len());
    if payload_str.len() > 500 {
        info!("[VoiceClone] Payload preview: {}...{} (truncated)", &payload_str[..200], &payload_str[payload_str.len()-100..]);
    } else {
        info!("[VoiceClone] Payload: {}", payload_str);
    }

    info!("[VoiceClone] Sending HTTP request to {}", url);
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120)) // 2 minute timeout for voice enrollment
        .build()
        .map_err(|e| AppError::TtsService(format!("Failed to create HTTP client: {}", e)))?;
    let resp = client
        .post(url)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&payload)
        .send()
        .await
        .map_err(|e| {
            warn!("[VoiceClone] HTTP request failed: {}", e);
            AppError::TtsService(format!("HTTP request failed: {}", e))
        })?;

    let status = resp.status();
    info!("[VoiceClone] Response status: {}", status);

    let body = resp
        .text()
        .await
        .map_err(|e| {
            warn!("[VoiceClone] Read response body failed: {}", e);
            AppError::TtsService(format!("Read response body failed: {}", e))
        })?;
    debug!("[VoiceClone] Response body: {}", body);

    if !status.is_success() {
        warn!("[VoiceClone] Voice enrollment failed: status={}, body={}", status, body);
        return Err(AppError::TtsService(format!(
            "Voice enrollment failed ({}): {}",
            status, body
        )));
    }

    let output: VoiceEnrollmentOutput = serde_json::from_str(&body)
        .map_err(|e| {
            warn!("[VoiceClone] Parse response failed: {}, body: {}", e, body);
            AppError::TtsService(format!("Parse response failed: {}, body: {}", e, body))
        })?;

    info!("[VoiceClone] Voice created successfully: {}", output.output.voice);
    Ok(output.output.voice)
}

/// Preview a cloned voice by synthesizing a short test sentence via WS realtime.
/// Returns the path to the generated preview audio file.
#[tauri::command]
pub async fn preview_voice(
    app: tauri::AppHandle,
    project_id: String,
    voice: String,
    _target_model: String, // Kept for API compatibility, but we use fixed model
) -> Result<String, AppError> {
    // Always use the fixed VC model
    let target_model = VC_TTS_MODEL;

    info!(
        "[VoiceClone] preview_voice: project={}, voice={}, using_fixed_model={}",
        project_id, voice, target_model
    );

    let api_key = {
        let config = crate::core::config::ConfigManager::new(app.clone());
        config
            .load_api_key("dashscope")
            .map_err(|e| AppError::Config(format!("Failed to load API key: {}", e)))?
            .ok_or_else(|| AppError::Config("DashScope API key not configured".into()))?
    };
    debug!("[VoiceClone] preview_voice: API key loaded");

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

    // Cache by voice name — skip regeneration if preview already exists
    let file_path = preview_dir
        .join(format!("{}.mp3", &voice))
        .to_string_lossy()
        .to_string();

    if std::path::Path::new(&file_path).exists() {
        info!("[VoiceClone] preview_voice: cache hit for voice={}", voice);
        return Ok(file_path);
    }

    // Use WS realtime to synthesize a short preview sentence
    let voice_config = VoiceConfig {
        voice_name: voice.clone(),
        tts_model: target_model.to_string(),
        speed: 1.0,
        pitch: 1.0,
    };

    let preview_text = "你好，这是我的专属声音";
    info!("[VoiceClone] preview_voice: Connecting to WebSocket for model: {}", target_model);

    let audio_bytes = {
        let ws = ws_realtime_connect(&api_key, target_model).await.map_err(|e| {
            warn!("[VoiceClone] preview_voice: WebSocket connection failed: {}", e);
            e
        })?;
        let mut ws = ws;
        info!("[VoiceClone] preview_voice: WebSocket connected, running TTS task");
        ws_realtime_run_task(
            &mut ws,
            &[preview_text],
            &voice_config,
            None,
            target_model,
        )
        .await
        .map_err(|e| {
            warn!("[VoiceClone] preview_voice: TTS task failed: {}", e);
            e
        })?
    };

    info!("[VoiceClone] preview_voice: Received {} bytes of audio", audio_bytes.len());

    std::fs::write(&file_path, &audio_bytes)
        .map_err(|e| AppError::FileSystem(format!("write preview: {}", e)))?;

    info!("[VoiceClone] Preview audio saved: {}", file_path);
    Ok(file_path)
}
