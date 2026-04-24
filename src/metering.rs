use std::sync::atomic::Ordering;
use atomic_float::AtomicF32;

pub struct ChannelLevel {
    pub channel: u8,
    pub peak: AtomicF32,
}

impl ChannelLevel {
    pub fn new(channel: u8) -> Self {
        ChannelLevel {
            channel,
            peak: AtomicF32::new(0.0),
        }
    }

    pub fn current(&self) -> f32 {
        self.peak.load(Ordering::Relaxed)
    }
}

pub fn levels_for(channels: &[u8]) -> Vec<ChannelLevel> {
    channels.iter().map(|&ch| ChannelLevel::new(ch)).collect()
}
