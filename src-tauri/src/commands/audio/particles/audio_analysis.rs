//! Audio feature extraction for driving particle animation.
//!
//! Uses efficient O(n) algorithms: RMS for loudness, and simple single-pole
//! IIR filters for band energy estimation (no DFT needed).

use std::path::Path;

use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::DecoderOptions;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

/// Audio features for a single video frame.
#[derive(Debug, Clone)]
pub struct FrameFeatures {
    /// Root mean square (0.0 - 1.0 normalized)
    pub rms: f32,
    /// Low frequency energy (bass, 0-300Hz)
    pub low_energy: f32,
    /// Mid frequency energy (300-2000Hz)
    pub mid_energy: f32,
    /// High frequency energy (2000Hz+)
    pub high_energy: f32,
}

/// Extract per-frame audio features from an audio file.
/// Returns one FrameFeatures per video frame at the given fps.
pub fn extract_audio_features(audio_path: &Path, fps: u32) -> Result<Vec<FrameFeatures>, String> {
    let file = std::fs::File::open(audio_path)
        .map_err(|e| format!("Failed to open audio: {}", e))?;

    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    if let Some(ext) = audio_path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }

    let probed = symphonia::default::get_probe()
        .format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default())
        .map_err(|e| format!("Failed to probe audio format: {}", e))?;

    let mut format = probed.format;

    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != symphonia::core::codecs::CODEC_TYPE_NULL)
        .ok_or("No audio track found")?;

    let sample_rate = track.codec_params.sample_rate.unwrap_or(44100);
    let track_id = track.id;

    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|e| format!("Failed to create decoder: {}", e))?;

    // Collect all samples into a single mono buffer
    let mut all_samples: Vec<f32> = Vec::new();

    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(symphonia::core::errors::Error::IoError(ref e))
                if e.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                break;
            }
            Err(_) => break,
        };

        if packet.track_id() != track_id {
            continue;
        }

        let decoded = match decoder.decode(&packet) {
            Ok(d) => d,
            Err(_) => continue,
        };

        let spec = *decoded.spec();
        let num_frames = decoded.capacity();
        let mut sample_buf = SampleBuffer::<f32>::new(num_frames as u64, spec);
        sample_buf.copy_interleaved_ref(decoded);

        let channels = spec.channels.count().max(1);
        let samples = sample_buf.samples();

        // Mix to mono
        for chunk in samples.chunks(channels) {
            let mono: f32 = chunk.iter().sum::<f32>() / channels as f32;
            all_samples.push(mono);
        }
    }

    if all_samples.is_empty() {
        return Ok(vec![FrameFeatures {
            rms: 0.0,
            low_energy: 0.0,
            mid_energy: 0.0,
            high_energy: 0.0,
        }]);
    }

    // Pre-filter the entire signal into 3 bands using single-pole IIR filters.
    // This is O(n) total — much faster than per-frame DFT.
    let low_filtered = low_pass_filter(&all_samples, sample_rate, 300.0);
    let high_filtered = high_pass_filter(&all_samples, sample_rate, 2000.0);
    // Mid = original - low - high (approximation)
    let mid_filtered: Vec<f32> = all_samples
        .iter()
        .zip(low_filtered.iter())
        .zip(high_filtered.iter())
        .map(|((&orig, &low), &high)| orig - low - high)
        .collect();

    // Calculate samples per frame
    let samples_per_frame = sample_rate as usize / fps as usize;
    let total_frames = (all_samples.len() + samples_per_frame - 1) / samples_per_frame;

    let mut features = Vec::with_capacity(total_frames);
    let mut max_rms: f32 = 0.0;
    let mut max_low: f32 = 0.0;
    let mut max_mid: f32 = 0.0;
    let mut max_high: f32 = 0.0;

    // Extract features per frame — all O(samples_per_frame) per frame
    for frame_idx in 0..total_frames {
        let start = frame_idx * samples_per_frame;
        let end = (start + samples_per_frame).min(all_samples.len());
        let len = (end - start) as f32;

        // RMS of original signal
        let rms = (all_samples[start..end].iter().map(|s| s * s).sum::<f32>() / len).sqrt();

        // RMS of each band
        let low_e = (low_filtered[start..end].iter().map(|s| s * s).sum::<f32>() / len).sqrt();
        let mid_e = (mid_filtered[start..end].iter().map(|s| s * s).sum::<f32>() / len).sqrt();
        let high_e = (high_filtered[start..end].iter().map(|s| s * s).sum::<f32>() / len).sqrt();

        max_rms = max_rms.max(rms);
        max_low = max_low.max(low_e);
        max_mid = max_mid.max(mid_e);
        max_high = max_high.max(high_e);

        features.push(FrameFeatures {
            rms,
            low_energy: low_e,
            mid_energy: mid_e,
            high_energy: high_e,
        });
    }

    // Normalize all values to 0..1
    max_rms = max_rms.max(1e-6);
    max_low = max_low.max(1e-6);
    max_mid = max_mid.max(1e-6);
    max_high = max_high.max(1e-6);

    for f in &mut features {
        f.rms = (f.rms / max_rms).min(1.0);
        f.low_energy = (f.low_energy / max_low).min(1.0);
        f.mid_energy = (f.mid_energy / max_mid).min(1.0);
        f.high_energy = (f.high_energy / max_high).min(1.0);
    }

    Ok(features)
}

/// Simple single-pole low-pass IIR filter. O(n).
fn low_pass_filter(samples: &[f32], sample_rate: u32, cutoff_hz: f32) -> Vec<f32> {
    let rc = 1.0 / (2.0 * std::f32::consts::PI * cutoff_hz);
    let dt = 1.0 / sample_rate as f32;
    let alpha = dt / (rc + dt);

    let mut output = Vec::with_capacity(samples.len());
    let mut prev = 0.0f32;
    for &s in samples {
        prev = prev + alpha * (s - prev);
        output.push(prev);
    }
    output
}

/// Simple single-pole high-pass IIR filter. O(n).
fn high_pass_filter(samples: &[f32], sample_rate: u32, cutoff_hz: f32) -> Vec<f32> {
    let rc = 1.0 / (2.0 * std::f32::consts::PI * cutoff_hz);
    let dt = 1.0 / sample_rate as f32;
    let alpha = rc / (rc + dt);

    let mut output = Vec::with_capacity(samples.len());
    let mut prev_input = 0.0f32;
    let mut prev_output = 0.0f32;
    for &s in samples {
        prev_output = alpha * (prev_output + s - prev_input);
        prev_input = s;
        output.push(prev_output);
    }
    output
}
