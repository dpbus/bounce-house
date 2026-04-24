use std::sync::{Arc, Mutex};

use cpal::Stream;
use cpal::traits::StreamTrait;

use crate::audio_interface::AudioInterface;

pub struct LevelMonitor {
    levels: Arc<Mutex<Vec<f32>>>,
    _stream: Stream,
}

impl LevelMonitor {
    pub fn new(interface: &AudioInterface) -> Self {
        let num_channels = interface.channel_count() as usize;

        let levels = Arc::new(Mutex::new(vec![0.0f32; num_channels]));
        let levels_clone = levels.clone();
        let decay = 0.75f32;

        let stream = interface
            .build_input_stream(
                move |data: &[f32]| {
                    let mut peaks = vec![0.0f32; num_channels];
                    for frame in 0..data.len() / num_channels {
                        for ch in 0..num_channels {
                            let sample = data[frame * num_channels + ch].abs();
                            if sample > peaks[ch] {
                                peaks[ch] = sample;
                            }
                        }
                    }
                    if let Ok(mut lvl) = levels_clone.lock() {
                        for ch in 0..num_channels {
                            lvl[ch] = peaks[ch].max(lvl[ch] * decay);
                        }
                    }
                },
            );

        stream.play().expect("Failed to start preview stream");

        LevelMonitor {
            levels,
            _stream: stream,
        }
    }

    pub fn levels(&self) -> Vec<f32> {
        self.levels.lock().map(|l| l.clone()).unwrap_or_default()
    }
}
