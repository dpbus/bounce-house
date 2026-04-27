use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};

use cpal::traits::StreamTrait;

use crate::audio::Device;
use crate::audio::levels::{ChannelLevel, LevelObservation, MAX_CHANNELS};
use crate::units::SampleRate;

pub const RECORDING_BUFFER_SECONDS: usize = 10;

/// ~10s of headroom at typical macOS callback rates (~93 Hz).
const LEVEL_BUFFER_CAPACITY: usize = 1000;

/// UI-side handle to the audio engine. Owns the cpal stream's lifetime,
/// shares atomic state with the audio thread (lock-free reads from UI),
/// and sends control commands.
pub struct EngineHandle {
    _stream: cpal::Stream,
    device: Device,
    levels: Arc<[ChannelLevel]>,
    sample_position: Arc<AtomicU64>,
    cmd_tx: Sender<Command>,
}

enum Command {
    StartRecording { raw_producer: rtrb::Producer<f32> },
    StopRecording { ack_tx: Sender<()> },
}

/// Audio-thread-side state. Lives in the cpal callback closure. Owns the
/// working peak buffer, the recording producer (when recording), and the
/// levels-observation producer; reads atomic state shared with the handle.
struct Engine {
    levels: Arc<[ChannelLevel]>,
    sample_position: Arc<AtomicU64>,
    total_channel_count: usize,
    peaks_buf: Vec<f32>,
    raw_producer: Option<rtrb::Producer<f32>>,
    levels_producer: rtrb::Producer<LevelObservation>,
}

impl EngineHandle {
    pub fn start(device: Device) -> (Self, rtrb::Consumer<LevelObservation>) {
        let total_channel_count = device.channel_count() as usize;
        assert!(
            total_channel_count <= MAX_CHANNELS,
            "device has {total_channel_count} channels; MAX_CHANNELS={MAX_CHANNELS}",
        );
        let levels: Arc<[ChannelLevel]> = (0..total_channel_count)
            .map(|_| ChannelLevel::new())
            .collect::<Vec<_>>()
            .into();
        let sample_position = Arc::new(AtomicU64::new(0));
        let (cmd_tx, cmd_rx) = mpsc::channel::<Command>();
        let (levels_producer, levels_consumer) =
            rtrb::RingBuffer::<LevelObservation>::new(LEVEL_BUFFER_CAPACITY);

        let mut engine = Engine {
            levels: levels.clone(),
            sample_position: sample_position.clone(),
            total_channel_count,
            peaks_buf: vec![0.0; total_channel_count],
            raw_producer: None,
            levels_producer,
        };

        let stream = device.build_input_stream(move |data: &[f32]| {
            engine.drain_commands(&cmd_rx);
            let frames = engine.scan_peaks(data);
            let callback_start_sample = engine.advance_sample_position(frames);
            engine.publish_observation(callback_start_sample);
            engine.push_raw_if_recording(data);
        });

        stream.play().expect("Failed to start audio stream");

        let handle = EngineHandle {
            _stream: stream,
            device,
            levels,
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

    pub fn levels(&self) -> &[ChannelLevel] {
        &self.levels
    }

    pub fn sample_position(&self) -> u64 {
        self.sample_position.load(Ordering::Relaxed)
    }

    pub fn start_recording(&self) -> rtrb::Consumer<f32> {
        let total_samples_buffer =
            self.channel_count() as usize * self.sample_rate().0 as usize * RECORDING_BUFFER_SECONDS;
        let (raw_producer, raw_consumer) = rtrb::RingBuffer::new(total_samples_buffer);
        self.cmd_tx
            .send(Command::StartRecording { raw_producer })
            .expect("audio thread dropped");
        raw_consumer
    }

    pub fn stop_recording(&self) {
        let (ack_tx, ack_rx) = mpsc::channel::<()>();
        self.cmd_tx
            .send(Command::StopRecording { ack_tx })
            .expect("audio thread dropped");
        let _ = ack_rx.recv();
    }
}

impl Engine {
    fn drain_commands(&mut self, cmd_rx: &Receiver<Command>) {
        while let Ok(cmd) = cmd_rx.try_recv() {
            match cmd {
                Command::StartRecording { raw_producer } => {
                    self.raw_producer = Some(raw_producer);
                }
                Command::StopRecording { ack_tx } => {
                    self.raw_producer = None;
                    let _ = ack_tx.send(());
                }
            }
        }
    }

    /// Per-channel absolute peak across the callback. Fills `peaks_buf`
    /// and forwards to the meter atomics. Returns the frame count.
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
        for (level, &peak) in self.levels.iter().zip(self.peaks_buf.iter()) {
            level.record(peak);
        }
        frames
    }

    fn advance_sample_position(&self, frames: usize) -> u64 {
        self.sample_position
            .fetch_add(frames as u64, Ordering::Relaxed)
    }

    /// Push one observation per callback. Drop on backpressure — UI can
    /// fall behind without harming capture.
    fn publish_observation(&mut self, callback_start_sample: u64) {
        let mut channel_peaks = [0.0f32; MAX_CHANNELS];
        channel_peaks[..self.total_channel_count].copy_from_slice(&self.peaks_buf);
        let _ = self.levels_producer.push(LevelObservation {
            sample: callback_start_sample,
            recorded: self.raw_producer.is_some(),
            channel_peaks,
        });
    }

    fn push_raw_if_recording(&mut self, data: &[f32]) {
        let Some(producer) = &mut self.raw_producer else { return };
        // rtrb full → drop this whole buffer's data, alignment intact.
        let Ok(mut chunk) = producer.write_chunk(data.len()) else { return };
        let (slice1, slice2) = chunk.as_mut_slices();
        let split = slice1.len();
        slice1.copy_from_slice(&data[..split]);
        slice2.copy_from_slice(&data[split..]);
        chunk.commit_all();
    }
}
