use std::fs::File;
use std::io::BufWriter;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use hound::{SampleFormat, WavSpec, WavWriter};

use crate::units::{ChannelIndex, SampleRate};

const FLUSH_INTERVAL: Duration = Duration::from_secs(1);

/// One armed channel: position in the interleaved frame plus user label
/// for the filename.
pub struct ArmedChannel {
    pub index: ChannelIndex,
    pub label: Option<String>,
}

pub struct DiskWriter {
    channel_files: Vec<PathBuf>,
    flushed_samples: Arc<AtomicU64>,
    stop_signal: Arc<AtomicBool>,
    writer_thread: Option<JoinHandle<()>>,
}

impl DiskWriter {
    /// One WAV per armed channel in `output_dir`. RIFF's u32 size field
    /// caps each file at ~4 GB (~5h 47m at 48 kHz mono float32); past that
    /// the writer will fail — not currently guarded. `total_channel_count`
    /// is needed to demultiplex the interleaved rtrb stream.
    pub fn start(
        consumer: rtrb::Consumer<f32>,
        output_dir: PathBuf,
        sample_rate: SampleRate,
        total_channel_count: u16,
        armed: Vec<ArmedChannel>,
    ) -> Self {
        std::fs::create_dir_all(&output_dir).expect("Failed to create recording directory");

        let channel_files: Vec<PathBuf> = armed
            .iter()
            .map(|ch| output_dir.join(channel_filename(ch)))
            .collect();

        let stop_signal = Arc::new(AtomicBool::new(false));
        let flushed_samples = Arc::new(AtomicU64::new(0));

        let writer_thread = {
            let stop_signal = stop_signal.clone();
            let flushed_samples = flushed_samples.clone();
            let channel_files = channel_files.clone();
            thread::spawn(move || {
                write_to_disk(
                    consumer,
                    channel_files,
                    sample_rate,
                    total_channel_count,
                    armed,
                    stop_signal,
                    flushed_samples,
                )
            })
        };

        DiskWriter {
            channel_files,
            flushed_samples,
            stop_signal,
            writer_thread: Some(writer_thread),
        }
    }

    pub fn channel_files(&self) -> &[PathBuf] {
        &self.channel_files
    }

    pub fn flushed_samples(&self) -> Arc<AtomicU64> {
        self.flushed_samples.clone()
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
    flushed_samples: Arc<AtomicU64>,
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
    let mut samples_written: u64 = 0;
    let mut last_flush = Instant::now();

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
                samples_written += 1;
                filled = 0;
            }
        }

        if last_flush.elapsed() >= FLUSH_INTERVAL {
            flush_and_publish(&mut writers, samples_written, &flushed_samples);
            last_flush = Instant::now();
        }

        if stop_signal.load(Ordering::Relaxed) {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }

    flush_and_publish(&mut writers, samples_written, &flushed_samples);
    finalize_writers(writers);
}

fn flush_and_publish(
    writers: &mut [WavWriter<BufWriter<File>>],
    samples_written: u64,
    flushed_samples: &AtomicU64,
) {
    for w in writers {
        w.flush().expect("Failed to flush WAV writer");
    }
    flushed_samples.store(samples_written, Ordering::Release);
}

fn open_writers(channel_files: &[PathBuf], spec: WavSpec) -> Vec<WavWriter<BufWriter<File>>> {
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
