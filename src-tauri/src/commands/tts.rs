use std::sync::Mutex;

use tauri::Manager;

use crate::core::db::Database;
use crate::core::error::AppError;
use crate::core::models::{AudioFragment, TtsEngine, VoiceConfig};

/// Build the audio file path for a given project and line.
/// Returns `{app_data_dir}/projects/{project_id}/audio/{line_id}.mp3`
pub fn build_audio_path(app_data_dir: &std::path::Path, project_id: &str, line_id: &str) -> std::path::PathBuf {
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
    api_key: Option<String>,
) -> Result<AudioFragment, AppError> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::FileSystem(e.to_string()))?;

    let audio_path = build_audio_path(&app_data_dir, &project_id, &line_id);

    // Ensure the audio directory exists
    if let Some(parent) = audio_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            AppError::FileSystem(format!("Failed to create audio directory: {}", e))
        })?;
    }

    // Call TTS API based on engine type
    let audio_bytes = match voice_config.engine {
        TtsEngine::EdgeTts => call_edge_tts(&text, &voice_config).await?,
        TtsEngine::AzureTts => {
            let key = api_key.clone().ok_or_else(|| {
                AppError::Config("Azure TTS requires an API key".to_string())
            })?;
            call_azure_tts(&text, &voice_config, &key).await?
        }
        TtsEngine::DashscopeTts => {
            let key = api_key.clone().ok_or_else(|| {
                AppError::Config("阿里百炼 TTS 需要 API Key".to_string())
            })?;
            call_dashscope_tts(&text, &voice_config, &key).await?
        }
    };

    // Save audio file
    std::fs::write(&audio_path, &audio_bytes).map_err(|e| {
        AppError::FileSystem(format!("Failed to write audio file: {}", e))
    })?;

    let file_path = audio_path.to_string_lossy().to_string();
    let fragment = AudioFragment {
        id: uuid::Uuid::new_v4().to_string(),
        project_id: project_id.clone(),
        line_id: line_id.clone(),
        file_path,
        duration_ms: None,
    };

    // Upsert audio fragment record in database
    let db = db.lock().map_err(|e| AppError::Database(e.to_string()))?;
    db.upsert_audio_fragment(&fragment)?;

    Ok(fragment)
}

/// Call Edge TTS service to generate audio.
/// Uses the edge-tts compatible HTTP endpoint.
async fn call_edge_tts(text: &str, voice_config: &VoiceConfig) -> Result<Vec<u8>, AppError> {
    use reqwest::header::CONTENT_TYPE;

    // Edge TTS uses a WebSocket-based protocol. For simplicity, we use a REST-compatible
    // approach via the Microsoft Speech API endpoint that Edge TTS wraps.

    // Build SSML payload
    let ssml = format!(
        r#"<speak version="1.0" xmlns="http://www.w3.org/2001/10/synthesis" xml:lang="zh-CN">
            <voice name="{}">
                <prosody rate="{}" pitch="{}%">{}</prosody>
            </voice>
        </speak>"#,
        voice_config.voice_name,
        format_rate(voice_config.speed),
        format_pitch(voice_config.pitch),
        escape_xml(text),
    );

    let client = reqwest::Client::new();
    let response = client
        .post("https://eastus.api.speech.microsoft.com/cognitiveservices/v1")
        .header(CONTENT_TYPE, "application/ssml+xml")
        .header("X-Microsoft-OutputFormat", "audio-16khz-128kbitrate-mono-mp3")
        .header("User-Agent", "VoxFlow/1.0")
        .body(ssml)
        .send()
        .await
        .map_err(|e| AppError::TtsService(format!("Edge TTS request failed: {}", e)))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(AppError::TtsService(format!(
            "Edge TTS API error {}: {}",
            status, body
        )));
    }

    response
        .bytes()
        .await
        .map(|b| b.to_vec())
        .map_err(|e| AppError::TtsService(format!("Failed to read Edge TTS response: {}", e)))
}

/// Call Azure TTS service to generate audio.
async fn call_azure_tts(
    text: &str,
    voice_config: &VoiceConfig,
    api_key: &str,
) -> Result<Vec<u8>, AppError> {
    use reqwest::header::CONTENT_TYPE;

    let ssml = format!(
        r#"<speak version="1.0" xmlns="http://www.w3.org/2001/10/synthesis" xml:lang="zh-CN">
            <voice name="{}">
                <prosody rate="{}" pitch="{}%">{}</prosody>
            </voice>
        </speak>"#,
        voice_config.voice_name,
        format_rate(voice_config.speed),
        format_pitch(voice_config.pitch),
        escape_xml(text),
    );

    let client = reqwest::Client::new();
    let response = client
        .post("https://eastus.tts.speech.microsoft.com/cognitiveservices/v1")
        .header(CONTENT_TYPE, "application/ssml+xml")
        .header("Ocp-Apim-Subscription-Key", api_key)
        .header("X-Microsoft-OutputFormat", "audio-16khz-128kbitrate-mono-mp3")
        .header("User-Agent", "VoxFlow/1.0")
        .body(ssml)
        .send()
        .await
        .map_err(|e| AppError::TtsService(format!("Azure TTS request failed: {}", e)))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(AppError::TtsService(format!(
            "Azure TTS API error {}: {}",
            status, body
        )));
    }

    response
        .bytes()
        .await
        .map(|b| b.to_vec())
        .map_err(|e| AppError::TtsService(format!("Failed to read Azure TTS response: {}", e)))
}

/// 调用阿里百炼 (DashScope) CosyVoice TTS 服务生成音频。
///
/// 使用 DashScope REST API：
/// POST https://dashscope.aliyuncs.com/api/v1/services/aigc/multimodal-generation/generation
///
/// voice_config.voice_name 对应 CosyVoice 音色名（如 "longanyang"、"longxiaochun" 等）。
/// 默认使用 cosyvoice-v3-flash 模型。
async fn call_dashscope_tts(
    text: &str,
    voice_config: &VoiceConfig,
    api_key: &str,
) -> Result<Vec<u8>, AppError> {
    use reqwest::header::CONTENT_TYPE;
    use serde_json::json;

    let body = json!({
        "model": "cosyvoice-v3-flash",
        "input": {
            "text": text,
            "voice": voice_config.voice_name,
            "speech_rate": voice_config.speed,
            "pitch_rate": voice_config.pitch
        },
        "parameters": {
            "response_format": "mp3",
            "sample_rate": 22050
        }
    });

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

    // DashScope returns JSON with base64-encoded audio in output.audio.data
    let resp_body: serde_json::Value = response
        .json()
        .await
        .map_err(|e| AppError::TtsService(format!("解析百炼 TTS 响应失败: {}", e)))?;

    // Try to extract audio URL first (non-streaming returns a URL)
    if let Some(url) = resp_body["output"]["audio"]["url"].as_str() {
        // Download audio from the URL
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

    // Fallback: try base64-encoded audio data
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

/// Format speed value for SSML prosody rate attribute.
/// 1.0 → "+0%", 1.5 → "+50%", 0.5 → "-50%"
fn format_rate(speed: f32) -> String {
    let percent = ((speed - 1.0) * 100.0) as i32;
    if percent >= 0 {
        format!("+{}%", percent)
    } else {
        format!("{}%", percent)
    }
}

/// Format pitch value for SSML prosody pitch attribute.
/// 1.0 → "0", 1.5 → "50", 0.5 → "-50"
fn format_pitch(pitch: f32) -> String {
    let percent = ((pitch - 1.0) * 100.0) as i32;
    format!("{}", percent)
}

/// Escape special XML characters in text.
fn escape_xml(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
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
    fn test_format_rate() {
        assert_eq!(format_rate(1.0), "+0%");
        assert_eq!(format_rate(1.5), "+50%");
        assert_eq!(format_rate(0.5), "-50%");
        assert_eq!(format_rate(2.0), "+100%");
    }

    #[test]
    fn test_format_pitch() {
        assert_eq!(format_pitch(1.0), "0");
        assert_eq!(format_pitch(1.5), "50");
        assert_eq!(format_pitch(0.5), "-50");
    }

    #[test]
    fn test_escape_xml() {
        assert_eq!(escape_xml("hello & world"), "hello &amp; world");
        assert_eq!(escape_xml("<tag>"), "&lt;tag&gt;");
        assert_eq!(escape_xml("a\"b'c"), "a&quot;b&apos;c");
        assert_eq!(escape_xml("no special chars"), "no special chars");
    }
}
