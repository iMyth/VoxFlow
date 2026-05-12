use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Shared cancellation flag for LLM requests.
/// Only one LLM request is expected at a time, so a single flag is sufficient.
#[derive(Default)]
pub struct CancellationToken(pub AtomicBool);

impl CancellationToken {
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    pub fn reset(&self) {
        self.0.store(false, Ordering::Relaxed);
    }
}

/// Separate cancellation flag for video export (independent of LLM/TTS).
/// Uses Arc<AtomicBool> so it can be cloned into blocking threads.
#[derive(Clone)]
pub struct VideoCancelToken(pub Arc<AtomicBool>);

impl Default for VideoCancelToken {
    fn default() -> Self {
        Self(Arc::new(AtomicBool::new(false)))
    }
}

impl VideoCancelToken {
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    pub fn reset(&self) {
        self.0.store(false, Ordering::Relaxed);
    }

    /// Get a clone of the inner Arc for passing into threads.
    pub fn flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.0)
    }
}
