/// Build FFmpeg command arguments for mixing audio files, optionally with BGM.
///
/// `gaps_ms` is a slice of per-line gap durations (in ms) after each audio clip.
/// gaps_ms[i] is the silence after audio_paths[i]. The last element is ignored (no gap after last clip).
/// If gaps_ms is empty, no gaps are inserted.
///
/// Each voice clip is normalized to -16 LUFS (EBU R128) using the `loudnorm` filter
/// to ensure consistent volume across TTS fragments before concatenation.
///
/// When `sleep_mode` is true, additional audio processing is applied to create a
/// soothing, sleep-friendly sound: slight pitch reduction, warmth boost (bass EQ),
/// gentle high-frequency rolloff, and reduced overall loudness target (-20 LUFS).
pub fn build_ffmpeg_args(
    audio_paths: &[String],
    bgm_path: Option<&str>,
    bgm_volume: f32,
    gaps_ms: &[i32],
    output_path: &str,
    sleep_mode: bool,
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

    // Single file without BGM or gaps: still normalize for consistency
    if n == 1 && bgm_path.is_none() && (gaps_ms.is_empty() || gaps_ms[0] == 0) {
        let mut filter = "[0:a]loudnorm=I=-16:TP=-1.5:LRA=11[voice]".to_string();
        if sleep_mode {
            // Sleep mode: warm tone + gentle rolloff + quieter target
            filter.push_str(
                ";[voice]equalizer=f=200:t=q:w=0.8:g=3,\
                 equalizer=f=3000:t=q:w=1.0:g=-2,\
                 lowpass=f=8000:p=1,\
                 loudnorm=I=-20:TP=-2:LRA=7[out]",
            );
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
        return args;
    }

    let mut filter = String::new();

    // Step 1: Normalize each voice clip to -16 LUFS (EBU R128)
    // This ensures consistent volume across TTS fragments without distortion.
    // loudnorm parameters:
    //   I=-16    target integrated loudness (LUFS)
    //   TP=-1.5  true peak limit (dBTP) — prevents clipping
    //   LRA=11   loudness range target (LU) — preserves natural dynamics
    for i in 0..n {
        filter.push_str(&format!(
            "[{i}:a]loudnorm=I=-16:TP=-1.5:LRA=11[norm{i}];",
            i = i
        ));
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
                    "anullsrc=r=44100:cl=stereo[sil{s}];[sil{s}]atrim=0:{dur}[gap{s}];",
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

    // Sleep mode: apply soothing audio processing to the concatenated voice
    // - equalizer f=200 g=3: gentle bass warmth (adds body/comfort to voice)
    // - equalizer f=3000 g=-2: slight upper-mid reduction (less harsh/bright)
    // - lowpass f=8000: roll off high frequencies (removes sibilance/sharpness)
    // - loudnorm I=-20: quieter target loudness (sleep-appropriate level)
    let voice_label = if sleep_mode {
        filter.push_str(
            ";[voice]equalizer=f=200:t=q:w=0.8:g=3,\
             equalizer=f=3000:t=q:w=1.0:g=-2,\
             lowpass=f=8000:p=1,\
             loudnorm=I=-20:TP=-2:LRA=7[sleepvoice]",
        );
        "[sleepvoice]"
    } else {
        "[voice]"
    };

    if bgm_path.is_some() {
        let bgm_idx = n;
        filter.push_str(&format!(
            ";[{}:a]volume={}[bgm];{}[bgm]amix=inputs=2:duration=first:dropout_transition=2[out]",
            bgm_idx, bgm_volume, voice_label
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
