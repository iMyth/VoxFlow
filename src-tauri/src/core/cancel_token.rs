use std::sync::atomic::{AtomicBool, Ordering};

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
