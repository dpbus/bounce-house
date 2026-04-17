use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

use cpal::Stream;
use cpal::traits::{DeviceTrait, StreamTrait};

pub fn start(
    device: &cpal::Device,
    channels: &[u8],
    output_path: &Path,
) -> (Stream, Arc<AtomicBool>, thread::JoinHandle<()>) {
    let config = device
        .default_input_config()
        .expect("No default input config");
    let total_channels = config.channels() as usize;

    let max_channel = channels.iter().max().copied().unwrap_or(0) as usize;
    assert!(
        max_channel < total_channels,
        "Requested channel {} but device only has {} channels",
        max_channel,
        total_channels,
    );

    let num_channels = channels.len() as u16;
    let sample_rate = config.sample_rate().0;
    let channels = channels.to_vec();

    let (mut producer, consumer) = rtrb::RingBuffer::new(48000 * 12);

    let stream = device
        .build_input_stream(
            &config.into(),
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                for frame in 0..data.len() / total_channels {
                    for &ch in &channels {
                        let sample = data[frame * total_channels + ch as usize];
                        let _ = producer.push(sample);
                    }
                }
            },
            |err| {
                eprintln!("Stream error: {}", err);
            },
            None,
        )
        .expect("Failed to build input stream");

    let running = Arc::new(AtomicBool::new(true));
    let running_clone = running.clone();

    let output_path = output_path.to_path_buf();

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
