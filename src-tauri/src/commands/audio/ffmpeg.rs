/// Build FFmpeg command arguments for mixing audio files, optionally with BGM.
///
/// `gaps_ms` is a slice of per-line gap durations (in ms) after each audio clip.
/// gaps_ms[i] is the silence after audio_paths[i]. The last element is ignored (no gap after last clip).
/// If gaps_ms is empty, no gaps are inserted.
///
/// Each voice clip is normalized to -16 LUFS (EBU R128) using the `loudnorm` filter
/// to ensure consistent volume across TTS fragments before concatenation.
///
/// When `sleep_mode` is true, level speech using measured fixed gains. The caller masters the complete mix
/// (including BGM) with the shared two-pass sleep processor.
pub fn build_ffmpeg_args(
    audio_paths: &[String],
    bgm_path: Option<&str>,
    bgm_volume: f32,
    gaps_ms: &[i32],
    output_path: &str,
    sleep_mode: bool,
    gains_db: &[f64],
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

    let mut filter = String::new();
    for i in 0..n {
        let leveling = if sleep_mode {
            format!("volume={:.4}dB", gains_db.get(i).copied().unwrap_or(0.0))
        } else {
            "loudnorm=I=-16:TP=-1.5:LRA=11".to_string()
        };
        filter.push_str(&format!("[{i}:a]{leveling},aresample=48000,aformat=sample_fmts=fltp:channel_layouts=stereo,asetpts=PTS-STARTPTS[norm{i}];"));
    }

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
                    "anullsrc=r=48000:cl=stereo[sil{s}];[sil{s}]atrim=0:{dur}[gap{s}];",
                    s = i,
                    dur = gap_sec
                ));
                gap_count += 1;
            }
        }
        // Interleave normalized audio and gaps
        let total_segments = n + gap_count;
        for i in 0..n {
            filter.push_str(&format!("[norm{}]", i));
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
            filter.push_str(&format!("[norm{}]", i));
        }
        if n > 1 {
            filter.push_str(&format!("concat=n={}:v=0:a=1[voice]", n));
        } else {
            filter.push_str("acopy[voice]");
        }
    }

    let voice_label = "[voice]";

    if bgm_path.is_some() {
        let bgm_idx = n;
        let mixing = if sleep_mode {
            "dropout_transition=0:normalize=0"
        } else {
            "dropout_transition=2"
        };
        filter.push_str(&format!(
            ";[{}:a]volume={}[bgm];{}[bgm]amix=inputs=2:duration=first:{}[out]",
            bgm_idx, bgm_volume, voice_label, mixing
        ));
        args.push("-filter_complex".to_string());
        args.push(filter);
        args.push("-map".to_string());
        args.push("[out]".to_string());
    } else {
        args.push("-filter_complex".to_string());
        args.push(filter);
        args.push("-map".to_string());
        args.push(voice_label.to_string());
    }

    if sleep_mode {
        // Floating-point intermediate avoids clipping before final mastering.
        args.extend(["-c:a".to_string(), "pcm_f32le".to_string()]);
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
