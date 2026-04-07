use std::sync::Mutex;
use std::sync::mpsc;

use tauri::Emitter;
use tauri::Manager;

use crate::core::db::Database;
use crate::core::error::AppError;
use crate::core::models::MixProgress;
use log::error;

// --- Audio playback via rodio on a dedicated thread ---

use cpal::traits::HostTrait;

enum AudioCommand {
    Play(String, mpsc::Sender<Result<(), String>>, Option<tauri::AppHandle>),
    Stop,
    Shutdown,
}

pub struct AudioPlayer {
    tx: mpsc::Sender<AudioCommand>,
}

unsafe impl Sync for AudioPlayer {}

impl AudioPlayer {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel::<AudioCommand>();

        std::thread::spawn(move || {
            let mut current_sink: Option<std::sync::Arc<rodio::Sink>> = None;
            let mut current_stream: Option<rodio::OutputStream> = None;

            while let Ok(cmd) = rx.recv() {
                match cmd {
                    AudioCommand::Play(path, reply, app_handle) => {
                        if let Some(sink) = current_sink.take() {
                            sink.stop();
                        }
                        current_stream.take();

                        let result = (|| -> Result<std::sync::Arc<rodio::Sink>, String> {
                            let host = cpal::default_host();
                            let device = host
                                .default_output_device()
                                .ok_or("No default audio output device found")?;

                            let (stream, stream_handle) =
                                rodio::OutputStream::try_from_device(&device)
                                    .map_err(|e| format!("Failed to open audio device: {}", e))?;

                            let sink = rodio::Sink::try_new(&stream_handle)
                                .map_err(|e| format!("Failed to create audio sink: {}", e))?;

                            let file = std::fs::File::open(&path)
                                .map_err(|e| format!("Failed to open file {}: {}", path, e))?;

                            let reader = std::io::BufReader::new(file);
                            let source = rodio::Decoder::new(reader)
                                .map_err(|e| format!("Failed to decode audio: {}", e))?;

                            sink.append(source);
                            let sink = std::sync::Arc::new(sink);
                            current_stream = Some(stream);
                            Ok(sink)
                        })();

                        match result {
                            Ok(sink) => {
                                current_sink = Some(sink.clone());
                                let _ = reply.send(Ok(()));

                                // Spawn a watcher thread that emits audio-finished when done
                                if let Some(app) = app_handle {
                                    std::thread::spawn(move || {
                                        sink.sleep_until_end();
                                        let _ = app.emit("audio-finished", ());
                                    });
                                }
                            }
                            Err(e) => {
                                let _ = reply.send(Err(e));
                            }
                        }
                    }
                    AudioCommand::Stop => {
                        if let Some(sink) = current_sink.take() {
                            sink.stop();
                        }
                        current_stream.take();
                    }
                    AudioCommand::Shutdown => break,
                }
            }
        });

        AudioPlayer { tx }
    }

    pub fn play(&self, file_path: &str, app: Option<tauri::AppHandle>) -> Result<(), String> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.tx
            .send(AudioCommand::Play(file_path.to_string(), reply_tx, app))
            .map_err(|e| format!("Audio thread gone: {}", e))?;
        reply_rx
            .recv()
            .map_err(|e| format!("Audio thread reply failed: {}", e))?
    }

    pub fn stop(&self) {
        let _ = self.tx.send(AudioCommand::Stop);
    }
}

impl Drop for AudioPlayer {
    fn drop(&mut self) {
        let _ = self.tx.send(AudioCommand::Shutdown);
    }
}

#[tauri::command]
pub fn play_audio(
    app: tauri::AppHandle,
    state: tauri::State<'_, AudioPlayer>,
    file_path: String,
) -> Result<(), AppError> {
    state.play(&file_path, Some(app)).map_err(AppError::Audio)
}

#[tauri::command]
pub fn stop_audio(
    state: tauri::State<'_, AudioPlayer>,
) -> Result<(), AppError> {
    state.stop();
    Ok(())
}

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
/// `gaps_ms` is a slice of per-line gap durations (in ms) after each audio clip.
/// gaps_ms[i] is the silence after audio_paths[i]. The last element is ignored (no gap after last clip).
/// If gaps_ms is empty, no gaps are inserted.
pub fn build_ffmpeg_args(
    audio_paths: &[String],
    bgm_path: Option<&str>,
    bgm_volume: f32,
    gaps_ms: &[i32],
    output_path: &str,
) -> Vec<String> {
    let n = audio_paths.len();
    let mut args = Vec::new();
    args.push("-y".to_string());

    for path in audio_paths {
        args.push("-i".to_string());
        args.push(path.clone());
    }

    if let Some(bgm) = bgm_path {
        args.push("-i".to_string());
        args.push(bgm.to_string());
    }

    if n == 1 && bgm_path.is_none() && (gaps_ms.is_empty() || gaps_ms[0] == 0) {
        args.push("-c".to_string());
        args.push("copy".to_string());
        args.push(output_path.to_string());
        return args;
    }

    let mut filter = String::new();

    // Check if any gap > 0 exists between clips
    let has_gaps = n > 1 && !gaps_ms.is_empty() && gaps_ms.iter().take(n - 1).any(|&g| g > 0);

    if has_gaps {
        // Generate unique silence pads for each gap
        let mut gap_count = 0;
        for i in 0..(n - 1) {
            let gap = gaps_ms.get(i).copied().unwrap_or(0);
            if gap > 0 {
                let gap_sec = gap as f64 / 1000.0;
                filter.push_str(&format!(
                    "anullsrc=r=44100:cl=stereo[sil{s}];[sil{s}]atrim=0:{dur}[gap{s}];",
                    s = i, dur = gap_sec
                ));
                gap_count += 1;
            }
        }
        // Interleave audio and gaps
        let total_segments = n + gap_count;
        for i in 0..n {
            filter.push_str(&format!("[{}:a]", i));
            if i < n - 1 {
                let gap = gaps_ms.get(i).copied().unwrap_or(0);
                if gap > 0 {
                    filter.push_str(&format!("[gap{}]", i));
                }
            }
        }
        filter.push_str(&format!("concat=n={}:v=0:a=1[voice]", total_segments));
    } else {
        for i in 0..n {
            filter.push_str(&format!("[{}:a]", i));
        }
        if n > 1 {
            filter.push_str(&format!("concat=n={}:v=0:a=1[voice]", n));
        } else {
            filter.push_str("acopy[voice]");
        }
    }

    if bgm_path.is_some() {
        let bgm_idx = n;
        filter.push_str(&format!(
            ";[{}:a]volume={}[bgm];[voice][bgm]amix=inputs=2:duration=first:dropout_transition=2[out]",
            bgm_idx, bgm_volume
        ));
        args.push("-filter_complex".to_string());
        args.push(filter);
        args.push("-map".to_string());
        args.push("[out]".to_string());
    } else {
        args.push("-filter_complex".to_string());
        args.push(filter);
        args.push("-map".to_string());
        args.push("[voice]".to_string());
    }

    args.push(output_path.to_string());
    args
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
    // Load script lines (ordered) and audio fragments from database
    let (script_lines, fragments) = {
        let db = db.lock().map_err(|e| AppError::Database(e.to_string()))?;
        let lines = db.load_script(&project_id)?;
        let frags = db.list_audio_fragments(&project_id)?;
        (lines, frags)
    };

    if fragments.is_empty() {
        return Err(AppError::FFmpeg("No audio fragments found for this project".to_string()));
    }

    // Build ordered audio paths and per-line gaps based on script line order
    let frag_map: std::collections::HashMap<&str, &crate::core::models::AudioFragment> =
        fragments.iter().map(|f| (f.line_id.as_str(), f)).collect();

    let mut audio_paths: Vec<String> = Vec::new();
    let mut gaps_ms: Vec<i32> = Vec::new();

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
        }
    }

    if audio_paths.is_empty() {
        return Err(AppError::FFmpeg("No audio fragments found for this project".to_string()));
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
            stage: "Preparing".to_string(),
        },
    );

    // Build FFmpeg command
    let ffmpeg_args = build_ffmpeg_args(
        &audio_paths,
        bgm_path.as_deref(),
        bgm_volume,
        &gaps_ms,
        &output_path,
    );

    let _ = app.emit(
        "mix-progress",
        MixProgress {
            percent: 10.0,
            stage: "Starting FFmpeg".to_string(),
        },
    );

    // Run FFmpeg subprocess — try common macOS paths if not in PATH
    let ffmpeg_bin = find_ffmpeg();

    let output = std::process::Command::new(&ffmpeg_bin)
        .args(&ffmpeg_args)
        .stderr(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .output()
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
            stage: "Complete".to_string(),
        },
    );

    Ok(output_path)
}

/// Find ffmpeg binary — check common macOS Homebrew paths if not in PATH.
fn find_ffmpeg() -> String {
    let candidates = [
        "ffmpeg",
        "/opt/homebrew/bin/ffmpeg",
        "/usr/local/bin/ffmpeg",
    ];
    for candidate in &candidates {
        if std::path::Path::new(candidate).exists() || candidate == &"ffmpeg" {
            // "ffmpeg" without path will be resolved by the OS via PATH
            if candidate != &"ffmpeg" {
                return candidate.to_string();
            }
        }
    }
    "ffmpeg".to_string()
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
        let args = build_ffmpeg_args(&paths, None, 0.3, &[], "/tmp/out.mp3");

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
        let args = build_ffmpeg_args(&paths, None, 0.3, &[], "/tmp/out.mp3");

        assert!(args.contains(&"-y".to_string()));
        assert!(args.contains(&"/tmp/a.mp3".to_string()));
        assert!(args.contains(&"/tmp/b.mp3".to_string()));
        assert!(args.contains(&"/tmp/c.mp3".to_string()));
        assert!(args.contains(&"-filter_complex".to_string()));

        // Find the filter_complex value
        let fc_idx = args.iter().position(|a| a == "-filter_complex").unwrap();
        let filter = &args[fc_idx + 1];
        assert!(filter.contains("concat=n=3:v=0:a=1"));
        assert!(filter.contains("[voice]"));
        assert!(args.contains(&"/tmp/out.mp3".to_string()));
    }

    #[test]
    fn test_build_ffmpeg_args_with_bgm() {
        let paths = vec!["/tmp/a.mp3".to_string(), "/tmp/b.mp3".to_string()];
        let args = build_ffmpeg_args(&paths, Some("/tmp/bgm.mp3"), 0.3, &[], "/tmp/out.mp3");

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
        let args = build_ffmpeg_args(&paths, Some("/tmp/bgm.mp3"), 0.5, &[], "/tmp/out.mp3");

        assert!(args.contains(&"/tmp/bgm.mp3".to_string()));
        let fc_idx = args.iter().position(|a| a == "-filter_complex").unwrap();
        let filter = &args[fc_idx + 1];
        assert!(filter.contains("amix"));
        assert!(filter.contains("volume=0.5"));
    }

    #[test]
    fn test_build_ffmpeg_args_output_is_last() {
        let paths = vec!["/tmp/a.mp3".to_string()];
        let args = build_ffmpeg_args(&paths, None, 0.3, &[], "/tmp/out.mp3");
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
        let args = build_ffmpeg_args(&paths, None, 0.3, &[], "/tmp/out.mp3");

        for p in &paths {
            assert!(args.contains(p), "Expected {} in args", p);
        }
    }
}
