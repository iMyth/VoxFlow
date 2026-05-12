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

/// Find ffmpeg binary — check common macOS paths first, then fall back to shell resolution.
///
/// Tauri apps do not inherit the user's shell PATH (e.g. /opt/homebrew/bin is missing),
/// so we must probe known absolute locations before trying a shell `which` lookup.
pub fn find_ffmpeg() -> String {
    // 1. Check well-known absolute paths (covers Homebrew Apple Silicon, Intel, MacPorts)
    let candidates = [
        "/opt/homebrew/bin/ffmpeg", // Homebrew on Apple Silicon (M1/M2/M3)
        "/usr/local/bin/ffmpeg",    // Homebrew on Intel Mac
        "/opt/local/bin/ffmpeg",    // MacPorts
        "/usr/bin/ffmpeg",          // System / manual install
    ];
    for candidate in &candidates {
        if std::path::Path::new(candidate).exists() {
            return candidate.to_string();
        }
    }

    // 2. Ask the shell — inherits the user's full PATH (including Homebrew shims, nix, etc.)
    if let Ok(output) = std::process::Command::new("/bin/sh")
        .args(["-c", "which ffmpeg"])
        .output()
    {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path.is_empty() {
                return path;
            }
        }
    }

    // 3. Last resort — let the OS try via whatever PATH it does have
    "ffmpeg".to_string()
}

// ─── Raw RGBA → H.264 video encoder ────────────────────────────────────────

/// Encapsulates an FFmpeg subprocess that reads raw RGBA frames from stdin
/// and encodes to H.264 + AAC audio.
///
/// Created via [`spawn_video_encoder`]. Call [`finish`](FfmpegVideoEncoder::finish)
/// when all frames have been sent to close the pipe and wait for encoding.
pub struct FfmpegVideoEncoder {
    child: Option<std::process::Child>,
}

impl FfmpegVideoEncoder {
    /// Wait for encoding to finish and check for errors.
    pub fn finish(self) -> Result<(), crate::core::error::AppError> {
        let child = self.child.ok_or_else(|| {
            crate::core::error::AppError::FFmpeg("Encoder already finished".to_string())
        })?;
        let output = child.wait_with_output().map_err(|e| {
            crate::core::error::AppError::FFmpeg(format!("FFmpeg wait failed: {}", e))
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(crate::core::error::AppError::FFmpeg(format!(
                "FFmpeg encoding failed: {}",
                stderr.chars().take(500).collect::<String>()
            )));
        }

        Ok(())
    }
}

/// Spawn an FFmpeg subprocess that reads raw RGBA frames from stdin and encodes video + AAC audio.
///
/// Automatically picks the best encoder and quality settings for the current platform:
/// - **macOS Apple Silicon (M1/M2/M3/M4)**: `hevc_videotoolbox` (HEVC hardware encoding, ~40% smaller)
/// - **Other platforms**: `libx264` with `slow` preset and CRF 26
///
/// Returns `(encoder, tx)` where:
/// - `tx` sends raw RGBA frames (`width * height * 4` bytes, RGBA order)
/// - `encoder` must be consumed via [`FfmpegVideoEncoder::finish`] after the channel is dropped
///
/// Frames should be rendered at `render_width x render_height`; FFmpeg will upscale
/// to `output_width x output_height` using lanczos.
pub fn spawn_video_encoder(
    render_width: u32,
    render_height: u32,
    output_width: u32,
    output_height: u32,
    fps: u32,
    audio_path: &str,
    output_path: &str,
) -> Result<(FfmpegVideoEncoder, std::sync::mpsc::SyncSender<Vec<u8>>), crate::core::error::AppError> {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let scale_filter = format!("scale={}:{}:flags=lanczos", output_width, output_height);
    let ffmpeg_bin = find_ffmpeg();
    let render_size = format!("{}x{}", render_width, render_height);
    let fps_str = fps.to_string();
    let mut args: Vec<String> = vec![
        "-y".into(),
        "-f".into(), "rawvideo".into(),
        "-pixel_format".into(), "rgba".into(),
        "-video_size".into(), render_size,
        "-framerate".into(), fps_str,
        "-i".into(), "pipe:0".into(),
        "-i".into(), audio_path.into(),
        "-vf".into(), scale_filter,
    ];

    if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        // Apple Silicon: HEVC hardware encoder via VideoToolbox
        // ~40% smaller than H.264 at same quality, zero CPU overhead.
        // -q:v 1-100 (lower = better). 55 gives excellent quality at small size.
        args.extend_from_slice(&[
            "-c:v".into(), "hevc_videotoolbox".into(),
            "-q:v".into(), "55".into(),
            "-tag:v".into(), "hvc1".into(), // Required for YouTube/Apple compatibility
            "-allow_sw".into(), "1".into(),
        ]);
    } else {
        // Intel Mac / Windows / Linux: software x264 (libx265 is too slow)
        args.extend_from_slice(&[
            "-c:v".into(), "libx264".into(),
            "-preset".into(), "slow".into(),
            "-tune".into(), "stillimage".into(),
            "-crf".into(), "26".into(),
        ]);
    }

    args.extend_from_slice(&[
        "-pix_fmt".into(), "yuv420p".into(),
        "-c:a".into(), "aac".into(),
        "-b:a".into(), "192k".into(),
        "-shortest".into(),
        output_path.into(),
    ]);

    let mut child = Command::new(&ffmpeg_bin)
        .args(&args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                crate::core::error::AppError::FFmpeg("FFmpeg not found. Please install FFmpeg.".to_string())
            } else {
                crate::core::error::AppError::FFmpeg(format!("Failed to start FFmpeg: {}", e))
            }
        })?;

    let mut stdin = child.stdin.take().ok_or_else(|| {
        crate::core::error::AppError::FFmpeg("Failed to open FFmpeg stdin".to_string())
    })?;

    let (tx, rx) = std::sync::mpsc::sync_channel::<Vec<u8>>(4);

    std::thread::spawn(move || {
        for frame_data in rx {
            if stdin.write_all(&frame_data).is_err() {
                break;
            }
        }
        drop(stdin);
    });

    Ok((FfmpegVideoEncoder { child: Some(child) }, tx))
}
