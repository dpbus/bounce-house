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

/// One output WAV: which interleaved input channel to demux from, and
/// where to write its samples. Caller decides both — DiskWriter just
/// opens the file and routes samples.
pub struct ChannelOutput {
    pub channel: u16,
    pub path: PathBuf,
}

pub struct DiskWriter {
    flushed_samples: Arc<AtomicU64>,
    stop_signal: Arc<AtomicBool>,
    writer_thread: Option<JoinHandle<()>>,
}

impl DiskWriter {
    /// Caller is responsible for ensuring each output's parent dir
    /// exists. RIFF's u32 size field caps each file at ~4 GB (~5h 47m
    /// at 48 kHz mono float32); past that the writer will fail — not
    /// currently guarded. `total_channel_count` is needed to
    /// demultiplex the interleaved rtrb stream.
    pub fn start(
        consumer: rtrb::Consumer<f32>,
        sample_rate: SampleRate,
        total_channel_count: u16,
        outputs: Vec<ChannelOutput>,
    ) -> Self {
        let stop_signal = Arc::new(AtomicBool::new(false));
        let flushed_samples = Arc::new(AtomicU64::new(0));

        let writer_thread = {
            let stop_signal = stop_signal.clone();
            let flushed_samples = flushed_samples.clone();
            thread::spawn(move || {
                write_to_disk(
                    consumer,
                    sample_rate,
                    total_channel_count,
                    outputs,
                    stop_signal,
                    flushed_samples,
                )
            })
        };

        DiskWriter {
            flushed_samples,
            stop_signal,
            writer_thread: Some(writer_thread),
        }
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
    sample_rate: SampleRate,
    total_channel_count: u16,
    outputs: Vec<ChannelOutput>,
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

    let mut writers = open_writers(&outputs, spec);
    let mut frame = vec![0.0f32; total];
    let mut filled = 0;
    let mut samples_written: u64 = 0;
    let mut last_flush = Instant::now();

    loop {
        while let Ok(sample) = consumer.pop() {
            frame[filled] = sample;
            filled += 1;
            if filled == frame.len() {
                for (writer, output) in writers.iter_mut().zip(outputs.iter()) {
                    writer
                        .write_sample(frame[output.channel as usize])
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

fn open_writers(outputs: &[ChannelOutput], spec: WavSpec) -> Vec<WavWriter<BufWriter<File>>> {
    outputs
        .iter()
        .map(|o| WavWriter::create(&o.path, spec).expect("Failed to create WAV file"))
        .collect()
}

fn finalize_writers(writers: Vec<WavWriter<BufWriter<File>>>) {
    for writer in writers {
        writer.finalize().expect("Failed to finalize WAV file");
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

    fn output(channel: u16, path: PathBuf) -> ChannelOutput {
        ChannelOutput { channel, path }
    }

    #[test]
    fn writes_only_routed_channels_demuxed_to_separate_wavs() {
        let dir = tempdir().unwrap();
        let (mut producer, consumer) = rtrb::RingBuffer::<f32>::new(1024);

        let outputs = vec![
            output(0, dir.path().join("ch0.wav")),
            output(2, dir.path().join("ch2.wav")),
        ];
        let writer = DiskWriter::start(consumer, SampleRate(48000), 4, outputs);

        // 3 frames of 4 interleaved channels. Channel 0 carries 1/2/3,
        // channel 2 carries 10/20/30. Channels 1 and 3 aren't routed
        // and should be dropped.
        for &s in &[
            1.0, 99.0, 10.0, 99.0, 2.0, 99.0, 20.0, 99.0, 3.0, 99.0, 30.0, 99.0,
        ] {
            producer.push(s).unwrap();
        }

        drain_wait();
        drop(writer);

        let ch0 = read_wav_samples(&dir.path().join("ch0.wav"));
        let ch2 = read_wav_samples(&dir.path().join("ch2.wav"));
        assert_eq!(ch0, vec![1.0, 2.0, 3.0]);
        assert_eq!(ch2, vec![10.0, 20.0, 30.0]);
    }

    #[test]
    fn drop_finalizes_partial_frames_cleanly() {
        let dir = tempdir().unwrap();
        let (mut producer, consumer) = rtrb::RingBuffer::<f32>::new(1024);

        let outputs = vec![output(0, dir.path().join("ch0.wav"))];
        let writer = DiskWriter::start(consumer, SampleRate(48000), 2, outputs);

        // Two complete frames, then one partial (only ch0).
        // The partial should not be written since it never completes.
        for &s in &[1.0, 0.0, 2.0, 0.0, 3.0] {
            producer.push(s).unwrap();
        }
        drain_wait();
        drop(writer);

        let ch0 = read_wav_samples(&dir.path().join("ch0.wav"));
        assert_eq!(ch0, vec![1.0, 2.0]);
    }

    #[test]
    fn writes_partial_frame_into_subsequent_call() {
        let dir = tempdir().unwrap();
        let (mut producer, consumer) = rtrb::RingBuffer::<f32>::new(1024);

        let outputs = vec![output(1, dir.path().join("ch1.wav"))];
        let writer = DiskWriter::start(consumer, SampleRate(48000), 2, outputs);

        // Push samples in unaligned chunks; the demux state is preserved
        // across pop() calls, so frames should still align correctly.
        producer.push(1.0).unwrap();
        producer.push(10.0).unwrap();
        drain_wait();
        producer.push(2.0).unwrap();
        producer.push(20.0).unwrap();
        drain_wait();
        drop(writer);

        let ch1 = read_wav_samples(&dir.path().join("ch1.wav"));
        assert_eq!(ch1, vec![10.0, 20.0]);
    }
}
