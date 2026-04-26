use std::fs::File;
use std::io::BufWriter;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use hound::{SampleFormat, WavSpec, WavWriter};

use crate::units::{ChannelIndex, SampleRate};

/// One armed channel — its position in the device's interleaved frame and
/// its user-supplied label, used for the per-channel filename.
pub struct ArmedChannel {
    pub index: ChannelIndex,
    pub label: Option<String>,
}

pub struct DiskWriter {
    channel_files: Vec<PathBuf>,
    stop_signal: Arc<AtomicBool>,
    writer_thread: Option<JoinHandle<()>>,
}

impl DiskWriter {
    /// `output_dir` will hold one WAV per armed channel. RIFF's u32 size
    /// field caps each file at ~4 GB → ~5h 47m at 48 kHz mono float32.
    /// Past that point the writer will fail; not currently guarded.
    /// `total_channel_count` is the device's full channel count, needed to
    /// demultiplex the interleaved rtrb stream.
    pub fn start(
        consumer: rtrb::Consumer<f32>,
        output_dir: PathBuf,
        sample_rate: SampleRate,
        total_channel_count: u16,
        armed: Vec<ArmedChannel>,
    ) -> Self {
        std::fs::create_dir_all(&output_dir)
            .expect("Failed to create recording directory");

        let channel_files: Vec<PathBuf> = armed
            .iter()
            .map(|ch| output_dir.join(channel_filename(ch)))
            .collect();

        let stop_signal = Arc::new(AtomicBool::new(false));

        let writer_thread = {
            let stop_signal = stop_signal.clone();
            let channel_files = channel_files.clone();
            thread::spawn(move || {
                write_to_disk(
                    consumer,
                    channel_files,
                    sample_rate,
                    total_channel_count,
                    armed,
                    stop_signal,
                )
            })
        };

        DiskWriter {
            channel_files,
            stop_signal,
            writer_thread: Some(writer_thread),
        }
    }

    pub fn channel_files(&self) -> &[PathBuf] {
        &self.channel_files
    }
}

impl Drop for DiskWriter {
    fn drop(&mut self) {
        self.stop_signal.store(true, Ordering::Relaxed);
        if let Some(handle) = self.writer_thread.take() {
            let _ = handle.join();
        }
    }
}

fn write_to_disk(
    mut consumer: rtrb::Consumer<f32>,
    channel_files: Vec<PathBuf>,
    sample_rate: SampleRate,
    total_channel_count: u16,
    armed: Vec<ArmedChannel>,
    stop_signal: Arc<AtomicBool>,
) {
    let total = total_channel_count as usize;
    let spec = WavSpec {
        channels: 1,
        sample_rate: sample_rate.0,
        bits_per_sample: 32,
        sample_format: SampleFormat::Float,
    };

    let mut writers = open_writers(&channel_files, spec);
    let mut frame = vec![0.0f32; total];
    let mut filled = 0;

    loop {
        while let Ok(sample) = consumer.pop() {
            frame[filled] = sample;
            filled += 1;
            if filled == frame.len() {
                for (writer, ch) in writers.iter_mut().zip(armed.iter()) {
                    writer
                        .write_sample(frame[ch.index.as_usize()])
                        .expect("Failed to write sample");
                }
                filled = 0;
            }
        }
        if stop_signal.load(Ordering::Relaxed) {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }

    finalize_writers(writers);
}

fn open_writers(
    channel_files: &[PathBuf],
    spec: WavSpec,
) -> Vec<WavWriter<BufWriter<File>>> {
    channel_files
        .iter()
        .map(|path| WavWriter::create(path, spec).expect("Failed to create WAV file"))
        .collect()
}

fn finalize_writers(writers: Vec<WavWriter<BufWriter<File>>>) {
    for writer in writers {
        writer.finalize().expect("Failed to finalize WAV file");
    }
}

fn channel_filename(ch: &ArmedChannel) -> String {
    match &ch.label {
        Some(label) if !label.trim().is_empty() => {
            let safe: String = label
                .trim()
                .chars()
                .map(|c| {
                    if c.is_alphanumeric() || c == '-' || c == '_' {
                        c
                    } else {
                        '_'
                    }
                })
                .collect();
            format!("ch{:02}-{}.wav", ch.index.0, safe)
        }
        _ => format!("ch{:02}.wav", ch.index.0),
    }
}
