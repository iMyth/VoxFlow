//! Resource management commands.
//!
//! Provides commands for listing, inspecting, and deleting generated audio/video
//! resources within a project directory.

use std::sync::Mutex;

use tauri::Manager;

use crate::core::db::Database;
use crate::core::error::AppError;
use crate::core::models::AudioFragment;

/// A resource entry representing a generated file on disk.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ResourceEntry {
    /// Unique identifier (file path relative to project dir)
    pub id: String,
    /// Display name for the resource
    pub name: String,
    /// Resource type: "audio", "video", "composition", "bgm", "export"
    pub resource_type: String,
    /// Absolute file path on disk
    pub file_path: String,
    /// File size in bytes
    pub file_size: u64,
    /// Creation/modification time as ISO string
    pub created_at: String,
    /// Duration in milliseconds (for audio/video, if known)
    pub duration_ms: Option<i64>,
    /// Associated section ID (if any)
    pub section_id: Option<String>,
    /// Associated section title (if any)
    pub section_title: Option<String>,
}

/// Summary of disk usage for a project.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ResourceSummary {
    pub total_files: usize,
    pub total_size_bytes: u64,
    pub audio_count: usize,
    pub audio_size_bytes: u64,
    pub video_count: usize,
    pub video_size_bytes: u64,
    pub other_count: usize,
    pub other_size_bytes: u64,
}

/// List all generated resources for a project by scanning the project directory
/// and cross-referencing with the database for audio fragments.
#[tauri::command]
pub fn list_resources(
    db: tauri::State<'_, Mutex<Database>>,
    app: tauri::AppHandle,
    project_id: String,
) -> Result<Vec<ResourceEntry>, AppError> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::FileSystem(e.to_string()))?;

    let project_dir = app_data_dir.join("projects").join(&project_id);
    if !project_dir.exists() {
        return Ok(Vec::new());
    }

    // Load audio fragments from DB for duration info
    let audio_fragments: Vec<AudioFragment> = {
        let db = db.lock().map_err(|e| AppError::Database(e.to_string()))?;
        db.list_audio_fragments(&project_id)?
    };

    // Load sections from DB for title mapping
    let sections: Vec<(String, String)> = {
        let db = db.lock().map_err(|e| AppError::Database(e.to_string()))?;
        let secs = db.list_sections(&project_id)?;
        secs.into_iter().map(|s| (s.id, s.title)).collect()
    };
    let section_map: std::collections::HashMap<String, String> =
        sections.into_iter().collect();

    let mut resources = Vec::new();

    // 1. Scan audio directory
    let audio_dir = project_dir.join("audio");
    if audio_dir.exists() {
        scan_directory(
            &audio_dir,
            &project_dir,
            "audio",
            &audio_fragments,
            &section_map,
            &mut resources,
        )?;
    }

    // 2. Scan BGM directory
    let bgm_dir = project_dir.join("bgm");
    if bgm_dir.exists() {
        scan_directory(
            &bgm_dir,
            &project_dir,
            "bgm",
            &audio_fragments,
            &section_map,
            &mut resources,
        )?;
    }

    // 3. Scan export directory (videos, compositions)
    let export_dir = project_dir.join("export");
    if export_dir.exists() {
        scan_export_directory(
            &export_dir,
            &project_dir,
            &audio_fragments,
            &section_map,
            &mut resources,
        )?;
    }

    // Sort by creation time descending (newest first)
    resources.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    Ok(resources)
}

/// Get a summary of resource usage for a project.
#[tauri::command]
pub fn get_resource_summary(
    app: tauri::AppHandle,
    project_id: String,
) -> Result<ResourceSummary, AppError> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::FileSystem(e.to_string()))?;

    let project_dir = app_data_dir.join("projects").join(&project_id);
    if !project_dir.exists() {
        return Ok(ResourceSummary {
            total_files: 0,
            total_size_bytes: 0,
            audio_count: 0,
            audio_size_bytes: 0,
            video_count: 0,
            video_size_bytes: 0,
            other_count: 0,
            other_size_bytes: 0,
        });
    }

    let mut summary = ResourceSummary {
        total_files: 0,
        total_size_bytes: 0,
        audio_count: 0,
        audio_size_bytes: 0,
        video_count: 0,
        video_size_bytes: 0,
        other_count: 0,
        other_size_bytes: 0,
    };

    scan_dir_for_summary(&project_dir, &mut summary)?;

    Ok(summary)
}

/// Delete a specific resource file from disk. For audio fragments, also removes
/// the database record.
#[tauri::command]
pub fn delete_resource(
    db: tauri::State<'_, Mutex<Database>>,
    app: tauri::AppHandle,
    project_id: String,
    file_path: String,
) -> Result<(), AppError> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::FileSystem(e.to_string()))?;

    let project_dir = app_data_dir.join("projects").join(&project_id);

    // Security check: ensure the file is within the project directory
    let canonical_project = project_dir
        .canonicalize()
        .map_err(|e| AppError::FileSystem(format!("Cannot resolve project dir: {}", e)))?;
    let target_path = std::path::PathBuf::from(&file_path);
    let canonical_target = target_path
        .canonicalize()
        .map_err(|e| AppError::FileSystem(format!("Cannot resolve target file: {}", e)))?;

    if !canonical_target.starts_with(&canonical_project) {
        return Err(AppError::FileSystem(
            "Cannot delete files outside the project directory".to_string(),
        ));
    }

    // If it's an audio fragment tracked in DB, remove the record
    {
        let db = db.lock().map_err(|e| AppError::Database(e.to_string()))?;
        let fragments = db.list_audio_fragments(&project_id)?;
        for frag in &fragments {
            if frag.file_path == file_path {
                // Delete via line_id to remove the DB record
                let _ = db.delete_audio_by_line(&frag.line_id);
                break;
            }
        }
    }

    // Delete the file from disk
    if target_path.exists() {
        std::fs::remove_file(&target_path).map_err(|e| {
            AppError::FileSystem(format!("Failed to delete file {}: {}", file_path, e))
        })?;
    }

    Ok(())
}

/// Delete multiple resource files at once.
#[tauri::command]
pub fn delete_resources_batch(
    db: tauri::State<'_, Mutex<Database>>,
    app: tauri::AppHandle,
    project_id: String,
    file_paths: Vec<String>,
) -> Result<u32, AppError> {
    let mut deleted = 0u32;
    for path in file_paths {
        match delete_resource(
            db.clone(),
            app.clone(),
            project_id.clone(),
            path,
        ) {
            Ok(_) => deleted += 1,
            Err(e) => {
                log::warn!("Failed to delete resource: {}", e);
            }
        }
    }
    Ok(deleted)
}

/// Open the project resource directory in the system file manager.
#[tauri::command]
pub fn open_resource_folder(
    app: tauri::AppHandle,
    project_id: String,
    subfolder: Option<String>,
) -> Result<(), AppError> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::FileSystem(e.to_string()))?;

    let mut target = app_data_dir.join("projects").join(&project_id);
    if let Some(sub) = subfolder {
        target = target.join(sub);
    }

    if !target.exists() {
        std::fs::create_dir_all(&target).map_err(|e| {
            AppError::FileSystem(format!("Failed to create directory: {}", e))
        })?;
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&target)
            .spawn()
            .map_err(|e| AppError::FileSystem(format!("Failed to open folder: {}", e)))?;
    }

    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(&target)
            .spawn()
            .map_err(|e| AppError::FileSystem(format!("Failed to open folder: {}", e)))?;
    }

    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(&target)
            .spawn()
            .map_err(|e| AppError::FileSystem(format!("Failed to open folder: {}", e)))?;
    }

    Ok(())
}

// ---- Helper functions ----

fn scan_directory(
    dir: &std::path::Path,
    project_dir: &std::path::Path,
    resource_type: &str,
    audio_fragments: &[AudioFragment],
    _section_map: &std::collections::HashMap<String, String>,
    resources: &mut Vec<ResourceEntry>,
) -> Result<(), AppError> {
    let entries = std::fs::read_dir(dir)
        .map_err(|e| AppError::FileSystem(format!("Failed to read directory: {}", e)))?;

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let metadata = std::fs::metadata(&path)
            .map_err(|e| AppError::FileSystem(format!("Failed to read metadata: {}", e)))?;

        let file_path_str = path.to_string_lossy().to_string();
        let rel_path = path
            .strip_prefix(project_dir)
            .unwrap_or(&path)
            .to_string_lossy()
            .to_string();

        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| rel_path.clone());

        // Find duration from DB if this is a known audio fragment
        let duration_ms = audio_fragments
            .iter()
            .find(|f| f.file_path == file_path_str)
            .and_then(|f| f.duration_ms);

        let created_at = metadata
            .modified()
            .or_else(|_| metadata.created())
            .map(|t| {
                let datetime: chrono::DateTime<chrono::Utc> = t.into();
                datetime.format("%Y-%m-%d %H:%M:%S").to_string()
            })
            .unwrap_or_default();

        resources.push(ResourceEntry {
            id: rel_path,
            name,
            resource_type: resource_type.to_string(),
            file_path: file_path_str,
            file_size: metadata.len(),
            created_at,
            duration_ms,
            section_id: None,
            section_title: None,
        });
    }

    Ok(())
}

fn scan_export_directory(
    export_dir: &std::path::Path,
    project_dir: &std::path::Path,
    audio_fragments: &[AudioFragment],
    section_map: &std::collections::HashMap<String, String>,
    resources: &mut Vec<ResourceEntry>,
) -> Result<(), AppError> {
    // Scan sections subdirectory
    let sections_dir = export_dir.join("sections");
    if sections_dir.exists() {
        let section_entries = std::fs::read_dir(&sections_dir)
            .map_err(|e| AppError::FileSystem(format!("Failed to read sections dir: {}", e)))?;

        for section_entry in section_entries.flatten() {
            let section_path = section_entry.path();
            if !section_path.is_dir() {
                continue;
            }

            let section_id = section_path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            let section_title = section_map.get(&section_id).cloned();

            // Look for output.mp4 (final video)
            let output_video = section_path.join("output.mp4");
            if output_video.exists() {
                if let Ok(metadata) = std::fs::metadata(&output_video) {
                    let file_path_str = output_video.to_string_lossy().to_string();
                    let rel_path = output_video
                        .strip_prefix(project_dir)
                        .unwrap_or(&output_video)
                        .to_string_lossy()
                        .to_string();

                    let display_name = if let Some(ref title) = section_title {
                        format!("{}.mp4", title)
                    } else {
                        "output.mp4".to_string()
                    };

                    let created_at = metadata
                        .modified()
                        .or_else(|_| metadata.created())
                        .map(|t| {
                            let datetime: chrono::DateTime<chrono::Utc> = t.into();
                            datetime.format("%Y-%m-%d %H:%M:%S").to_string()
                        })
                        .unwrap_or_default();

                    resources.push(ResourceEntry {
                        id: rel_path,
                        name: display_name,
                        resource_type: "video".to_string(),
                        file_path: file_path_str,
                        file_size: metadata.len(),
                        created_at,
                        duration_ms: None,
                        section_id: Some(section_id.clone()),
                        section_title: section_title.clone(),
                    });
                }
            }

            // Look for composition/index.html
            let composition_html = section_path.join("composition").join("index.html");
            if composition_html.exists() {
                if let Ok(metadata) = std::fs::metadata(&composition_html) {
                    let file_path_str = composition_html.to_string_lossy().to_string();
                    let rel_path = composition_html
                        .strip_prefix(project_dir)
                        .unwrap_or(&composition_html)
                        .to_string_lossy()
                        .to_string();

                    let display_name = if let Some(ref title) = section_title {
                        format!("{} (HTML)", title)
                    } else {
                        "composition.html".to_string()
                    };

                    let created_at = metadata
                        .modified()
                        .or_else(|_| metadata.created())
                        .map(|t| {
                            let datetime: chrono::DateTime<chrono::Utc> = t.into();
                            datetime.format("%Y-%m-%d %H:%M:%S").to_string()
                        })
                        .unwrap_or_default();

                    resources.push(ResourceEntry {
                        id: rel_path,
                        name: display_name,
                        resource_type: "composition".to_string(),
                        file_path: file_path_str,
                        file_size: metadata.len(),
                        created_at,
                        duration_ms: None,
                        section_id: Some(section_id.clone()),
                        section_title: section_title.clone(),
                    });
                }
            }

            // Look for _render_output.mp4 inside composition
            let render_output = section_path.join("composition").join("_render_output.mp4");
            if render_output.exists() {
                if let Ok(metadata) = std::fs::metadata(&render_output) {
                    let file_path_str = render_output.to_string_lossy().to_string();
                    let rel_path = render_output
                        .strip_prefix(project_dir)
                        .unwrap_or(&render_output)
                        .to_string_lossy()
                        .to_string();

                    let display_name = if let Some(ref title) = section_title {
                        format!("{} (渲染)", title)
                    } else {
                        "render_output.mp4".to_string()
                    };

                    let created_at = metadata
                        .modified()
                        .or_else(|_| metadata.created())
                        .map(|t| {
                            let datetime: chrono::DateTime<chrono::Utc> = t.into();
                            datetime.format("%Y-%m-%d %H:%M:%S").to_string()
                        })
                        .unwrap_or_default();

                    resources.push(ResourceEntry {
                        id: rel_path,
                        name: display_name,
                        resource_type: "video".to_string(),
                        file_path: file_path_str,
                        file_size: metadata.len(),
                        created_at,
                        duration_ms: None,
                        section_id: Some(section_id.clone()),
                        section_title: section_title.clone(),
                    });
                }
            }
        }
    }

    // Scan top-level export files (e.g., merged videos, audio exports)
    let top_entries = std::fs::read_dir(export_dir)
        .map_err(|e| AppError::FileSystem(format!("Failed to read export dir: {}", e)))?;

    for entry in top_entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let ext = path
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();

        let resource_type = match ext.as_str() {
            "mp4" | "webm" | "mov" => "video",
            "mp3" | "wav" | "ogg" | "m4a" | "flac" => "export",
            _ => continue, // skip non-media files at top level
        };

        if let Ok(metadata) = std::fs::metadata(&path) {
            let file_path_str = path.to_string_lossy().to_string();
            let rel_path = path
                .strip_prefix(project_dir)
                .unwrap_or(&path)
                .to_string_lossy()
                .to_string();

            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| rel_path.clone());

            // Check if it's a known audio fragment
            let duration_ms = audio_fragments
                .iter()
                .find(|f| f.file_path == file_path_str)
                .and_then(|f| f.duration_ms);

            let created_at = metadata
                .modified()
                .or_else(|_| metadata.created())
                .map(|t| {
                    let datetime: chrono::DateTime<chrono::Utc> = t.into();
                    datetime.format("%Y-%m-%d %H:%M:%S").to_string()
                })
                .unwrap_or_default();

            resources.push(ResourceEntry {
                id: rel_path,
                name,
                resource_type: resource_type.to_string(),
                file_path: file_path_str,
                file_size: metadata.len(),
                created_at,
                duration_ms,
                section_id: None,
                section_title: None,
            });
        }
    }

    Ok(())
}

fn scan_dir_for_summary(
    dir: &std::path::Path,
    summary: &mut ResourceSummary,
) -> Result<(), AppError> {
    let entries = std::fs::read_dir(dir)
        .map_err(|e| AppError::FileSystem(format!("Failed to read directory: {}", e)))?;

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            scan_dir_for_summary(&path, summary)?;
        } else if path.is_file() {
            let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            let ext = path
                .extension()
                .map(|e| e.to_string_lossy().to_lowercase())
                .unwrap_or_default();

            summary.total_files += 1;
            summary.total_size_bytes += size;

            match ext.as_str() {
                "mp3" | "wav" | "ogg" | "m4a" | "flac" => {
                    summary.audio_count += 1;
                    summary.audio_size_bytes += size;
                }
                "mp4" | "webm" | "mov" => {
                    summary.video_count += 1;
                    summary.video_size_bytes += size;
                }
                _ => {
                    summary.other_count += 1;
                    summary.other_size_bytes += size;
                }
            }
        }
    }

    Ok(())
}
