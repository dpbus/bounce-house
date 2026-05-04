#![allow(dead_code)]

use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use hound::{WavIntoSamples, WavReader};

pub struct TrackReader {
    stop_signal: Arc<AtomicBool>,
    reader_thread: Option<JoinHandle<()>>,
}

impl TrackReader {
    pub fn start(
        producer: rtrb::Producer<f32>,
        output_channel_count: u16,
        track_paths: Vec<PathBuf>,
    ) -> Self {
        let stop_signal = Arc::new(AtomicBool::new(false));
        let reader_thread = {
            let stop_signal = stop_signal.clone();
            thread::spawn(move || {
                read_from_disk(producer, output_channel_count, track_paths, stop_signal)
            })
        };
        TrackReader {
            stop_signal,
            reader_thread: Some(reader_thread),
        }
    }
}

impl Drop for TrackReader {
    fn drop(&mut self) {
        self.stop_signal.store(true, Ordering::Relaxed);
        if let Some(handle) = self.reader_thread.take() {
            let _ = handle.join();
        }
    }
}

fn read_from_disk(
    mut producer: rtrb::Producer<f32>,
    output_channel_count: u16,
    track_paths: Vec<PathBuf>,
    stop_signal: Arc<AtomicBool>,
) {
    let mut streams: Vec<TrackStream> = track_paths.iter().map(|p| TrackStream::open(p)).collect();
    let out_channels = output_channel_count as usize;

    loop {
        if stop_signal.load(Ordering::Relaxed) {
            break;
        }

        while producer.slots() >= out_channels {
            let Some(sum) = mix_one_frame(&mut streams) else {
                return;
            };
            for _ in 0..out_channels {
                producer.push(sum).expect("Failed to push to producer");
            }
        }

        thread::sleep(Duration::from_millis(10));
    }
}

fn mix_one_frame(streams: &mut [TrackStream]) -> Option<f32> {
    let mut sum = 0.0_f32;
    let mut any_active = false;
    for stream in streams {
        if let Some(sample) = stream.next_sample() {
            sum += sample;
            any_active = true;
        }
    }
    any_active.then_some(sum)
}

struct TrackStream {
    samples: WavIntoSamples<BufReader<File>, f32>,
    finished: bool,
}

impl TrackStream {
    fn open(path: &Path) -> Self {
        let reader = WavReader::open(path).expect("Failed to open WAV file");
        TrackStream {
            samples: reader.into_samples::<f32>(),
            finished: false,
        }
    }

    fn next_sample(&mut self) -> Option<f32> {
        if self.finished {
            return None;
        }
        match self.samples.next() {
            Some(Ok(sample)) => Some(sample),
            Some(Err(_)) | None => {
                self.finished = true;
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hound::{SampleFormat, WavSpec, WavWriter};
    use tempfile::tempdir;

    fn write_mono_wav(path: &Path, samples: &[f32]) {
        let spec = WavSpec {
            channels: 1,
            sample_rate: 48000,
            bits_per_sample: 32,
            sample_format: SampleFormat::Float,
        };
        let mut writer = WavWriter::create(path, spec).unwrap();
        for &s in samples {
            writer.write_sample(s).unwrap();
        }
        writer.finalize().unwrap();
    }

    fn drain_wait() {
        // Reader sleeps 10ms between fills; two cycles is enough to catch up.
        thread::sleep(Duration::from_millis(30));
    }

    fn drain_consumer(consumer: &mut rtrb::Consumer<f32>) -> Vec<f32> {
        let mut out = Vec::new();
        while let Ok(s) = consumer.pop() {
            out.push(s);
        }
        out
    }

    #[test]
    fn single_track_duplicates_mono_to_stereo() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("a.wav");
        write_mono_wav(&path, &[0.1, 0.2, 0.3]);

        let (producer, mut consumer) = rtrb::RingBuffer::<f32>::new(1024);
        let reader = TrackReader::start(producer, 2, vec![path]);

        drain_wait();
        drop(reader);

        assert_eq!(
            drain_consumer(&mut consumer),
            vec![0.1, 0.1, 0.2, 0.2, 0.3, 0.3]
        );
    }

    #[test]
    fn two_tracks_are_summed_per_frame() {
        let dir = tempdir().unwrap();
        let a = dir.path().join("a.wav");
        let b = dir.path().join("b.wav");
        write_mono_wav(&a, &[0.5, 0.5, 0.5]);
        write_mono_wav(&b, &[0.25, 0.25, 0.25]);

        let (producer, mut consumer) = rtrb::RingBuffer::<f32>::new(1024);
        let reader = TrackReader::start(producer, 2, vec![a, b]);

        drain_wait();
        drop(reader);

        assert_eq!(
            drain_consumer(&mut consumer),
            vec![0.75, 0.75, 0.75, 0.75, 0.75, 0.75]
        );
    }

    #[test]
    fn shorter_track_drops_out_longer_continues() {
        let dir = tempdir().unwrap();
        let short = dir.path().join("short.wav");
        let long = dir.path().join("long.wav");
        write_mono_wav(&short, &[1.0]);
        write_mono_wav(&long, &[0.1, 0.2, 0.3]);

        let (producer, mut consumer) = rtrb::RingBuffer::<f32>::new(1024);
        let reader = TrackReader::start(producer, 2, vec![short, long]);

        drain_wait();
        drop(reader);

        // Frame 0: 1.0 + 0.1 = 1.1; frame 1: 0.2; frame 2: 0.3.
        assert_eq!(
            drain_consumer(&mut consumer),
            vec![1.1, 1.1, 0.2, 0.2, 0.3, 0.3]
        );
    }

    #[test]
    fn mono_output_writes_one_sample_per_frame() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("a.wav");
        write_mono_wav(&path, &[0.4, 0.5]);

        let (producer, mut consumer) = rtrb::RingBuffer::<f32>::new(1024);
        let reader = TrackReader::start(producer, 1, vec![path]);

        drain_wait();
        drop(reader);

        assert_eq!(drain_consumer(&mut consumer), vec![0.4, 0.5]);
    }

    #[test]
    fn drop_completes_when_producer_is_full() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("a.wav");
        // Long enough that the reader can't drain it before drop fires.
        let samples: Vec<f32> = (0..10_000).map(|i| i as f32).collect();
        write_mono_wav(&path, &samples);

        let (producer, _consumer) = rtrb::RingBuffer::<f32>::new(64);
        let reader = TrackReader::start(producer, 2, vec![path]);

        // Don't drain; producer fills and reader sleeps. Drop should
        // wake it and join without hanging.
        drop(reader);
    }
}
