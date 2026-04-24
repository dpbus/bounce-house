use std::sync::Arc;
use std::sync::atomic::Ordering;

use cpal::Stream;
use cpal::traits::StreamTrait;

use crate::audio_interface::AudioInterface;
use crate::metering::{ChannelLevel, levels_for};

pub struct LevelMonitor {
    levels: Arc<Vec<ChannelLevel>>,
    _stream: Stream,
}

impl LevelMonitor {
    pub fn new(interface: &AudioInterface) -> Self {
        let num_channels = interface.channel_count() as usize;
        let all_channels: Vec<u8> = (0..num_channels as u8).collect();
        let levels = Arc::new(levels_for(&all_channels));
        let levels_clone = levels.clone();

        let mut peaks_buf = vec![0.0f32; num_channels];

        let stream = interface.build_input_stream(move |data: &[f32]| {
            peaks_buf.fill(0.0);
            for frame in 0..data.len() / num_channels {
                for ch in 0..num_channels {
                    let sample = data[frame * num_channels + ch].abs();
                    if sample > peaks_buf[ch] {
                        peaks_buf[ch] = sample;
                    }
                }
            };
            for (entry, &peak) in levels_clone.iter().zip(peaks_buf.iter()) {
                entry.peak.store(peak, Ordering::Relaxed);
            };
        },
        );

        stream.play().expect("Failed to start preview stream");

        LevelMonitor {
            levels,
            _stream: stream,
        }
    }

    pub fn levels(&self) -> &[ChannelLevel] {
        &self.levels
    }
}
