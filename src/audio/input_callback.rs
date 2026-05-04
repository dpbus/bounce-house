use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender};

use crate::audio::levels::{LevelObservation, MAX_CHANNELS};

/// ~10s of headroom at typical macOS callback rates (~93 Hz).
pub(super) const LEVEL_BUFFER_CAPACITY: usize = 1000;

pub(super) enum Command {
    AttachConsumer {
        producer: rtrb::Producer<f32>,
        pause_signal: Arc<AtomicBool>,
    },
    DetachConsumer {
        ack_tx: Sender<()>,
    },
}

pub(super) struct ConsumerAttachment {
    pub producer: rtrb::Producer<f32>,
    pub pause_signal: Arc<AtomicBool>,
}

/// Audio-thread state. Lives in the cpal callback closure; owns working
/// buffers and producers, reads atomics shared with the handle.
pub(super) struct InputCallback {
    pub sample_position: Arc<AtomicU64>,
    pub total_channel_count: usize,
    pub peaks_buf: Vec<f32>,
    pub consumer_attachment: Option<ConsumerAttachment>,
    pub levels_producer: rtrb::Producer<LevelObservation>,
}

impl InputCallback {
    pub fn drain_commands(&mut self, cmd_rx: &Receiver<Command>) {
        while let Ok(cmd) = cmd_rx.try_recv() {
            match cmd {
                Command::AttachConsumer {
                    producer,
                    pause_signal,
                } => {
                    self.consumer_attachment = Some(ConsumerAttachment {
                        producer,
                        pause_signal,
                    });
                }
                Command::DetachConsumer { ack_tx } => {
                    self.consumer_attachment = None;
                    let _ = ack_tx.send(());
                }
            }
        }
    }

    /// Per-channel absolute peak across the callback. Fills `peaks_buf`
    /// and returns the frame count.
    pub fn scan_peaks(&mut self, data: &[f32]) -> usize {
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

    pub fn advance_sample_position(&self, frames: usize) -> u64 {
        self.sample_position
            .fetch_add(frames as u64, Ordering::Relaxed)
    }

    /// One push per callback. Backpressure drops silently; UI lag must
    /// not affect capture.
    pub fn publish_observation(&mut self, callback_start_sample: u64) {
        let mut channel_peaks = [0.0f32; MAX_CHANNELS];
        channel_peaks[..self.total_channel_count].copy_from_slice(&self.peaks_buf);
        let _ = self.levels_producer.push(LevelObservation {
            sample: callback_start_sample,
            channel_peaks,
        });
    }

    pub fn push_raw_if_attached(&mut self, data: &[f32]) {
        let Some(attachment) = &mut self.consumer_attachment else {
            return;
        };
        if attachment.pause_signal.load(Ordering::Relaxed) {
            return;
        }
        // rtrb full → drop this whole buffer's data, alignment intact.
        let Ok(mut chunk) = attachment.producer.write_chunk(data.len()) else {
            return;
        };
        let (slice1, slice2) = chunk.as_mut_slices();
        let split = slice1.len();
        slice1.copy_from_slice(&data[..split]);
        slice2.copy_from_slice(&data[split..]);
        chunk.commit_all();
    }

    #[cfg(test)]
    pub fn for_test(channel_count: usize) -> (Self, rtrb::Consumer<LevelObservation>) {
        let sample_position = Arc::new(AtomicU64::new(0));
        let (levels_producer, levels_consumer) =
            rtrb::RingBuffer::<LevelObservation>::new(LEVEL_BUFFER_CAPACITY);
        let callback = InputCallback {
            sample_position,
            total_channel_count: channel_count,
            peaks_buf: vec![0.0; channel_count],
            consumer_attachment: None,
            levels_producer,
        };
        (callback, levels_consumer)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use super::*;

    fn attach_for_test(callback: &mut InputCallback, paused: bool) -> rtrb::Consumer<f32> {
        let (producer, consumer) = rtrb::RingBuffer::<f32>::new(64);
        callback.consumer_attachment = Some(ConsumerAttachment {
            producer,
            pause_signal: Arc::new(AtomicBool::new(paused)),
        });
        consumer
    }

    #[test]
    fn scan_peaks_finds_max_abs_per_channel() {
        let (mut callback, _) = InputCallback::for_test(2);
        // 3 frames × 2 channels, interleaved (frame-major):
        //   ch0 stream:  0.1, -0.5,  0.3 → max abs 0.5
        //   ch1 stream: -0.2,  0.4, -0.7 → max abs 0.7
        let data = [0.1, -0.2, -0.5, 0.4, 0.3, -0.7];
        let frames = callback.scan_peaks(&data);
        assert_eq!(frames, 3);
        assert!((callback.peaks_buf[0] - 0.5).abs() < 1e-6);
        assert!((callback.peaks_buf[1] - 0.7).abs() < 1e-6);
    }

    #[test]
    fn scan_peaks_resets_between_callbacks() {
        let (mut callback, _) = InputCallback::for_test(1);
        callback.scan_peaks(&[0.9]);
        assert!((callback.peaks_buf[0] - 0.9).abs() < 1e-6);
        // Next "callback" with smaller signal shouldn't carry old peak.
        callback.scan_peaks(&[0.1]);
        assert!((callback.peaks_buf[0] - 0.1).abs() < 1e-6);
    }

    #[test]
    fn scan_peaks_drops_partial_trailing_frame() {
        let (mut callback, _) = InputCallback::for_test(2);
        // 5 samples → 2 complete frames + 1 trailing sample (dropped).
        let data = [0.1, 0.2, 0.3, 0.4, 0.5];
        let frames = callback.scan_peaks(&data);
        assert_eq!(frames, 2);
        // Only first 4 samples are scanned; ch0={0.1, 0.3}, ch1={0.2, 0.4}.
        assert!((callback.peaks_buf[0] - 0.3).abs() < 1e-6);
        assert!((callback.peaks_buf[1] - 0.4).abs() < 1e-6);
    }

    #[test]
    fn scan_peaks_empty_input_yields_zero_frames() {
        let (mut callback, _) = InputCallback::for_test(2);
        let frames = callback.scan_peaks(&[]);
        assert_eq!(frames, 0);
        assert_eq!(callback.peaks_buf, vec![0.0, 0.0]);
    }

    #[test]
    fn advance_sample_position_returns_pre_advance_value() {
        let (callback, _) = InputCallback::for_test(2);
        // Atomic starts at 0; the first advance returns 0 (callback start)
        // and leaves the atomic at the new position.
        assert_eq!(callback.advance_sample_position(64), 0);
        assert_eq!(callback.sample_position.load(Ordering::Relaxed), 64);
        // Next advance returns the previous total (64) — that's the
        // callback_start_sample for the second buffer.
        assert_eq!(callback.advance_sample_position(64), 64);
        assert_eq!(callback.sample_position.load(Ordering::Relaxed), 128);
    }

    #[test]
    fn paused_attachment_drops_raw_samples() {
        let (mut callback, _) = InputCallback::for_test(2);
        let mut consumer = attach_for_test(&mut callback, true);

        callback.push_raw_if_attached(&[0.1, 0.2, 0.3, 0.4]);

        assert!(consumer.pop().is_err());
    }

    #[test]
    fn unpaused_attachment_forwards_raw_samples() {
        let (mut callback, _) = InputCallback::for_test(2);
        let consumer = attach_for_test(&mut callback, false);

        callback.push_raw_if_attached(&[0.1, 0.2, 0.3, 0.4]);

        assert_eq!(consumer.slots(), 4);
    }

    #[test]
    fn detached_callback_drops_raw_samples() {
        let (mut callback, _) = InputCallback::for_test(2);
        // No attachment at all — early return is the behavior.
        callback.push_raw_if_attached(&[0.1, 0.2]);
    }

    #[test]
    fn publish_observation_pushes_sample_and_peaks() {
        let (mut callback, mut consumer) = InputCallback::for_test(2);
        callback.peaks_buf[0] = 0.3;
        callback.peaks_buf[1] = 0.7;
        callback.publish_observation(48_000);

        let obs = consumer.pop().expect("observation should be pushed");
        assert_eq!(obs.sample, 48_000);
        assert_eq!(obs.channel_peaks[0], 0.3);
        assert_eq!(obs.channel_peaks[1], 0.7);
        // Channels past total_channel_count remain at default zero.
        assert_eq!(obs.channel_peaks[2], 0.0);
    }

    // Use mpsc to make the import non-dead in tests.
    #[test]
    fn drain_commands_processes_attach_then_detach() {
        let (mut callback, _) = InputCallback::for_test(2);
        let (cmd_tx, cmd_rx) = mpsc::channel::<Command>();
        let (producer, _consumer) = rtrb::RingBuffer::<f32>::new(64);
        let pause_signal = Arc::new(AtomicBool::new(false));
        cmd_tx
            .send(Command::AttachConsumer {
                producer,
                pause_signal,
            })
            .unwrap();
        callback.drain_commands(&cmd_rx);
        assert!(callback.consumer_attachment.is_some());

        let (ack_tx, ack_rx) = mpsc::channel::<()>();
        cmd_tx.send(Command::DetachConsumer { ack_tx }).unwrap();
        callback.drain_commands(&cmd_rx);
        assert!(callback.consumer_attachment.is_none());
        assert!(ack_rx.try_recv().is_ok());
    }
}
