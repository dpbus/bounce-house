use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::units::{ChannelIndex, SampleRate};

pub struct RecordingConfig {
    pub output_path: PathBuf,
    pub sample_rate: SampleRate,
    pub total_channel_count: u16,
    pub armed: Vec<ChannelIndex>,
}

pub struct Recording {
    output_path: PathBuf,
    stop_signal: Arc<AtomicBool>,
    writer_thread: Option<JoinHandle<()>>,
}

impl Recording {
    pub fn start(consumer: rtrb::Consumer<f32>, config: RecordingConfig) -> Self {
        let stop_signal = Arc::new(AtomicBool::new(false));
        let output_path = config.output_path.clone();

        let writer_thread = {
            let stop_signal = stop_signal.clone();
            thread::spawn(move || write_to_disk(consumer, config, stop_signal))
        };

        Recording {
            output_path,
            stop_signal,
            writer_thread: Some(writer_thread),
        }
    }

    pub fn output_path(&self) -> &Path {
        &self.output_path
    }
}

impl Drop for Recording {
    fn drop(&mut self) {
        self.stop_signal.store(true, Ordering::Relaxed);
        if let Some(handle) = self.writer_thread.take() {
            let _ = handle.join();
        }
    }
}

fn write_to_disk(
    mut consumer: rtrb::Consumer<f32>,
    config: RecordingConfig,
    stop_signal: Arc<AtomicBool>,
) {
    let armed_count = config.armed.len() as u16;
    let total = config.total_channel_count as usize;

    let spec = hound::WavSpec {
        channels: armed_count,
        sample_rate: config.sample_rate.0,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut writer = hound::WavWriter::create(&config.output_path, spec)
        .expect("Failed to create WAV file");

    // Reusable buffer for one full frame from the rtrb
    let mut frame = vec![0.0f32; total];
    let mut filled = 0;

    let drain = |consumer: &mut rtrb::Consumer<f32>,
                 writer: &mut hound::WavWriter<_>,
                 frame: &mut [f32],
                 filled: &mut usize| {
        while let Ok(sample) = consumer.pop() {
            frame[*filled] = sample;
            *filled += 1;
            if *filled == frame.len() {
                for ch in &config.armed {
                    writer
                        .write_sample(frame[ch.as_usize()])
                        .expect("Failed to write sample");
                }
                *filled = 0;
            }
        }
    };

    while !stop_signal.load(Ordering::Relaxed) {
        drain(&mut consumer, &mut writer, &mut frame, &mut filled);
        thread::sleep(Duration::from_millis(10));
    }

    // Final drain — catch any samples that arrived between the last loop
    // iteration and the stop signal.
    drain(&mut consumer, &mut writer, &mut frame, &mut filled);

    writer.finalize().expect("Failed to finalize WAV file");
}
