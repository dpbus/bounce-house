use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};

use cpal::traits::StreamTrait;

use crate::audio::Device;
use crate::audio::levels::{LevelObservation, MAX_CHANNELS};
use crate::units::SampleRate;

pub const RECORDING_BUFFER_SECONDS: usize = 10;

/// ~10s of headroom at typical macOS callback rates (~93 Hz).
const LEVEL_BUFFER_CAPACITY: usize = 1000;

/// UI-side handle. Owns the cpal stream, shares atomic state with the
/// audio thread, and sends control commands.
pub struct EngineHandle {
    _stream: cpal::Stream,
    device: Device,
    sample_position: Arc<AtomicU64>,
    cmd_tx: Sender<Command>,
}

enum Command {
    AttachConsumer { producer: rtrb::Producer<f32> },
    DetachConsumer { ack_tx: Sender<()> },
    Pause,
    Resume,
}

/// Audio-thread state. Lives in the cpal callback closure; owns working
/// buffers and producers, reads atomics shared with the handle.
struct Engine {
    sample_position: Arc<AtomicU64>,
    total_channel_count: usize,
    peaks_buf: Vec<f32>,
    raw_producer: Option<rtrb::Producer<f32>>,
    levels_producer: rtrb::Producer<LevelObservation>,
    paused: bool,
}

impl EngineHandle {
    pub fn start(device: Device) -> (Self, rtrb::Consumer<LevelObservation>) {
        let total_channel_count = device.channel_count() as usize;
        assert!(
            total_channel_count <= MAX_CHANNELS,
            "device has {total_channel_count} channels; MAX_CHANNELS={MAX_CHANNELS}",
        );
        let sample_position = Arc::new(AtomicU64::new(0));
        let (cmd_tx, cmd_rx) = mpsc::channel::<Command>();
        let (levels_producer, levels_consumer) =
            rtrb::RingBuffer::<LevelObservation>::new(LEVEL_BUFFER_CAPACITY);

        let mut engine = Engine {
            sample_position: sample_position.clone(),
            total_channel_count,
            peaks_buf: vec![0.0; total_channel_count],
            raw_producer: None,
            levels_producer,
            paused: false,
        };

        let stream = device.build_input_stream(move |data: &[f32]| {
            engine.drain_commands(&cmd_rx);
            let frames = engine.scan_peaks(data);
            let callback_start_sample = engine.advance_sample_position(frames);
            engine.publish_observation(callback_start_sample);
            engine.push_raw_if_attached(data);
        });

        stream.play().expect("Failed to start audio stream");

        let handle = EngineHandle {
            _stream: stream,
            device,
            sample_position,
            cmd_tx,
        };
        (handle, levels_consumer)
    }

    pub fn device_name(&self) -> &str {
        self.device.name()
    }

    pub fn channel_count(&self) -> u16 {
        self.device.channel_count()
    }

    pub fn sample_rate(&self) -> SampleRate {
        self.device.sample_rate()
    }

    pub fn sample_position(&self) -> u64 {
        self.sample_position.load(Ordering::Relaxed)
    }

    pub(crate) fn sample_position_atomic(&self) -> Arc<AtomicU64> {
        Arc::clone(&self.sample_position)
    }

    pub fn attach_consumer(&self) -> rtrb::Consumer<f32> {
        let total_samples_buffer = self.channel_count() as usize
            * self.sample_rate().0 as usize
            * RECORDING_BUFFER_SECONDS;
        let (producer, consumer) = rtrb::RingBuffer::new(total_samples_buffer);
        self.cmd_tx
            .send(Command::AttachConsumer { producer })
            .expect("audio thread dropped");
        consumer
    }

    pub fn detach_consumer(&self) {
        let (ack_tx, ack_rx) = mpsc::channel::<()>();
        self.cmd_tx
            .send(Command::DetachConsumer { ack_tx })
            .expect("audio thread dropped");
        let _ = ack_rx.recv();
    }

    pub fn pause(&self) {
        let _ = self.cmd_tx.send(Command::Pause);
    }

    pub fn resume(&self) {
        let _ = self.cmd_tx.send(Command::Resume);
    }
}

impl Engine {
    fn drain_commands(&mut self, cmd_rx: &Receiver<Command>) {
        while let Ok(cmd) = cmd_rx.try_recv() {
            match cmd {
                Command::AttachConsumer { producer } => {
                    self.raw_producer = Some(producer);
                }
                Command::DetachConsumer { ack_tx } => {
                    self.raw_producer = None;
                    let _ = ack_tx.send(());
                }
                Command::Pause => {
                    self.paused = true;
                }
                Command::Resume => {
                    self.paused = false;
                }
            }
        }
    }

    /// Per-channel absolute peak across the callback. Fills `peaks_buf`
    /// and returns the frame count.
    fn scan_peaks(&mut self, data: &[f32]) -> usize {
        self.peaks_buf.fill(0.0);
        let frames = data.len() / self.total_channel_count;
        for frame in 0..frames {
            for ch in 0..self.total_channel_count {
                let sample = data[frame * self.total_channel_count + ch].abs();
                if sample > self.peaks_buf[ch] {
                    self.peaks_buf[ch] = sample;
                }
            }
        }
        frames
    }

    fn advance_sample_position(&self, frames: usize) -> u64 {
        self.sample_position
            .fetch_add(frames as u64, Ordering::Relaxed)
    }

    /// One push per callback. Backpressure drops silently; UI lag must
    /// not affect capture.
    fn publish_observation(&mut self, callback_start_sample: u64) {
        let mut channel_peaks = [0.0f32; MAX_CHANNELS];
        channel_peaks[..self.total_channel_count].copy_from_slice(&self.peaks_buf);
        let _ = self.levels_producer.push(LevelObservation {
            sample: callback_start_sample,
            channel_peaks,
        });
    }

    #[cfg(test)]
    fn for_test(channel_count: usize) -> (Self, rtrb::Consumer<LevelObservation>) {
        let sample_position = Arc::new(AtomicU64::new(0));
        let (levels_producer, levels_consumer) =
            rtrb::RingBuffer::<LevelObservation>::new(LEVEL_BUFFER_CAPACITY);
        let engine = Engine {
            sample_position,
            total_channel_count: channel_count,
            peaks_buf: vec![0.0; channel_count],
            raw_producer: None,
            levels_producer,
            paused: false,
        };
        (engine, levels_consumer)
    }

    fn push_raw_if_attached(&mut self, data: &[f32]) {
        if self.paused {
            return;
        }
        let Some(producer) = &mut self.raw_producer else {
            return;
        };
        // rtrb full → drop this whole buffer's data, alignment intact.
        let Ok(mut chunk) = producer.write_chunk(data.len()) else {
            return;
        };
        let (slice1, slice2) = chunk.as_mut_slices();
        let split = slice1.len();
        slice1.copy_from_slice(&data[..split]);
        slice2.copy_from_slice(&data[split..]);
        chunk.commit_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scan_peaks_finds_max_abs_per_channel() {
        let (mut engine, _) = Engine::for_test(2);
        // 3 frames × 2 channels, interleaved (frame-major):
        //   ch0 stream:  0.1, -0.5,  0.3 → max abs 0.5
        //   ch1 stream: -0.2,  0.4, -0.7 → max abs 0.7
        let data = [0.1, -0.2, -0.5, 0.4, 0.3, -0.7];
        let frames = engine.scan_peaks(&data);
        assert_eq!(frames, 3);
        assert!((engine.peaks_buf[0] - 0.5).abs() < 1e-6);
        assert!((engine.peaks_buf[1] - 0.7).abs() < 1e-6);
    }

    #[test]
    fn scan_peaks_resets_between_callbacks() {
        let (mut engine, _) = Engine::for_test(1);
        engine.scan_peaks(&[0.9]);
        assert!((engine.peaks_buf[0] - 0.9).abs() < 1e-6);
        // Next "callback" with smaller signal shouldn't carry old peak.
        engine.scan_peaks(&[0.1]);
        assert!((engine.peaks_buf[0] - 0.1).abs() < 1e-6);
    }

    #[test]
    fn scan_peaks_drops_partial_trailing_frame() {
        let (mut engine, _) = Engine::for_test(2);
        // 5 samples → 2 complete frames + 1 trailing sample (dropped).
        let data = [0.1, 0.2, 0.3, 0.4, 0.5];
        let frames = engine.scan_peaks(&data);
        assert_eq!(frames, 2);
        // Only first 4 samples are scanned; ch0={0.1, 0.3}, ch1={0.2, 0.4}.
        assert!((engine.peaks_buf[0] - 0.3).abs() < 1e-6);
        assert!((engine.peaks_buf[1] - 0.4).abs() < 1e-6);
    }

    #[test]
    fn scan_peaks_empty_input_yields_zero_frames() {
        let (mut engine, _) = Engine::for_test(2);
        let frames = engine.scan_peaks(&[]);
        assert_eq!(frames, 0);
        assert_eq!(engine.peaks_buf, vec![0.0, 0.0]);
    }

    #[test]
    fn advance_sample_position_returns_pre_advance_value() {
        let (engine, _) = Engine::for_test(2);
        // Atomic starts at 0; the first advance returns 0 (callback start)
        // and leaves the atomic at the new position.
        assert_eq!(engine.advance_sample_position(64), 0);
        assert_eq!(engine.sample_position.load(Ordering::Relaxed), 64);
        // Next advance returns the previous total (64) — that's the
        // callback_start_sample for the second buffer.
        assert_eq!(engine.advance_sample_position(64), 64);
        assert_eq!(engine.sample_position.load(Ordering::Relaxed), 128);
    }

    #[test]
    fn paused_engine_drops_raw_samples() {
        let (mut engine, _) = Engine::for_test(2);
        let (producer, mut consumer) = rtrb::RingBuffer::<f32>::new(64);
        engine.raw_producer = Some(producer);
        engine.paused = true;

        engine.push_raw_if_attached(&[0.1, 0.2, 0.3, 0.4]);

        assert!(consumer.pop().is_err());
    }

    #[test]
    fn unpaused_engine_forwards_raw_samples() {
        let (mut engine, _) = Engine::for_test(2);
        let (producer, consumer) = rtrb::RingBuffer::<f32>::new(64);
        engine.raw_producer = Some(producer);

        engine.push_raw_if_attached(&[0.1, 0.2, 0.3, 0.4]);

        assert_eq!(consumer.slots(), 4);
    }

    #[test]
    fn pause_resume_commands_flip_paused_flag() {
        let (mut engine, _) = Engine::for_test(2);
        let (cmd_tx, cmd_rx) = mpsc::channel::<Command>();

        cmd_tx.send(Command::Pause).unwrap();
        engine.drain_commands(&cmd_rx);
        assert!(engine.paused);

        cmd_tx.send(Command::Resume).unwrap();
        engine.drain_commands(&cmd_rx);
        assert!(!engine.paused);
    }

    #[test]
    fn publish_observation_pushes_sample_and_peaks() {
        let (mut engine, mut consumer) = Engine::for_test(2);
        engine.peaks_buf[0] = 0.3;
        engine.peaks_buf[1] = 0.7;
        engine.publish_observation(48_000);

        let obs = consumer.pop().expect("observation should be pushed");
        assert_eq!(obs.sample, 48_000);
        assert_eq!(obs.channel_peaks[0], 0.3);
        assert_eq!(obs.channel_peaks[1], 0.7);
        // Channels past total_channel_count remain at default zero.
        assert_eq!(obs.channel_peaks[2], 0.0);
    }
}
