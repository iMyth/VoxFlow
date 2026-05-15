//! Hyperframes video export module.
//!
//! Exports VoxFlow projects as Hyperframes HTML compositions
//! for rendering audiobook videos with synchronized text/animations.

pub mod ai_generate;
pub mod prompt;
pub mod templates;
pub mod timeline;
pub mod validation;

use std::sync::Mutex;

use log::info;
use tauri::Emitter;

use crate::core::config::ConfigManager;
use crate::core::db::Database;
use crate::core::error::AppError;

use self::ai_generate::{generate_composition, LlmConfig};
use self::templates::{generate_html, generate_meta_json};
use self::timeline::compute_timeline;

/// Progress event payload emitted to the frontend via `hyperframes-progress`.
#[derive(Debug, Clone, serde::Serialize)]
struct HyperframesProgress {
    percent: f32,
    stage: String,
}

/// Main entry point for Hyperframes video export.
///
/// Loads project data, computes the timeline, generates HTML (via template or AI),
/// and writes the output files to the specified directory.
#[tauri::command]
pub async fn export_hyperframes(
    app: tauri::AppHandle,
    db: tauri::State<'_, Mutex<Database>>,
    project_id: String,
    output_dir: String,
    template: String,
    include_audio: bool,
    audio_path: Option<String>,
    use_ai: bool,
) -> Result<String, AppError> {
    info!(
        "[Hyperframes] export_hyperframes: project={}, template={}, use_ai={}, include_audio={}, audio_path={:?}",
        project_id, template, use_ai, include_audio, audio_path
    );

    // --- Emit initial progress ---
    let _ = app.emit(
        "hyperframes-progress",
        HyperframesProgress {
            percent: 0.0,
            stage: "正在加载项目数据...".to_string(),
        },
    );

    // --- Load project data from database ---
    let (script_lines, fragments) = {
        let db = db.lock().map_err(|e| AppError::Database(e.to_string()))?;
        let lines = db.load_script_lines(&project_id)?;
        let frags = db.list_audio_fragments(&project_id)?;
        (lines, frags)
    };

    // --- Check if there are any audio fragments ---
    if fragments.is_empty() {
        return Err(AppError::FileSystem("请先生成音频".to_string()));
    }

    let _ = app.emit(
        "hyperframes-progress",
        HyperframesProgress {
            percent: 10.0,
            stage: "正在计算时间轴...".to_string(),
        },
    );

    // --- Compute timeline ---
    let timeline_entries = compute_timeline(&script_lines, &fragments);

    if timeline_entries.is_empty() {
        return Err(AppError::FileSystem(
            "没有可用的时间轴数据（所有音频片段缺少时长信息）".to_string(),
        ));
    }

    let _ = app.emit(
        "hyperframes-progress",
        HyperframesProgress {
            percent: 20.0,
            stage: "正在生成 HTML...".to_string(),
        },
    );

    // --- Generate HTML based on use_ai flag ---
    let html = if use_ai {
        // AI generation path: load LLM settings and call AI pipeline
        let (api_endpoint, model) = {
            let db = db.lock().map_err(|e| AppError::Database(e.to_string()))?;
            let settings = db.load_settings()?;
            (settings.llm_endpoint, settings.llm_model)
        };

        // Load API key from secure store
        let config_manager = ConfigManager::new(app.clone());
        let api_key = config_manager
            .load_api_key("llm")
            .map_err(|e| AppError::Config(format!("无法加载 API 密钥: {}", e)))?
            .ok_or_else(|| AppError::Config("未配置 LLM API 密钥".to_string()))?;

        let llm_config = LlmConfig {
            api_endpoint: &api_endpoint,
            api_key: &api_key,
            model: &model,
        };

        // Create a progress callback that emits events
        let app_clone = app.clone();
        let on_progress: Box<dyn Fn(&str) + Send + Sync> = Box::new(move |stage: &str| {
            let _ = app_clone.emit(
                "hyperframes-progress",
                HyperframesProgress {
                    percent: 50.0,
                    stage: stage.to_string(),
                },
            );
        });

        generate_composition(&timeline_entries, &llm_config, Some(on_progress))
            .await
            .map_err(AppError::LlmService)?
    } else {
        // Fixed template path
        generate_html(&template, &timeline_entries).map_err(AppError::FileSystem)?
    };

    let _ = app.emit(
        "hyperframes-progress",
        HyperframesProgress {
            percent: 80.0,
            stage: "正在写入文件...".to_string(),
        },
    );

    // --- Write output files ---
    let output_path = std::path::Path::new(&output_dir);

    // Create output directory and assets subdirectory
    std::fs::create_dir_all(output_path.join("assets"))
        .map_err(|e| AppError::FileSystem(format!("无法创建输出目录: {}", e)))?;

    // Write index.html
    std::fs::write(output_path.join("index.html"), &html)
        .map_err(|e| AppError::FileSystem(format!("无法写入 index.html: {}", e)))?;

    // Write meta.json
    let total_duration = timeline_entries
        .iter()
        .map(|e| e.start_time + e.duration)
        .fold(0.0_f64, f64::max);
    let meta_json = generate_meta_json(&template, &project_id, total_duration);
    std::fs::write(output_path.join("meta.json"), &meta_json)
        .map_err(|e| AppError::FileSystem(format!("无法写入 meta.json: {}", e)))?;

    // Copy audio file if requested
    if include_audio {
        if let Some(ref src_audio_path) = audio_path {
            let src = std::path::Path::new(src_audio_path);
            if src.exists() {
                let dest = output_path.join("assets").join("audio.mp3");
                std::fs::copy(src, &dest)
                    .map_err(|e| AppError::FileSystem(format!("无法复制音频文件: {}", e)))?;
                info!("[Hyperframes] Copied audio: {:?} -> {:?}", src, dest);
            } else {
                info!("[Hyperframes] Audio file not found: {:?}", src);
            }
        } else {
            info!("[Hyperframes] include_audio=true but no audio_path provided");
        }
    }

    let _ = app.emit(
        "hyperframes-progress",
        HyperframesProgress {
            percent: 100.0,
            stage: "导出完成".to_string(),
        },
    );

    info!("[Hyperframes] Export complete: {}", output_dir);
    Ok(output_dir)
}
