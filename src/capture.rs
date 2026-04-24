use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

use cpal::Stream;
use cpal::traits::StreamTrait;

use crate::audio_interface::AudioInterface;
use crate::metering::{ChannelLevel, levels_for};

pub struct CaptureHandle {
    pub stream: Stream,
    pub running: Arc<AtomicBool>,
    pub writer_handle: Option<thread::JoinHandle<()>>,
    pub levels: Arc<Vec<ChannelLevel>>,
}

pub fn start(
    interface: &AudioInterface,
    channels: &[u8],
    output_path: &Path,
) -> CaptureHandle {
    let total_channel_count = interface.channel_count();
    let max_channel = channels.iter().max().copied().unwrap_or(0) as usize;
    assert!(
        max_channel < total_channel_count,
        "Requested channel {} but device only has {} channels",
        max_channel,
        total_channel_count,
    );

    let levels = Arc::new(levels_for(channels));
    let levels_clone = levels.clone();
    let stream_channels = channels.to_vec();
    let mut peaks_buf = vec![0.0f32; channels.len()];

    let (mut producer, consumer) = rtrb::RingBuffer::new(48000 * 12);

    let stream = interface.build_input_stream(move |data: &[f32]| {
        peaks_buf.fill(0.0);
        for frame in 0..data.len() / total_channel_count {
            for (i, &ch) in stream_channels.iter().enumerate() {
                let sample = data[frame * total_channel_count + ch as usize];
                let abs = sample.abs();
                if abs > peaks_buf[i] {
                    peaks_buf[i] = abs;
                }
                let _ = producer.push(sample);
            }
        }
        for (entry, &peak) in levels_clone.iter().zip(peaks_buf.iter()) {
            entry.peak.store(peak, Ordering::Relaxed);
        }
    });

    let running = Arc::new(AtomicBool::new(true));
    let running_clone = running.clone();
    let output_path = output_path.to_path_buf();
    let sample_rate = interface.sample_rate();
    let num_channels = channels.len() as u16;

    let writer_handle = thread::spawn(move || {
        write_to_disk(
            consumer,
            &output_path,
            sample_rate,
            num_channels,
            running_clone,
        )
    });

    stream.play().expect("Failed to start stream");

    CaptureHandle {
        stream,
        running,
        writer_handle: Some(writer_handle),
        levels,
    }
}

fn write_to_disk(
    mut consumer: rtrb::Consumer<f32>,
    path: &Path,
    sample_rate: u32,
    num_channels: u16,
    running: Arc<AtomicBool>,
) {
    let spec = hound::WavSpec {
        channels: num_channels,
        sample_rate,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };

    let mut writer = hound::WavWriter::create(path, spec).expect("Failed to create WAV file");

    while running.load(Ordering::Relaxed) {
        while let Ok(sample) = consumer.pop() {
            writer.write_sample(sample).expect("Failed to write sample");
        }
        thread::sleep(std::time::Duration::from_millis(10));
    }

    // Drain any remaining samples that arrived between the last loop iteration and
    // running flag set to false.
    while let Ok(sample) = consumer.pop() {
        writer.write_sample(sample).expect("Failed to write sample");
    }
    writer.finalize().expect("Failed to finalize WAV file");
}
