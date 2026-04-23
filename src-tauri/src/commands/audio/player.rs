use std::sync::mpsc;

use rodio::Source;
use tauri::Emitter;

enum AudioCommand {
    Play(String, mpsc::Sender<Result<AudioFileInfo, String>>, Option<tauri::AppHandle>),
    Stop,
    Seek(f64),
    SetVolume(f32),
    GetPosition(mpsc::Sender<f64>),
    GetDuration(mpsc::Sender<f64>),
    Shutdown,
}

/// Info returned after starting playback.
#[derive(Clone)]
#[allow(dead_code)]
pub struct AudioFileInfo {
    pub duration_ms: f64,
}

pub struct AudioPlayer {
    tx: mpsc::Sender<AudioCommand>,
}

/// SAFETY: AudioPlayer only contains an `mpsc::Sender<AudioCommand>`, which is inherently
/// thread-safe (Sync + Send). The sender is a reference-counted channel handle that can
/// be safely shared across threads. All mutation happens on the dedicated background
/// thread that owns the receiver.
unsafe impl Sync for AudioPlayer {}

/// Reusable helper to open an audio file and build a decoder source.
fn open_audio_source(path: &str, skip_ms: f64) -> Result<(impl Source<Item = f32> + Send, f64), String> {
    let file = std::fs::File::open(path)
        .map_err(|e| format!("Failed to open audio file '{}': {}", path, e))?;
    let decoder = rodio::Decoder::new(std::io::BufReader::new(file))
        .map_err(|e| format!("Failed to decode audio: {}", e))?;
    let duration_ms = decoder
        .total_duration()
        .map(|d| d.as_millis() as f64)
        .unwrap_or(0.0);
    let skip_dur = std::time::Duration::from_millis((skip_ms as u64).min(duration_ms as u64));
    let source = decoder.skip_duration(skip_dur).convert_samples();
    Ok((source, duration_ms))
}

impl AudioPlayer {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel::<AudioCommand>();

        std::thread::spawn(move || {
            let mut current_sink: Option<std::sync::Arc<rodio::Sink>> = None;
            let mut current_stream: Option<rodio::OutputStream> = None;
            let mut current_path: Option<String> = None;
            let mut current_seek_ms: f64 = 0.0;
            let mut current_duration_ms: f64 = 0.0;
            let mut playback_start: Option<std::time::Instant> = None;
            let mut paused_at: Option<f64> = None; // ms position when paused
            let mut current_app: Option<tauri::AppHandle> = None;

            while let Ok(cmd) = rx.recv() {
                match cmd {
                    AudioCommand::Play(path, reply, app_handle) => {
                        // Stop any currently playing audio
                        if let Some(sink) = current_sink.take() {
                            sink.stop();
                        }
                        current_stream.take();
                        current_path = Some(path.clone());
                        current_seek_ms = 0.0;
                        paused_at = None;

                        let result = (|| -> Result<(std::sync::Arc<rodio::Sink>, f64), String> {
                            let (stream, stream_handle) =
                                rodio::OutputStream::try_default()
                                    .map_err(|e| format!("Failed to open audio output: {}", e))?;

                            let sink = rodio::Sink::try_new(&stream_handle)
                                .map_err(|e| format!("Failed to create audio sink: {}", e))?;

                            let (source, duration_ms) = open_audio_source(&path, 0.0)?;
                            sink.append(source);
                            let sink = std::sync::Arc::new(sink);
                            current_stream = Some(stream);
                            Ok((sink, duration_ms))
                        })();

                        match result {
                            Ok((sink, duration_ms)) => {
                                current_duration_ms = duration_ms;
                                current_sink = Some(sink.clone());
                                playback_start = Some(std::time::Instant::now());
                                let _ = reply.send(Ok(AudioFileInfo { duration_ms }));

                                if let Some(app) = app_handle {
                                    current_app = Some(app.clone());
                                    let path = path.clone();
                                    std::thread::spawn(move || {
                                        sink.sleep_until_end();
                                        let _ = app.emit("audio-finished", &path);
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
                        current_path = None;
                        current_seek_ms = 0.0;
                        playback_start = None;
                        paused_at = None;
                        current_app = None;
                    }
                    AudioCommand::Seek(pos_ms) => {
                        if let Some(ref file_path) = current_path {
                            // rodio doesn't support mid-stream seeking, so we
                            // recreate the sink from the seek position.
                            if let Some(sink) = current_sink.take() {
                                sink.stop();
                            }
                            current_stream.take();
                            paused_at = None;

                            let path = file_path.clone();
                            let seek = pos_ms;

                            if let Ok((stream, stream_handle)) = rodio::OutputStream::try_default() {
                                if let Ok(sink) = rodio::Sink::try_new(&stream_handle) {
                                    if let Ok((source, _dur)) = open_audio_source(&path, seek) {
                                        sink.append(source);
                                        let sink = std::sync::Arc::new(sink);
                                        current_sink = Some(sink.clone());
                                        current_stream = Some(stream);
                                        current_seek_ms = seek;
                                        playback_start = Some(std::time::Instant::now());

                                        if let Some(ref app) = current_app {
                                            let path2 = path.clone();
                                            let app2 = app.clone();
                                            std::thread::spawn(move || {
                                                sink.sleep_until_end();
                                                let _ = app2.emit("audio-finished", &path2);
                                            });
                                        }
                                    }
                                }
                            }
                        }
                    }
                    AudioCommand::SetVolume(vol) => {
                        if let Some(ref sink) = current_sink {
                            sink.set_volume(vol);
                        }
                    }
                    AudioCommand::GetPosition(reply) => {
                        let pos = if let Some(start) = playback_start {
                            let elapsed = start.elapsed().as_millis() as f64;
                            if let Some(paused) = paused_at {
                                reply.send(paused).ok();
                            } else {
                                let actual = (current_seek_ms + elapsed).min(current_duration_ms);
                                reply.send(actual).ok();
                            }
                        } else {
                            reply.send(0.0).ok();
                        };
                        let _ = pos;
                    }
                    AudioCommand::GetDuration(reply) => {
                        let _ = reply.send(current_duration_ms);
                    }
                    AudioCommand::Shutdown => break,
                }
            }
        });

        AudioPlayer { tx }
    }

    pub fn play(&self, file_path: &str, app: Option<tauri::AppHandle>) -> Result<AudioFileInfo, String> {
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

    pub fn get_position(&self) -> f64 {
        let (reply_tx, reply_rx) = mpsc::channel();
        let _ = self.tx.send(AudioCommand::GetPosition(reply_tx));
        reply_rx.recv().unwrap_or(0.0)
    }

    pub fn get_duration(&self) -> f64 {
        let (reply_tx, reply_rx) = mpsc::channel();
        let _ = self.tx.send(AudioCommand::GetDuration(reply_tx));
        reply_rx.recv().unwrap_or(0.0)
    }

    pub fn seek(&self, pos_ms: f64) {
        let _ = self.tx.send(AudioCommand::Seek(pos_ms));
    }

    pub fn set_volume(&self, volume: f32) {
        let _ = self.tx.send(AudioCommand::SetVolume(volume.clamp(0.0, 1.0)));
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
) -> Result<(), crate::core::error::AppError> {
    state.play(&file_path, Some(app)).map(|_| ()).map_err(crate::core::error::AppError::Audio)
}

#[tauri::command]
pub fn stop_audio(
    state: tauri::State<'_, AudioPlayer>,
) -> Result<(), crate::core::error::AppError> {
    state.stop();
    Ok(())
}

#[tauri::command]
pub fn set_audio_volume(
    state: tauri::State<'_, AudioPlayer>,
    volume: f32,
) -> Result<(), crate::core::error::AppError> {
    state.set_volume(volume);
    Ok(())
}

#[tauri::command]
pub fn get_audio_position(
    state: tauri::State<'_, AudioPlayer>,
) -> Result<f64, crate::core::error::AppError> {
    Ok(state.get_position())
}

#[tauri::command]
pub fn get_audio_duration_ms(
    state: tauri::State<'_, AudioPlayer>,
) -> Result<f64, crate::core::error::AppError> {
    Ok(state.get_duration())
}

#[tauri::command]
pub fn seek_audio(
    state: tauri::State<'_, AudioPlayer>,
    position_ms: f64,
) -> Result<(), crate::core::error::AppError> {
    state.seek(position_ms);
    Ok(())
}

