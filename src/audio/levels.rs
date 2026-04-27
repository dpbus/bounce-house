use std::sync::atomic::Ordering;

use atomic_float::AtomicF32;

pub const MAX_CHANNELS: usize = 128;

#[derive(Clone, Copy)]
pub struct LevelObservation {
    pub sample: u64,
    pub recorded: bool,
    pub channel_peaks: [f32; MAX_CHANNELS],
}

/// Per-channel peak amplitude. Audio thread accumulates the absolute peak
/// across every audio buffer (so peaks aren't missed when multiple buffers
/// run between UI ticks); UI thread atomically reads-and-resets via
/// `take_current` once per tick.
pub struct ChannelLevel(AtomicF32);

impl ChannelLevel {
    pub(super) fn new() -> Self {
        ChannelLevel(AtomicF32::new(0.0))
    }

    /// Audio thread: fold a buffer's absolute peak into the accumulator.
    pub(super) fn record(&self, buffer_peak: f32) {
        let _ = self
            .0
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |c| {
                if buffer_peak > c { Some(buffer_peak) } else { None }
            });
    }

    /// UI thread: read the accumulated peak since the last call and reset.
    pub fn take_current(&self) -> f32 {
        self.0.swap(0.0, Ordering::Relaxed)
    }
}
