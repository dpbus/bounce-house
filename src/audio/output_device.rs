use std::sync::mpsc::{self, Receiver, Sender};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use crate::units::SampleRate;

const PLAYBACK_BUFFER_SECONDS: usize = 10;

pub struct OutputDevice {
    _stream: cpal::Stream,
    name: String,
    channel_count: u16,
    sample_rate: SampleRate,
    cmd_tx: Sender<Command>,
}

enum Command {
    Attach(rtrb::Consumer<f32>),
    Detach,
}

/// Returned alongside the producer from `attach_producer`. Lets the
/// attached party tell the audio thread to release its consumer
/// immediately — without that signal the callback would keep draining
/// queued samples until empty (which is the right behavior for natural
/// end-of-stream, but not for explicit stop).
pub struct ProducerControl {
    cmd_tx: Sender<Command>,
}

impl ProducerControl {
    pub fn detach(self) {
        let _ = self.cmd_tx.send(Command::Detach);
    }
}

impl OutputDevice {
    /// Opens the host's default output device. Independent of the input
    /// device choice — temporary until the Settings picker grows separate
    /// input/output selection.
    pub fn start() -> Option<Self> {
        let cpal_device = cpal::default_host().default_output_device()?;
        let name = cpal_device
            .description()
            .map(|d| d.name().to_string())
            .unwrap_or_else(|_| "Unknown".to_string());
        let output_config: cpal::StreamConfig = cpal_device.default_output_config().ok()?.into();
        let channel_count = output_config.channels;
        let sample_rate = SampleRate(output_config.sample_rate);

        let (cmd_tx, cmd_rx) = mpsc::channel::<Command>();
        let mut callback = OutputCallback { consumer: None };

        let stream = cpal_device
            .build_output_stream(
                &output_config,
                move |data: &mut [f32], _| {
                    callback.drain_commands(&cmd_rx);
                    callback.fill(data);
                },
                |err| eprintln!("Output stream error: {}", err),
                None,
            )
            .expect("Failed to build output stream");
        stream.play().expect("Failed to start output stream");

        Some(OutputDevice {
            _stream: stream,
            name,
            channel_count,
            sample_rate,
            cmd_tx,
        })
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn channel_count(&self) -> u16 {
        self.channel_count
    }

    #[allow(dead_code)]
    pub fn sample_rate(&self) -> SampleRate {
        self.sample_rate
    }

    pub fn attach_producer(&self) -> (rtrb::Producer<f32>, ProducerControl) {
        let buffer_samples =
            self.channel_count as usize * self.sample_rate.0 as usize * PLAYBACK_BUFFER_SECONDS;
        let (producer, consumer) = rtrb::RingBuffer::<f32>::new(buffer_samples);
        self.cmd_tx
            .send(Command::Attach(consumer))
            .expect("audio thread dropped");
        let control = ProducerControl {
            cmd_tx: self.cmd_tx.clone(),
        };
        (producer, control)
    }
}

struct OutputCallback {
    consumer: Option<rtrb::Consumer<f32>>,
}

impl OutputCallback {
    fn drain_commands(&mut self, cmd_rx: &Receiver<Command>) {
        while let Ok(cmd) = cmd_rx.try_recv() {
            match cmd {
                Command::Attach(consumer) => self.consumer = Some(consumer),
                Command::Detach => self.consumer = None,
            }
        }
    }

    fn fill(&mut self, data: &mut [f32]) {
        let mut written = 0;
        if let Some(consumer) = &mut self.consumer {
            while written < data.len() {
                match consumer.pop() {
                    Ok(sample) => {
                        data[written] = sample;
                        written += 1;
                    }
                    Err(_) => {
                        // Buffer drained: only now is it safe to drop the
                        // consumer if the producer is gone. Checking earlier
                        // would discard queued samples.
                        if consumer.is_abandoned() {
                            self.consumer = None;
                        }
                        break;
                    }
                }
            }
        }
        data[written..].fill(0.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh_callback() -> (OutputCallback, Sender<Command>, Receiver<Command>) {
        let (tx, rx) = mpsc::channel();
        (OutputCallback { consumer: None }, tx, rx)
    }

    #[test]
    fn fill_with_no_consumer_writes_silence() {
        let (mut callback, _tx, rx) = fresh_callback();
        let mut buf = vec![1.0; 8];
        callback.drain_commands(&rx);
        callback.fill(&mut buf);
        assert_eq!(buf, vec![0.0; 8]);
    }

    #[test]
    fn fill_drains_attached_consumer_then_pads_silence() {
        let (mut callback, tx, rx) = fresh_callback();
        let (mut producer, consumer) = rtrb::RingBuffer::<f32>::new(16);
        for i in 1..=4 {
            producer.push(i as f32).unwrap();
        }
        tx.send(Command::Attach(consumer)).unwrap();
        callback.drain_commands(&rx);

        let mut buf = vec![99.0; 8];
        callback.fill(&mut buf);
        assert_eq!(buf, vec![1.0, 2.0, 3.0, 4.0, 0.0, 0.0, 0.0, 0.0]);
    }

    #[test]
    fn fill_clears_consumer_slot_when_producer_abandoned() {
        let (mut callback, tx, rx) = fresh_callback();
        let (mut producer, consumer) = rtrb::RingBuffer::<f32>::new(16);
        producer.push(0.5).unwrap();
        tx.send(Command::Attach(consumer)).unwrap();
        callback.drain_commands(&rx);
        drop(producer);

        let mut buf = vec![99.0; 4];
        callback.fill(&mut buf);
        assert_eq!(buf, vec![0.5, 0.0, 0.0, 0.0]);
        assert!(callback.consumer.is_none());
    }

    #[test]
    fn fill_keeps_consumer_while_buffer_has_samples_after_producer_drop() {
        let (mut callback, tx, rx) = fresh_callback();
        let (mut producer, consumer) = rtrb::RingBuffer::<f32>::new(16);
        for i in 1..=8 {
            producer.push(i as f32).unwrap();
        }
        tx.send(Command::Attach(consumer)).unwrap();
        callback.drain_commands(&rx);
        drop(producer);

        let mut buf = vec![0.0; 4];
        callback.fill(&mut buf);
        assert_eq!(buf, vec![1.0, 2.0, 3.0, 4.0]);
        assert!(
            callback.consumer.is_some(),
            "consumer must stay attached while buffered samples remain"
        );

        let mut buf = vec![0.0; 4];
        callback.fill(&mut buf);
        assert_eq!(buf, vec![5.0, 6.0, 7.0, 8.0]);

        let mut buf = vec![0.0; 4];
        callback.fill(&mut buf);
        assert_eq!(buf, vec![0.0, 0.0, 0.0, 0.0]);
        assert!(callback.consumer.is_none());
    }

    #[test]
    fn detach_clears_consumer_immediately_even_with_queued_samples() {
        let (mut callback, tx, rx) = fresh_callback();
        let (mut producer, consumer) = rtrb::RingBuffer::<f32>::new(16);
        for i in 1..=8 {
            producer.push(i as f32).unwrap();
        }
        tx.send(Command::Attach(consumer)).unwrap();
        callback.drain_commands(&rx);

        tx.send(Command::Detach).unwrap();
        callback.drain_commands(&rx);

        let mut buf = vec![99.0; 4];
        callback.fill(&mut buf);
        assert_eq!(buf, vec![0.0, 0.0, 0.0, 0.0]);
        assert!(callback.consumer.is_none());
    }

    #[test]
    fn drain_commands_replaces_existing_consumer() {
        let (mut callback, tx, rx) = fresh_callback();
        let (_p1, c1) = rtrb::RingBuffer::<f32>::new(8);
        let (mut p2, c2) = rtrb::RingBuffer::<f32>::new(8);
        p2.push(7.0).unwrap();

        tx.send(Command::Attach(c1)).unwrap();
        callback.drain_commands(&rx);
        tx.send(Command::Attach(c2)).unwrap();
        callback.drain_commands(&rx);

        let mut buf = vec![0.0; 1];
        callback.fill(&mut buf);
        assert_eq!(buf, vec![7.0]);
    }
}
