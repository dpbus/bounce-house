use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

use cpal::Stream;
use cpal::traits::StreamTrait;

use crate::audio_interface::AudioInterface;

pub fn start(
    interface: &AudioInterface,
    channels: &[u8],
    output_path: &Path,
) -> (Stream, Arc<AtomicBool>, thread::JoinHandle<()>) {
    let total_channel_count = interface.channel_count();

    let max_channel = channels.iter().max().copied().unwrap_or(0) as usize;
    assert!(
        max_channel < total_channel_count,
        "Requested channel {} but device only has {} channels",
        max_channel,
        total_channel_count,
    );

    let stream_channels = channels.to_vec();
    let (mut producer, consumer) = rtrb::RingBuffer::new(48000 * 12);

    let stream = interface
        .build_input_stream(
            move |data: &[f32]| {
                for frame in 0..data.len() / total_channel_count {
                    for &ch in &stream_channels {
                        let sample = data[frame * total_channel_count + ch as usize];
                        let _ = producer.push(sample);
                    }
                }
            }
        );


    let running = Arc::new(AtomicBool::new(true));
    let running_clone = running.clone();
    let output_path = output_path.to_path_buf();
    let sample_rate = interface.sample_rate();
    let num_channels = channels.len() as u16;

    let handle = thread::spawn(move || {
        write_to_disk(
            consumer,
            &output_path,
            sample_rate,
            num_channels,
            running_clone,
        )
    });

    stream.play().expect("Failed to start stream");

    (stream, running, handle)
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
