use std::sync::Mutex;

use tauri::Emitter;

use crate::core::db::Database;
use crate::core::error::AppError;
use crate::core::models::MixProgress;

use super::ffmpeg::{build_ffmpeg_args, find_ffmpeg};

#[tauri::command]
pub async fn export_audio_mix(
    app: tauri::AppHandle,
    db: tauri::State<'_, Mutex<Database>>,
    project_id: String,
    output_path: String,
    bgm_path: Option<String>,
    bgm_volume: f32,
    sleep_mode: Option<bool>,
) -> Result<String, AppError> {
    // Load script lines (ordered) and audio fragments from database
    let (script_lines, fragments) = {
        let db = db.lock().map_err(|e| AppError::Database(e.to_string()))?;
        let lines = db.load_script(&project_id)?;
        let frags = db.list_audio_fragments(&project_id)?;
        (lines, frags)
    };

    if fragments.is_empty() {
        return Err(AppError::FFmpeg(
            "No audio fragments found for this project".to_string(),
        ));
    }

    // Build ordered audio paths and per-line gaps based on script line order
    let frag_map: std::collections::HashMap<&str, &crate::core::models::AudioFragment> =
        fragments.iter().map(|f| (f.line_id.as_str(), f)).collect();

    let mut audio_paths: Vec<String> = Vec::new();
    let mut gaps_ms: Vec<i32> = Vec::new();
    let total_clips = script_lines
        .iter()
        .filter(|line| frag_map.contains_key(line.id.as_str()))
        .count();
    let mut processed = 0usize;

    for line in &script_lines {
        if let Some(frag) = frag_map.get(line.id.as_str()) {
            let path = std::path::Path::new(&frag.file_path);
            if !path.exists() {
                return Err(AppError::FileSystem(format!(
                    "Audio fragment file not found: {}",
                    frag.file_path
                )));
            }
            audio_paths.push(frag.file_path.clone());
            gaps_ms.push(line.gap_after_ms);
            processed += 1;

            // Emit per-clip verification progress (0-15%)
            if processed.is_multiple_of(5) || processed == total_clips {
                let pct = (processed as f64 / total_clips as f64) * 15.0;
                let _ = app.emit(
                    "mix-progress",
                    MixProgress {
                        percent: pct as f32,
                        stage: format!("正在校验音频 {}/{}", processed, total_clips),
                    },
                );
            }
        }
    }

    if audio_paths.is_empty() {
        return Err(AppError::FFmpeg(
            "No audio fragments found for this project".to_string(),
        ));
    }

    // Verify BGM file exists if provided
    if let Some(ref bgm) = bgm_path {
        if !std::path::Path::new(bgm).exists() {
            return Err(AppError::FileSystem(format!("BGM file not found: {}", bgm)));
        }
    }

    // Verify output parent directory exists
    if let Some(parent) = std::path::Path::new(&output_path).parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            AppError::FileSystem(format!("Failed to create output directory: {}", e))
        })?;
    }

    // Emit initial progress
    let _ = app.emit(
        "mix-progress",
        MixProgress {
            percent: 0.0,
            stage: "正在准备混音".to_string(),
        },
    );

    // Build FFmpeg command
    let ffmpeg_args = build_ffmpeg_args(
        &audio_paths,
        bgm_path.as_deref(),
        bgm_volume,
        &gaps_ms,
        &output_path,
        sleep_mode.unwrap_or(false),
    );

    let _ = app.emit(
        "mix-progress",
        MixProgress {
            percent: 20.0,
            stage: "正在启动 FFmpeg".to_string(),
        },
    );

    // Run FFmpeg subprocess — try common macOS paths if not in PATH
    let ffmpeg_bin = find_ffmpeg();
    let ffmpeg_args = ffmpeg_args.clone();

    let output = tokio::task::spawn_blocking(move || {
        std::process::Command::new(&ffmpeg_bin)
            .args(&ffmpeg_args)
            .stderr(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .output()
    })
    .await
    .map_err(|e| AppError::FFmpeg(format!("FFmpeg spawn_blocking failed: {}", e)))?
    .map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            AppError::FFmpeg(
                "FFmpeg not found. Please install FFmpeg (brew install ffmpeg) and ensure it is in your PATH."
                    .to_string(),
            )
        } else {
            AppError::FFmpeg(format!("Failed to start FFmpeg: {}", e))
        }
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AppError::FFmpeg(format!(
            "FFmpeg exited with error: {}",
            stderr
        )));
    }

    let _ = app.emit(
        "mix-progress",
        MixProgress {
            percent: 100.0,
            stage: "混音完成".to_string(),
        },
    );

    Ok(output_path)
}
