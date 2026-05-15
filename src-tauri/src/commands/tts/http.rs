use crate::core::error::AppError;
use crate::core::models::VoiceConfig;
use log::{debug, error, info, warn};

// ---------------------------------------------------------------------------
// HTTP REST API for Qwen-TTS models
// ---------------------------------------------------------------------------
// POST https://dashscope.aliyuncs.com/api/v1/services/aigc/multimodal-generation/generation
// These models don't work on the CosyVoice WS endpoint.
// ---------------------------------------------------------------------------

pub(crate) async fn call_http_tts(
    text: &str,
    voice_config: &VoiceConfig,
    instructions: Option<&str>,
    api_key: &str,
    model: &str,
) -> Result<Vec<u8>, AppError> {
    use reqwest::header::CONTENT_TYPE;
    use serde_json::json;

    let has_instr = instructions.map(|i| !i.trim().is_empty()).unwrap_or(false);

    info!(
        "[TTS][http] model={}, voice={}, text_len={}, has_instr={}",
        model,
        voice_config.voice_name,
        text.len(),
        has_instr
    );

    let mut input = serde_json::Map::new();
    input.insert("text".to_string(), json!(text));
    input.insert("voice".to_string(), json!(voice_config.voice_name));
    if has_instr {
        input.insert("instructions".to_string(), json!(instructions.unwrap()));
        input.insert("optimize_instructions".to_string(), json!(true));
    }

    let body = json!({ "model": model, "input": input });
    debug!(
        "[TTS][http] Request body:\n{}",
        serde_json::to_string_pretty(&body).unwrap_or_default()
    );

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(180))
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
        return Err(AppError::TtsService(format!(
            "百炼 TTS API 错误 {}: {}",
            status, body_text
        )));
    }

    let resp_body: serde_json::Value = response
        .json()
        .await
        .map_err(|e| AppError::TtsService(format!("解析响应失败: {}", e)))?;

    debug!(
        "[TTS][http] Response:\n{}",
        serde_json::to_string_pretty(&resp_body).unwrap_or_default()
    );

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
                Ok(resp) if resp.status().is_success() => match resp.bytes().await {
                    Ok(b) => {
                        info!(
                            "[TTS][http] Downloaded {} bytes (attempt {})",
                            b.len(),
                            attempt
                        );
                        return Ok(b.to_vec());
                    }
                    Err(e) => {
                        last_err = format!("读取音频失败: {}", e);
                    }
                },
                Ok(resp) => {
                    last_err = format!("下载状态 {}", resp.status());
                }
                Err(e) => {
                    last_err = format!("下载失败: {}", e);
                }
            }
            if attempt < 3 {
                warn!(
                    "[TTS][http] Download attempt {} failed: {}, retrying...",
                    attempt, last_err
                );
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            }
        }
        return Err(AppError::TtsService(format!(
            "下载百炼 TTS 音频失败 (3次重试): {}",
            last_err
        )));
    }

    // Fallback: base64
    if let Some(data) = resp_body["output"]["audio"]["data"].as_str() {
        use base64::Engine;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(data)
            .map_err(|e| AppError::TtsService(format!("解码音频失败: {}", e)))?;
        info!("[TTS][http] Decoded base64 audio: {} bytes", bytes.len());
        return Ok(bytes);
    }

    Err(AppError::TtsService(format!(
        "响应中未找到音频: {}",
        resp_body
    )))
}
