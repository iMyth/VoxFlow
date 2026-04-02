use std::sync::Mutex;

use tauri::Emitter;
use tauri::Manager;

use crate::core::db::Database;
use crate::core::error::AppError;
use crate::core::models::MixProgress;

/// Detect which script lines are missing audio fragments.
///
/// Pure function: takes script line IDs and audio fragment line IDs,
/// returns the IDs of script lines that have no corresponding audio fragment.
pub fn detect_missing_audio(
    script_line_ids: &[String],
    audio_fragment_line_ids: &[String],
) -> Vec<String> {
    let audio_set: std::collections::HashSet<&str> = audio_fragment_line_ids
        .iter()
        .map(|s| s.as_str())
        .collect();

    script_line_ids
        .iter()
        .filter(|id| !audio_set.contains(id.as_str()))
        .cloned()
        .collect()
}

/// Build FFmpeg command arguments for mixing audio files, optionally with BGM.
///
/// Pure function: takes audio paths, optional BGM config, and output path.
/// Returns the complete list of FFmpeg command-line arguments.
///
/// Without BGM: uses concat demuxer to join audio files sequentially.
/// With BGM: concatenates voice tracks first, then uses amix filter to mix with BGM.
pub fn build_ffmpeg_args(
    audio_paths: &[String],
    bgm_path: Option<&str>,
    bgm_volume: f32,
    output_path: &str,
) -> Vec<String> {
    match bgm_path {
        None => {
            // Simple concat: use concat demuxer via pipe
            // ffmpeg -f concat -safe 0 -i <concat_file> -c copy <output>
            // We'll use a concat file approach; the caller writes the concat file.
            // Here we build args assuming a concat list file will be provided.
            //
            // Actually, for a pure function we build the full command.
            // Use multiple -i inputs and a filter_complex to concat them.
            let mut args = Vec::new();
            args.push("-y".to_string());

            for path in audio_paths {
                args.push("-i".to_string());
                args.push(path.clone());
            }

            let n = audio_paths.len();
            if n == 1 {
                // Single file, just copy
                args.push("-c".to_string());
                args.push("copy".to_string());
            } else {
                // Use concat filter
                let mut filter = String::new();
                for i in 0..n {
                    filter.push_str(&format!("[{}:a]", i));
                }
                filter.push_str(&format!("concat=n={}:v=0:a=1[out]", n));

                args.push("-filter_complex".to_string());
                args.push(filter);
                args.push("-map".to_string());
                args.push("[out]".to_string());
            }

            args.push(output_path.to_string());
            args
        }
        Some(bgm) => {
            // With BGM: concat voice tracks, then amix with BGM
            let mut args = Vec::new();
            args.push("-y".to_string());

            for path in audio_paths {
                args.push("-i".to_string());
                args.push(path.clone());
            }

            // Add BGM as last input
            args.push("-i".to_string());
            args.push(bgm.to_string());

            let n = audio_paths.len();
            let bgm_idx = n; // BGM is the last input

            let mut filter = String::new();

            // Concat all voice tracks
            if n == 1 {
                // Single voice track, use it directly
                filter.push_str(&format!(
                    "[0:a]volume=1.0[voice];[{}:a]volume={}[bgm];[voice][bgm]amix=inputs=2:duration=first:dropout_transition=2[out]",
                    bgm_idx, bgm_volume
                ));
            } else {
                // Multiple voice tracks: concat first, then mix with BGM
                for i in 0..n {
                    filter.push_str(&format!("[{}:a]", i));
                }
                filter.push_str(&format!(
                    "concat=n={}:v=0:a=1[voice];[{}:a]volume={}[bgm];[voice][bgm]amix=inputs=2:duration=first:dropout_transition=2[out]",
                    n, bgm_idx, bgm_volume
                ));
            }

            args.push("-filter_complex".to_string());
            args.push(filter);
            args.push("-map".to_string());
            args.push("[out]".to_string());
            args.push(output_path.to_string());
            args
        }
    }
}

#[tauri::command]
pub async fn export_audio_mix(
    app: tauri::AppHandle,
    db: tauri::State<'_, Mutex<Database>>,
    project_id: String,
    output_path: String,
    bgm_path: Option<String>,
    bgm_volume: f32,
) -> Result<String, AppError> {
    // Load audio fragments from database
    let fragments = {
        let db = db.lock().map_err(|e| AppError::Database(e.to_string()))?;
        db.list_audio_fragments(&project_id)?
    };

    if fragments.is_empty() {
        return Err(AppError::FFmpeg("No audio fragments found for this project".to_string()));
    }

    // Verify all audio fragment files exist
    let mut audio_paths: Vec<String> = Vec::new();
    for fragment in &fragments {
        let path = std::path::Path::new(&fragment.file_path);
        if !path.exists() {
            return Err(AppError::FileSystem(format!(
                "Audio fragment file not found: {}",
                fragment.file_path
            )));
        }
        audio_paths.push(fragment.file_path.clone());
    }

    // Verify BGM file exists if provided
    if let Some(ref bgm) = bgm_path {
        if !std::path::Path::new(bgm).exists() {
            return Err(AppError::FileSystem(format!(
                "BGM file not found: {}",
                bgm
            )));
        }
    }

    // Emit initial progress
    let _ = app.emit(
        "mix-progress",
        MixProgress {
            percent: 0.0,
            stage: "Preparing".to_string(),
        },
    );

    // Build FFmpeg command
    let ffmpeg_args = build_ffmpeg_args(
        &audio_paths,
        bgm_path.as_deref(),
        bgm_volume,
        &output_path,
    );

    let _ = app.emit(
        "mix-progress",
        MixProgress {
            percent: 10.0,
            stage: "Starting FFmpeg".to_string(),
        },
    );

    // Run FFmpeg subprocess
    let output = std::process::Command::new("ffmpeg")
        .args(&ffmpeg_args)
        .stderr(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .output()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                AppError::FFmpeg(
                    "FFmpeg not found. Please install FFmpeg and ensure it is in your PATH."
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
            stage: "Complete".to_string(),
        },
    );

    Ok(output_path)
}

#[tauri::command]
pub fn import_bgm(
    db: tauri::State<'_, Mutex<Database>>,
    app: tauri::AppHandle,
    project_id: String,
    source_path: String,
    name: String,
) -> Result<(), AppError> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::FileSystem(e.to_string()))?;

    let bgm_dir = app_data_dir
        .join("projects")
        .join(&project_id)
        .join("bgm");

    // Ensure bgm directory exists
    std::fs::create_dir_all(&bgm_dir).map_err(|e| {
        AppError::FileSystem(format!("Failed to create BGM directory: {}", e))
    })?;

    // Determine destination filename
    let source = std::path::Path::new(&source_path);
    let extension = source
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("mp3");
    let dest_filename = format!("{}.{}", name, extension);
    let dest_path = bgm_dir.join(&dest_filename);

    // Copy file
    std::fs::copy(&source_path, &dest_path).map_err(|e| {
        AppError::FileSystem(format!("Failed to copy BGM file: {}", e))
    })?;

    // Record in database
    let id = uuid::Uuid::new_v4().to_string();
    let file_path = dest_path.to_string_lossy().to_string();

    let db = db.lock().map_err(|e| AppError::Database(e.to_string()))?;
    db.insert_bgm(&id, &project_id, &file_path, &name)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- detect_missing_audio tests ----

    #[test]
    fn test_detect_missing_audio_all_present() {
        let script_ids = vec!["l1".to_string(), "l2".to_string(), "l3".to_string()];
        let audio_ids = vec!["l1".to_string(), "l2".to_string(), "l3".to_string()];
        let missing = detect_missing_audio(&script_ids, &audio_ids);
        assert!(missing.is_empty());
    }

    #[test]
    fn test_detect_missing_audio_some_missing() {
        let script_ids = vec!["l1".to_string(), "l2".to_string(), "l3".to_string()];
        let audio_ids = vec!["l1".to_string(), "l3".to_string()];
        let missing = detect_missing_audio(&script_ids, &audio_ids);
        assert_eq!(missing, vec!["l2".to_string()]);
    }

    #[test]
    fn test_detect_missing_audio_all_missing() {
        let script_ids = vec!["l1".to_string(), "l2".to_string()];
        let audio_ids: Vec<String> = vec![];
        let missing = detect_missing_audio(&script_ids, &audio_ids);
        assert_eq!(missing, vec!["l1".to_string(), "l2".to_string()]);
    }

    #[test]
    fn test_detect_missing_audio_empty_script() {
        let script_ids: Vec<String> = vec![];
        let audio_ids = vec!["l1".to_string()];
        let missing = detect_missing_audio(&script_ids, &audio_ids);
        assert!(missing.is_empty());
    }

    #[test]
    fn test_detect_missing_audio_preserves_order() {
        let script_ids = vec![
            "l3".to_string(),
            "l1".to_string(),
            "l2".to_string(),
        ];
        let audio_ids = vec!["l1".to_string()];
        let missing = detect_missing_audio(&script_ids, &audio_ids);
        assert_eq!(missing, vec!["l3".to_string(), "l2".to_string()]);
    }

    // ---- build_ffmpeg_args tests ----

    #[test]
    fn test_build_ffmpeg_args_single_file_no_bgm() {
        let paths = vec!["/tmp/a.mp3".to_string()];
        let args = build_ffmpeg_args(&paths, None, 0.3, "/tmp/out.mp3");

        assert!(args.contains(&"-y".to_string()));
        assert!(args.contains(&"-i".to_string()));
        assert!(args.contains(&"/tmp/a.mp3".to_string()));
        assert!(args.contains(&"-c".to_string()));
        assert!(args.contains(&"copy".to_string()));
        assert!(args.contains(&"/tmp/out.mp3".to_string()));
    }

    #[test]
    fn test_build_ffmpeg_args_multiple_files_no_bgm() {
        let paths = vec![
            "/tmp/a.mp3".to_string(),
            "/tmp/b.mp3".to_string(),
            "/tmp/c.mp3".to_string(),
        ];
        let args = build_ffmpeg_args(&paths, None, 0.3, "/tmp/out.mp3");

        assert!(args.contains(&"-y".to_string()));
        assert!(args.contains(&"/tmp/a.mp3".to_string()));
        assert!(args.contains(&"/tmp/b.mp3".to_string()));
        assert!(args.contains(&"/tmp/c.mp3".to_string()));
        assert!(args.contains(&"-filter_complex".to_string()));

        // Find the filter_complex value
        let fc_idx = args.iter().position(|a| a == "-filter_complex").unwrap();
        let filter = &args[fc_idx + 1];
        assert!(filter.contains("concat=n=3:v=0:a=1"));
        assert!(filter.contains("[out]"));
        assert!(args.contains(&"/tmp/out.mp3".to_string()));
    }

    #[test]
    fn test_build_ffmpeg_args_with_bgm() {
        let paths = vec!["/tmp/a.mp3".to_string(), "/tmp/b.mp3".to_string()];
        let args = build_ffmpeg_args(&paths, Some("/tmp/bgm.mp3"), 0.3, "/tmp/out.mp3");

        assert!(args.contains(&"-y".to_string()));
        assert!(args.contains(&"/tmp/a.mp3".to_string()));
        assert!(args.contains(&"/tmp/b.mp3".to_string()));
        assert!(args.contains(&"/tmp/bgm.mp3".to_string()));
        assert!(args.contains(&"-filter_complex".to_string()));

        let fc_idx = args.iter().position(|a| a == "-filter_complex").unwrap();
        let filter = &args[fc_idx + 1];
        assert!(filter.contains("amix"));
        assert!(filter.contains("volume=0.3"));
        assert!(filter.contains("concat=n=2"));
        assert!(args.contains(&"/tmp/out.mp3".to_string()));
    }

    #[test]
    fn test_build_ffmpeg_args_single_file_with_bgm() {
        let paths = vec!["/tmp/a.mp3".to_string()];
        let args = build_ffmpeg_args(&paths, Some("/tmp/bgm.mp3"), 0.5, "/tmp/out.mp3");

        assert!(args.contains(&"/tmp/bgm.mp3".to_string()));
        let fc_idx = args.iter().position(|a| a == "-filter_complex").unwrap();
        let filter = &args[fc_idx + 1];
        assert!(filter.contains("amix"));
        assert!(filter.contains("volume=0.5"));
    }

    #[test]
    fn test_build_ffmpeg_args_output_is_last() {
        let paths = vec!["/tmp/a.mp3".to_string()];
        let args = build_ffmpeg_args(&paths, None, 0.3, "/tmp/out.mp3");
        assert_eq!(args.last().unwrap(), "/tmp/out.mp3");
    }

    #[test]
    fn test_build_ffmpeg_args_all_inputs_present() {
        let paths = vec![
            "/tmp/1.mp3".to_string(),
            "/tmp/2.mp3".to_string(),
            "/tmp/3.mp3".to_string(),
            "/tmp/4.mp3".to_string(),
        ];
        let args = build_ffmpeg_args(&paths, None, 0.3, "/tmp/out.mp3");

        for p in &paths {
            assert!(args.contains(p), "Expected {} in args", p);
        }
    }
}
