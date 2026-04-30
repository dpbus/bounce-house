use std::fs::File;
use std::io::BufWriter;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use hound::{SampleFormat, WavSpec, WavWriter};

use crate::units::SampleRate;

const FLUSH_INTERVAL: Duration = Duration::from_secs(1);

/// One armed channel: position in the interleaved frame plus user label
/// for the filename.
pub struct ArmedChannel {
    pub index: u16,
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
                        .write_sample(frame[ch.index as usize])
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
            let safe = crate::paths::filename_safe(label.trim());
            format!("ch{:02}-{}.wav", ch.index, safe)
        }
        _ => format!("ch{:02}.wav", ch.index),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hound::WavReader;
    use tempfile::tempdir;

    fn read_wav_samples(path: &std::path::Path) -> Vec<f32> {
        let mut reader = WavReader::open(path).expect("open wav");
        reader
            .samples::<f32>()
            .map(|s| s.expect("read sample"))
            .collect()
    }

    fn drain_wait() {
        // The writer loop sleeps 10ms between drains. Two cycles is enough
        // for any pushed samples to be picked up before we drop and join.
        std::thread::sleep(Duration::from_millis(30));
    }

    #[test]
    fn writes_only_armed_channels_demuxed_to_separate_wavs() {
        let dir = tempdir().unwrap();
        let (mut producer, consumer) = rtrb::RingBuffer::<f32>::new(1024);

        let armed = vec![
            ArmedChannel {
                index: 0,
                label: Some("Vocals".into()),
            },
            ArmedChannel {
                index: 2,
                label: None,
            },
        ];

        let writer = DiskWriter::start(
            consumer,
            dir.path().to_path_buf(),
            SampleRate(48000),
            4,
            armed,
        );

        // 3 frames of 4 interleaved channels. Channel 0 carries 1/2/3,
        // channel 2 carries 10/20/30. Channels 1 and 3 are not armed
        // and should be dropped.
        for &s in &[
            1.0, 99.0, 10.0, 99.0, 2.0, 99.0, 20.0, 99.0, 3.0, 99.0, 30.0, 99.0,
        ] {
            producer.push(s).unwrap();
        }

        drain_wait();
        drop(writer);

        let ch0 = read_wav_samples(&dir.path().join("ch00-Vocals.wav"));
        let ch2 = read_wav_samples(&dir.path().join("ch02.wav"));
        assert_eq!(ch0, vec![1.0, 2.0, 3.0]);
        assert_eq!(ch2, vec![10.0, 20.0, 30.0]);
        assert!(!dir.path().join("ch01.wav").exists());
        assert!(!dir.path().join("ch03.wav").exists());
    }

    #[test]
    fn drop_finalizes_partial_frames_cleanly() {
        let dir = tempdir().unwrap();
        let (mut producer, consumer) = rtrb::RingBuffer::<f32>::new(1024);

        let armed = vec![ArmedChannel {
            index: 0,
            label: None,
        }];
        let writer = DiskWriter::start(
            consumer,
            dir.path().to_path_buf(),
            SampleRate(48000),
            2,
            armed,
        );

        // Two complete frames, then one partial (only ch0).
        // The partial should not be written since it never completes.
        for &s in &[1.0, 0.0, 2.0, 0.0, 3.0] {
            producer.push(s).unwrap();
        }
        drain_wait();
        drop(writer);

        let ch0 = read_wav_samples(&dir.path().join("ch00.wav"));
        assert_eq!(ch0, vec![1.0, 2.0]);
    }

    #[test]
    fn writes_partial_frame_into_subsequent_call() {
        let dir = tempdir().unwrap();
        let (mut producer, consumer) = rtrb::RingBuffer::<f32>::new(1024);

        let armed = vec![ArmedChannel {
            index: 1,
            label: None,
        }];
        let writer = DiskWriter::start(
            consumer,
            dir.path().to_path_buf(),
            SampleRate(48000),
            2,
            armed,
        );

        // Push samples in unaligned chunks; the demux state is preserved
        // across pop() calls, so frames should still align correctly.
        producer.push(1.0).unwrap();
        producer.push(10.0).unwrap();
        drain_wait();
        producer.push(2.0).unwrap();
        producer.push(20.0).unwrap();
        drain_wait();
        drop(writer);

        let ch1 = read_wav_samples(&dir.path().join("ch01.wav"));
        assert_eq!(ch1, vec![10.0, 20.0]);
    }

    #[test]
    fn channel_files_paths_match_disk() {
        let dir = tempdir().unwrap();
        let (_, consumer) = rtrb::RingBuffer::<f32>::new(1);

        let armed = vec![
            ArmedChannel {
                index: 0,
                label: Some("Kick".into()),
            },
            ArmedChannel {
                index: 1,
                label: None,
            },
        ];
        let writer = DiskWriter::start(
            consumer,
            dir.path().to_path_buf(),
            SampleRate(48000),
            2,
            armed,
        );

        let files = writer.channel_files().to_vec();
        assert_eq!(files.len(), 2);
        assert_eq!(files[0], dir.path().join("ch00-Kick.wav"));
        assert_eq!(files[1], dir.path().join("ch01.wav"));
    }

    #[test]
    fn channel_filename_with_label() {
        let ch = ArmedChannel {
            index: 5,
            label: Some("Vocals".into()),
        };
        assert_eq!(channel_filename(&ch), "ch05-Vocals.wav");
    }

    #[test]
    fn channel_filename_without_label() {
        let ch = ArmedChannel {
            index: 0,
            label: None,
        };
        assert_eq!(channel_filename(&ch), "ch00.wav");
    }

    #[test]
    fn channel_filename_treats_whitespace_label_as_unlabeled() {
        let ch = ArmedChannel {
            index: 1,
            label: Some("   ".into()),
        };
        assert_eq!(channel_filename(&ch), "ch01.wav");
    }

    #[test]
    fn channel_filename_sanitizes_path_separators() {
        let ch = ArmedChannel {
            index: 2,
            label: Some("a/b/c".into()),
        };
        let result = channel_filename(&ch);
        assert!(!result.contains('/'), "expected no '/' in {result}");
    }
}
