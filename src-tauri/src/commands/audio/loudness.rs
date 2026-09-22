//! Shared duration-preserving speech leveling and two-pass sleep mastering.
use super::ffmpeg::find_ffmpeg;
use crate::core::error::AppError;
use std::path::{Path, PathBuf};
use tokio::process::Command;

pub const SLEEP_TONE: &str = "highpass=f=60,equalizer=f=3000:t=q:w=1:g=-1.5,lowpass=f=8000:p=1,acompressor=threshold=0.0631:ratio=2:attack=20:release=250:makeup=1";
const TARGET: &str = "loudnorm=I=-21:TP=-3:LRA=5";

pub struct TemporaryMedia(pub PathBuf);
impl TemporaryMedia {
    pub fn beside(output: &Path, extension: &str) -> Self {
        Self(output.with_file_name(format!(".voxflow-{}.{}", uuid::Uuid::new_v4(), extension)))
    }
}
impl Drop for TemporaryMedia {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

pub async fn run(args: &[String]) -> Result<std::process::Output, AppError> {
    let mut command = Command::new(find_ffmpeg());
    command
        .args(["-hide_banner", "-nostdin", "-nostats"])
        .args(args)
        .kill_on_drop(true);
    let output = tokio::time::timeout(std::time::Duration::from_secs(1800), command.output())
        .await
        .map_err(|_| AppError::FFmpeg("Audio processing timed out".into()))?
        .map_err(|e| AppError::FFmpeg(e.to_string()))?;
    if !output.status.success() {
        return Err(AppError::FFmpeg(
            String::from_utf8_lossy(&output.stderr).into_owned(),
        ));
    }
    Ok(output)
}

#[derive(Debug)]
struct Measurement {
    integrated: f64,
    peak: f64,
    range: f64,
    threshold: f64,
    offset: f64,
}
fn parse_measurement(log: &str) -> Result<Measurement, AppError> {
    let start = log
        .rfind('{')
        .ok_or_else(|| AppError::FFmpeg("Missing loudness measurement".into()))?;
    let end = log[start..].find('}').unwrap_or(log.len() - start - 1) + start;
    let value: serde_json::Value =
        serde_json::from_str(&log[start..=end]).map_err(|e| AppError::FFmpeg(e.to_string()))?;
    let get = |key: &str| -> Result<f64, AppError> {
        value[key]
            .as_str()
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| AppError::FFmpeg(format!("Invalid loudness field: {key}")))
    };
    Ok(Measurement {
        integrated: get("input_i")?,
        peak: get("input_tp")?,
        range: get("input_lra")?,
        threshold: get("input_thresh")?,
        offset: get("target_offset")?,
    })
}
async fn measure(path: &str, prefix: &str) -> Result<Measurement, AppError> {
    let filter = if prefix.is_empty() {
        format!("{TARGET}:print_format=json")
    } else {
        format!("{prefix},{TARGET}:print_format=json")
    };
    let args = [
        "-i", path, "-map", "0:a:0", "-af", &filter, "-f", "null", "-",
    ]
    .map(str::to_string);
    parse_measurement(&String::from_utf8_lossy(&run(&args).await?.stderr))
}
fn gain_db(m: &Measurement) -> f64 {
    // Silence / very quiet material and short unmeasurable clips are never boosted.
    if !m.integrated.is_finite() || !m.peak.is_finite() || m.integrated < -50.0 {
        return 0.0;
    }
    (-21.0 - m.integrated).clamp(-18.0, 9.0).min(-3.0 - m.peak)
}
/// Analyze sequentially to avoid starting one FFmpeg process per line at once.
pub async fn clip_gains(paths: &[String]) -> Result<Vec<f64>, AppError> {
    let mut gains = Vec::with_capacity(paths.len());
    for path in paths {
        gains.push(gain_db(&measure(path, "").await?));
    }
    Ok(gains)
}

/// Measure the actual processed mix, then apply the same processing with measured
/// normalization parameters. No tempo or pitch changes; video is stream-copied.
pub async fn master_sleep(input: &Path, output: &Path, video: bool) -> Result<(), AppError> {
    let input_str = input.to_string_lossy();
    let m = measure(&input_str, SLEEP_TONE).await?;
    let normalization = if [m.integrated, m.peak, m.range, m.threshold, m.offset]
        .iter()
        .all(|n| n.is_finite())
    {
        format!("{TARGET}:measured_I={}:measured_TP={}:measured_LRA={}:measured_thresh={}:offset={}:linear=true", m.integrated, m.peak, m.range, m.threshold, m.offset)
    } else {
        // Silent inputs have -inf measurements, which are not valid filter options.
        "anull".to_string()
    };
    let filter = format!("{SLEEP_TONE},{normalization},aresample=48000");
    let extension = output
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or(if video { "mp4" } else { "mp3" });
    let temporary = TemporaryMedia::beside(output, extension);
    let mut args = vec!["-y".into(), "-i".into(), input_str.into_owned()];
    if video {
        args.extend(["-map", "0:v:0", "-c:v", "copy", "-c:a", "aac"].map(str::to_string));
    }
    args.extend(["-map", "0:a:0", "-af", &filter, "-ar", "48000"].map(str::to_string));
    args.push(temporary.0.to_string_lossy().into_owned());
    run(&args).await?;
    std::fs::rename(&temporary.0, output).map_err(|e| AppError::FileSystem(e.to_string()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn reading(i: f64, peak: f64) -> Measurement {
        Measurement {
            integrated: i,
            peak,
            range: 1.0,
            threshold: -40.0,
            offset: 0.0,
        }
    }
    #[test]
    fn gain_is_bounded_and_does_not_boost_silence() {
        assert_eq!(gain_db(&reading(-35.0, -20.0)), 9.0);
        assert_eq!(gain_db(&reading(-30.0, -4.0)), 1.0);
        assert_eq!(gain_db(&reading(-10.0, -1.0)), -11.0);
        assert_eq!(gain_db(&reading(-60.0, -40.0)), 0.0);
        assert_eq!(gain_db(&reading(f64::NEG_INFINITY, f64::NEG_INFINITY)), 0.0);
    }
    #[tokio::test]
    #[ignore = "requires installed FFmpeg and FFprobe"]
    async fn sleep_pipeline_levels_clips_preserves_gaps_and_duration() {
        let dir = tempfile::tempdir().unwrap();
        let mut paths = Vec::new();
        for (i, volume) in [0.5, 1.0, 2.0].iter().enumerate() {
            let path = dir.path().join(format!("clip{i}.wav"));
            let source = format!("sine=frequency=440:sample_rate=48000:duration=3,volume={volume}");
            run(&["-y", "-f", "lavfi", "-i", &source, path.to_str().unwrap()].map(str::to_string))
                .await
                .unwrap();
            paths.push(path.to_string_lossy().into_owned());
        }
        let gains = clip_gains(&paths).await.unwrap();
        assert!((gains[0] - gains[2]) > 10.0, "{gains:?}");
        let raw = dir.path().join("mix.wav");
        let args = super::super::ffmpeg::build_ffmpeg_args(
            &paths,
            None,
            0.1,
            &[500, 500, 0],
            raw.to_str().unwrap(),
            true,
            &gains,
        );
        run(&args).await.unwrap();
        let final_path = dir.path().join("sleep.wav");
        master_sleep(&raw, &final_path, false).await.unwrap();
        let measured = measure(final_path.to_str().unwrap(), "").await.unwrap();
        assert!((measured.integrated + 21.0).abs() < 1.0, "{measured:?}");
        assert!(measured.peak <= -2.9, "{measured:?}");
        let pcm = run(&[
            "-i",
            final_path.to_str().unwrap(),
            "-ac",
            "1",
            "-ar",
            "48000",
            "-f",
            "f32le",
            "-",
        ]
        .map(str::to_string))
        .await
        .unwrap()
        .stdout;
        let samples: Vec<f64> = pcm
            .chunks_exact(4)
            .map(|b| f32::from_le_bytes(b.try_into().unwrap()) as f64)
            .collect();
        assert!((samples.len() as f64 / 48000.0 - 10.0).abs() < 0.02);
        let rms = |start: f64, end: f64| {
            let values = &samples[(start * 48000.0) as usize..(end * 48000.0) as usize];
            (values.iter().map(|v| v * v).sum::<f64>() / values.len() as f64).sqrt()
        };
        let levels = [rms(1.0, 2.0), rms(4.5, 5.5), rms(8.0, 9.0)];
        let max = levels.iter().copied().fold(0.0, f64::max);
        let min = levels.iter().copied().fold(f64::INFINITY, f64::min);
        assert!(20.0 * (max / min).log10() < 1.0, "{levels:?}");
        assert!(rms(3.2, 3.4) < 0.0001, "gap must remain quiet");

        // Silent content must remain silent, without invalid -inf filter options.
        let silence = dir.path().join("silence.wav");
        run(&[
            "-f",
            "lavfi",
            "-i",
            "anullsrc=r=48000:cl=mono",
            "-t",
            "1",
            silence.to_str().unwrap(),
        ]
        .map(str::to_string))
        .await
        .unwrap();
        assert_eq!(
            clip_gains(&[silence.to_string_lossy().into_owned()])
                .await
                .unwrap(),
            vec![0.0]
        );
        master_sleep(&silence, &silence, false).await.unwrap();
        assert!(!measure(silence.to_str().unwrap(), "")
            .await
            .unwrap()
            .integrated
            .is_finite());
    }
    #[tokio::test]
    #[ignore = "requires installed FFmpeg and FFprobe"]
    async fn video_sleep_export_preserves_video_and_audio_timing() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source.mp4");
        run(&[
            "-f",
            "lavfi",
            "-i",
            "color=black:s=32x32:r=30:d=3",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:sample_rate=48000:duration=3",
            "-c:v",
            "libx264",
            "-c:a",
            "aac",
            source.to_str().unwrap(),
        ]
        .map(str::to_string))
        .await
        .unwrap();
        let output = dir.path().join("sleep.mp4");
        let section = super::super::hyperframes::section_types::SectionVideoFile {
            section_id: "test".into(),
            section_order: 0,
            file_path: source.to_string_lossy().into_owned(),
            duration_ms: 3000,
        };
        super::super::hyperframes::video_merger::merge_videos(
            &[section],
            &output,
            0,
            true,
            |_, _| {},
        )
        .await
        .unwrap();
        let probe = std::process::Command::new(find_ffmpeg().replace("ffmpeg", "ffprobe"))
            .args([
                "-v",
                "error",
                "-show_entries",
                "stream=codec_type,duration",
                "-of",
                "json",
            ])
            .arg(&output)
            .output()
            .unwrap();
        assert!(probe.status.success());
        let data: serde_json::Value = serde_json::from_slice(&probe.stdout).unwrap();
        let streams = data["streams"].as_array().unwrap();
        assert_eq!(streams.len(), 2);
        for stream in streams {
            let duration: f64 = stream["duration"].as_str().unwrap().parse().unwrap();
            assert!((duration - 3.0).abs() < 0.04, "{stream}");
        }
        let hashes = |path: &Path| {
            std::process::Command::new(find_ffmpeg())
                .args(["-v", "error", "-i"])
                .arg(path)
                .args(["-map", "0:v:0", "-c:v", "copy", "-f", "hash", "-"])
                .output()
                .unwrap()
                .stdout
        };
        assert_eq!(
            hashes(&source),
            hashes(&output),
            "video packets must remain unchanged"
        );
        let m = measure(output.to_str().unwrap(), "").await.unwrap();
        assert!((m.integrated + 21.0).abs() < 1.0, "{m:?}");
    }
}
