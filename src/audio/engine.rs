use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Sender};

use cpal::traits::StreamTrait;

use crate::audio::Device;
use crate::audio::levels::ChannelLevel;
use crate::units::SampleRate;

pub const RECORDING_BUFFER_SECONDS: usize = 10;

pub struct Engine {
    _stream: cpal::Stream,
    device: Device,
    levels: Arc<[ChannelLevel]>,
    sample_position: Arc<AtomicU64>,
    cmd_tx: Sender<Command>,
}

enum Command {
    StartRecording { producer: rtrb::Producer<f32> },
    StopRecording { ack_tx: Sender<()> },
}

struct CallbackState {
    levels: Arc<[ChannelLevel]>,
    sample_position: Arc<AtomicU64>,
    total_channel_count: usize,
    peaks_buf: Vec<f32>,
    producer: Option<rtrb::Producer<f32>>,
}

impl Engine {
    pub fn start(device: Device) -> Self {
        let total_channel_count = device.channel_count() as usize;
        let levels: Arc<[ChannelLevel]> = (0..total_channel_count)
            .map(|_| ChannelLevel::new())
            .collect::<Vec<_>>()
            .into();
        let sample_position = Arc::new(AtomicU64::new(0));
        let (cmd_tx, cmd_rx) = mpsc::channel::<Command>();

        let mut state = CallbackState {
            levels: levels.clone(),
            sample_position: sample_position.clone(),
            total_channel_count,
            peaks_buf: vec![0.0; total_channel_count],
            producer: None,
        };

        let stream = device.build_input_stream(move |data: &[f32]| {
            // 1. Drain control commands
            while let Ok(cmd) = cmd_rx.try_recv() {
                match cmd {
                    Command::StartRecording { producer } => {
                        state.producer = Some(producer);
                    }
                    Command::StopRecording { ack_tx } => {
                        state.producer = None;
                        let _ = ack_tx.send(());
                    }
                }
            }

            // 2. Publish per-channel absolute peak (always)
            state.peaks_buf.fill(0.0);
            let frames = data.len() / state.total_channel_count;
            for frame in 0..frames {
                for ch in 0..state.total_channel_count {
                    let sample = data[frame * state.total_channel_count + ch].abs();
                    if sample > state.peaks_buf[ch] {
                        state.peaks_buf[ch] = sample;
                    }
                }
            }
            for (level, &peak) in state.levels.iter().zip(state.peaks_buf.iter()) {
                level.record(peak);
            }

            // 3. Advance sample position
            state.sample_position.fetch_add(frames as u64, Ordering::Relaxed);

            // 4. If recording, push the entire raw buffer atomically
            if let Some(producer) = &mut state.producer {
                if let Ok(mut chunk) = producer.write_chunk(data.len()) {
                    let (slice1, slice2) = chunk.as_mut_slices();
                    let split = slice1.len();
                    slice1.copy_from_slice(&data[..split]);
                    slice2.copy_from_slice(&data[split..]);
                    chunk.commit_all();
                }
                // else: rtrb full — drop this whole buffer's data, alignment intact
            }
        });

        stream.play().expect("Failed to start audio stream");

        Engine {
            _stream: stream,
            device,
            levels,
            sample_position,
            cmd_tx,
        }
    }

    pub fn name(&self) -> &str {
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
        let (producer, consumer) = rtrb::RingBuffer::new(total_samples_buffer);
        self.cmd_tx
            .send(Command::StartRecording { producer })
            .expect("audio thread dropped");
        consumer
    }

    pub fn stop_recording(&self) {
        let (ack_tx, ack_rx) = mpsc::channel::<()>();
        self.cmd_tx
            .send(Command::StopRecording { ack_tx })
            .expect("audio thread dropped");
        let _ = ack_rx.recv();
    }
}
