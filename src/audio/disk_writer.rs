use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use hound::{SampleFormat, WavSpec, WavWriter};

use crate::units::{ChannelIndex, SampleRate};

/// Conservative data-byte threshold per file. RIFF size (u32) hard-caps at
/// 4 GiB − 1 with another ~44 bytes of headers; rotating at 4 GB decimal
/// gives plenty of margin and keeps filenames predictable. At 48 kHz mono
/// 32-bit float that's just under 5h 47m per part — long enough that most
/// sessions never split, short enough we never approach the actual cap.
const MAX_BYTES_PER_FILE: u64 = 4_000_000_000;
const SAMPLE_BYTES: u64 = 4;
const MAX_SAMPLES_PER_FILE: u64 = MAX_BYTES_PER_FILE / SAMPLE_BYTES;

/// One armed channel — its position in the device's interleaved frame and
/// its user-supplied label, used for the per-channel filename.
pub struct ArmedChannel {
    pub index: ChannelIndex,
    pub label: Option<String>,
}

pub struct DiskWriterConfig {
    /// Folder that will hold one WAV per armed channel (plus continuation
    /// files for any channel that crosses 4 GB).
    pub output_dir: PathBuf,
    pub sample_rate: SampleRate,
    /// Total channels in each frame from the audio thread (= device channel
    /// count); needed to demultiplex the rtrb stream into per-channel files.
    pub total_channel_count: u16,
    pub armed: Vec<ArmedChannel>,
}

pub struct DiskWriter {
    output_dir: PathBuf,
    stop_signal: Arc<AtomicBool>,
    writer_thread: Option<JoinHandle<()>>,
}

impl DiskWriter {
    pub fn start(consumer: rtrb::Consumer<f32>, config: DiskWriterConfig) -> Self {
        std::fs::create_dir_all(&config.output_dir)
            .expect("Failed to create recording directory");

        let stop_signal = Arc::new(AtomicBool::new(false));
        let output_dir = config.output_dir.clone();

        let writer_thread = {
            let stop_signal = stop_signal.clone();
            thread::spawn(move || write_to_disk(consumer, config, stop_signal))
        };

        DiskWriter {
            output_dir,
            stop_signal,
            writer_thread: Some(writer_thread),
        }
    }

    pub fn output_dir(&self) -> &Path {
        &self.output_dir
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
    config: DiskWriterConfig,
    stop_signal: Arc<AtomicBool>,
) {
    let total = config.total_channel_count as usize;
    let spec = WavSpec {
        channels: 1,
        sample_rate: config.sample_rate.0,
        bits_per_sample: 32,
        sample_format: SampleFormat::Float,
    };

    let mut part: u32 = 1;
    let mut writers = open_writers(&config, spec, part);
    let mut samples_in_part: u64 = 0;
    let mut frame = vec![0.0f32; total];
    let mut filled = 0;

    loop {
        while let Ok(sample) = consumer.pop() {
            frame[filled] = sample;
            filled += 1;
            if filled == frame.len() {
                if samples_in_part >= MAX_SAMPLES_PER_FILE {
                    // All channels split at the same frame boundary so parts
                    // stay sample-aligned across files.
                    finalize_writers(writers);
                    part += 1;
                    writers = open_writers(&config, spec, part);
                    samples_in_part = 0;
                }
                for (writer, ch) in writers.iter_mut().zip(config.armed.iter()) {
                    writer
                        .write_sample(frame[ch.index.as_usize()])
                        .expect("Failed to write sample");
                }
                samples_in_part += 1;
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
    config: &DiskWriterConfig,
    spec: WavSpec,
    part: u32,
) -> Vec<WavWriter<BufWriter<File>>> {
    config
        .armed
        .iter()
        .map(|ch| {
            let path = config.output_dir.join(channel_filename(ch, part));
            WavWriter::create(&path, spec).expect("Failed to create WAV file")
        })
        .collect()
}

fn finalize_writers(writers: Vec<WavWriter<BufWriter<File>>>) {
    for writer in writers {
        writer.finalize().expect("Failed to finalize WAV file");
    }
}

/// `chNN[-label][-ptN].wav`. Part suffix is omitted on part 1 so short
/// recordings get clean filenames (`ch00-kick.wav`); rotation produces
/// `ch00-kick-pt2.wav`, `ch00-kick-pt3.wav`, …
fn channel_filename(ch: &ArmedChannel, part: u32) -> String {
    let suffix = if part == 1 {
        String::new()
    } else {
        format!("-pt{}", part)
    };
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
            format!("ch{:02}-{}{}.wav", ch.index.0, safe, suffix)
        }
        _ => format!("ch{:02}{}.wav", ch.index.0, suffix),
    }
}
