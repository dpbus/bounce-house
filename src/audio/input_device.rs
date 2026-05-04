use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Sender};

use cpal::traits::{DeviceTrait, StreamTrait};

use crate::audio::DeviceInfo;
use crate::audio::input_callback::{Command, InputCallback, LEVEL_BUFFER_CAPACITY};
use crate::audio::levels::{LevelObservation, MAX_CHANNELS};
use crate::units::SampleRate;

const RECORDING_BUFFER_SECONDS: usize = 10;

/// The running input device: physical hardware + active cpal stream +
/// audio-thread state. Constructed via `DeviceInfo::start_input()`.
pub struct InputDevice {
    _stream: cpal::Stream,
    name: String,
    channel_count: u16,
    sample_rate: SampleRate,
    sample_position: Arc<AtomicU64>,
    cmd_tx: Sender<Command>,
}

impl InputDevice {
    pub fn start(info: &DeviceInfo) -> (Self, rtrb::Consumer<LevelObservation>) {
        let cpal_device = info.cpal_device.clone();
        let name = info.name.clone();
        let input_config: cpal::StreamConfig = cpal_device
            .default_input_config()
            .expect("No default input config")
            .into();
        let channel_count = input_config.channels;
        let sample_rate = SampleRate(input_config.sample_rate);
        let total_channel_count = channel_count as usize;
        assert!(
            total_channel_count <= MAX_CHANNELS,
            "device has {total_channel_count} channels; MAX_CHANNELS={MAX_CHANNELS}",
        );

        let sample_position = Arc::new(AtomicU64::new(0));
        let (cmd_tx, cmd_rx) = mpsc::channel::<Command>();
        let (levels_producer, levels_consumer) =
            rtrb::RingBuffer::<LevelObservation>::new(LEVEL_BUFFER_CAPACITY);

        let mut callback = InputCallback {
            sample_position: sample_position.clone(),
            total_channel_count,
            peaks_buf: vec![0.0; total_channel_count],
            consumer_attachment: None,
            levels_producer,
        };

        let stream = cpal_device
            .build_input_stream(
                &input_config,
                move |data: &[f32], _| {
                    callback.drain_commands(&cmd_rx);
                    let frames = callback.scan_peaks(data);
                    let callback_start_sample = callback.advance_sample_position(frames);
                    callback.publish_observation(callback_start_sample);
                    callback.push_raw_if_attached(data);
                },
                |err| eprintln!("Stream error: {}", err),
                None,
            )
            .expect("Failed to build input stream");

        stream.play().expect("Failed to start audio stream");

        let device = InputDevice {
            _stream: stream,
            name,
            channel_count,
            sample_rate,
            sample_position,
            cmd_tx,
        };
        (device, levels_consumer)
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn channel_count(&self) -> u16 {
        self.channel_count
    }

    pub fn sample_rate(&self) -> SampleRate {
        self.sample_rate
    }

    pub fn sample_position(&self) -> u64 {
        self.sample_position.load(Ordering::Relaxed)
    }

    pub(crate) fn sample_position_atomic(&self) -> Arc<AtomicU64> {
        Arc::clone(&self.sample_position)
    }

    pub fn attach_consumer(&self) -> (rtrb::Consumer<f32>, ConsumerControl) {
        let total_samples_buffer =
            self.channel_count as usize * self.sample_rate.0 as usize * RECORDING_BUFFER_SECONDS;
        let (producer, consumer) = rtrb::RingBuffer::new(total_samples_buffer);
        let pause_signal = Arc::new(AtomicBool::new(false));
        self.cmd_tx
            .send(Command::AttachConsumer {
                producer,
                pause_signal: pause_signal.clone(),
            })
            .expect("audio thread dropped");
        let control = ConsumerControl {
            pause_signal,
            cmd_tx: self.cmd_tx.clone(),
        };
        (consumer, control)
    }
}

/// Control side of an attached consumer. Returned alongside the
/// `Consumer` from `attach_consumer`. Lets the attached party pause/
/// resume the audio thread's pushes and detach when done — without
/// holding a reference back to InputDevice.
pub struct ConsumerControl {
    pause_signal: Arc<AtomicBool>,
    cmd_tx: Sender<Command>,
}

impl ConsumerControl {
    pub fn pause(&self) {
        self.pause_signal.store(true, Ordering::Relaxed);
    }

    pub fn resume(&self) {
        self.pause_signal.store(false, Ordering::Relaxed);
    }

    pub fn detach(self) {
        let (ack_tx, ack_rx) = mpsc::channel::<()>();
        let _ = self.cmd_tx.send(Command::DetachConsumer { ack_tx });
        let _ = ack_rx.recv();
    }
}
