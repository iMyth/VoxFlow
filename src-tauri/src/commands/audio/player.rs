use std::sync::mpsc;

use tauri::Emitter;

enum AudioCommand {
    Play(String, mpsc::Sender<Result<(), String>>, Option<tauri::AppHandle>),
    Stop,
    SetVolume(f32),
    Shutdown,
}

pub struct AudioPlayer {
    tx: mpsc::Sender<AudioCommand>,
}

/// SAFETY: AudioPlayer only contains an `mpsc::Sender<AudioCommand>`, which is inherently
/// thread-safe (Sync + Send). The sender is a reference-counted channel handle that can
/// be safely shared across threads. All mutation happens on the dedicated background
/// thread that owns the receiver.
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
                        // Stop any currently playing audio
                        if let Some(sink) = current_sink.take() {
                            sink.stop();
                        }
                        current_stream.take();

                        let result = (|| -> Result<std::sync::Arc<rodio::Sink>, String> {
                            let (stream, stream_handle) =
                                rodio::OutputStream::try_default()
                                    .map_err(|e| format!("Failed to open audio output: {}", e))?;

                            let sink = rodio::Sink::try_new(&stream_handle)
                                .map_err(|e| format!("Failed to create audio sink: {}", e))?;

                            // Use rodio's built-in decoder (via symphonia) — no FFmpeg needed.
                            // Supports MP3, WAV, OGG, FLAC, AAC, M4A out of the box.
                            let file = std::fs::File::open(&path)
                                .map_err(|e| format!("Failed to open audio file '{}': {}", path, e))?;
                            let source = rodio::Decoder::new(std::io::BufReader::new(file))
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
                    }
                    AudioCommand::SetVolume(vol) => {
                        if let Some(ref sink) = current_sink {
                            sink.set_volume(vol);
                        }
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
    state.play(&file_path, Some(app)).map_err(crate::core::error::AppError::Audio)
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

