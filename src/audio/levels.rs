use std::sync::atomic::Ordering;

use atomic_float::AtomicF32;

/// A peak level shared between the audio thread (writer) and UI threads (readers).
pub struct ChannelLevel(AtomicF32);

impl ChannelLevel {
    pub(super) fn new() -> Self {
        ChannelLevel(AtomicF32::new(0.0))
    }

    pub fn current(&self) -> f32 {
        self.0.load(Ordering::Relaxed)
    }

    pub(super) fn store(&self, value: f32) {
        self.0.store(value, Ordering::Relaxed);
    }
}
