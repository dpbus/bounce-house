use std::fs;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::audio::{AudioInput, ChannelOutput, DiskWriter};
use crate::channel::Channel;
use crate::session::Session;

#[derive(Debug)]
pub enum CaptureError {
    NothingArmed,
}

pub struct Capture {
    start_sample: u64,
    audio_input_position: Arc<AtomicU64>,
    writer: DiskWriter,
    total_paused_samples: u64,
    pause_start_sample: Option<u64>,
}

impl Capture {
    pub fn start(
        audio_input: &AudioInput,
        session: &mut Session,
        armed_channels: &[Channel],
    ) -> Result<Self, CaptureError> {
        let recorded = session
            .start_recording(armed_channels)
            .ok_or(CaptureError::NothingArmed)?;

        let outputs: Vec<ChannelOutput> = recorded
            .iter()
            .map(|track| ChannelOutput {
                channel: track.index,
                path: session.dir().join(&track.file),
            })
            .collect();

        for output in &outputs {
            if let Some(parent) = output.path.parent() {
                fs::create_dir_all(parent).expect("Failed to create tracks dir");
            }
        }

        let start_sample = audio_input.sample_position();
        let audio_input_position = audio_input.sample_position_atomic();
        let consumer = audio_input.attach_consumer();
        let writer = DiskWriter::start(
            consumer,
            audio_input.sample_rate(),
            audio_input.channel_count(),
            outputs,
        );

        Ok(Self {
            start_sample,
            audio_input_position,
            writer,
            total_paused_samples: 0,
            pause_start_sample: None,
        })
    }

    pub fn stop(self, audio_input: &AudioInput, session: &mut Session) {
        audio_input.detach_consumer();
        let rel_end = self.rel_sample_position();
        session.stop_recording(rel_end);
        // self drops here, joining the writer thread
    }

    pub fn pause(&mut self, audio_input: &AudioInput) -> bool {
        if self.is_paused() {
            return false;
        }
        self.pause_start_sample = Some(self.audio_input_position.load(Ordering::Relaxed));
        audio_input.pause();
        true
    }

    pub fn resume(&mut self, audio_input: &AudioInput) -> bool {
        let Some(started) = self.pause_start_sample.take() else {
            return false;
        };
        let now = self.audio_input_position.load(Ordering::Relaxed);
        self.total_paused_samples += now.saturating_sub(started);
        audio_input.resume();
        true
    }

    pub fn is_paused(&self) -> bool {
        self.pause_start_sample.is_some()
    }

    pub fn rel_sample_position(&self) -> u64 {
        rel_sample_position(
            self.audio_input_position.load(Ordering::Relaxed),
            self.start_sample,
            self.total_paused_samples,
            self.pause_start_sample,
        )
    }

    pub fn absolute(&self, rel: u64) -> u64 {
        self.start_sample + rel
    }

    pub fn flushed_samples(&self) -> Arc<AtomicU64> {
        self.writer.flushed_samples()
    }
}

fn rel_sample_position(
    audio_input_position: u64,
    start_sample: u64,
    total_paused_samples: u64,
    pause_start_sample: Option<u64>,
) -> u64 {
    let position = pause_start_sample.unwrap_or(audio_input_position);
    position
        .saturating_sub(start_sample)
        .saturating_sub(total_paused_samples)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rel_sample_position_no_pause() {
        assert_eq!(rel_sample_position(5000, 1000, 0, None), 4000);
    }

    #[test]
    fn rel_sample_position_freezes_during_pause() {
        // Paused at audio-input sample 4000; input kept ticking to 7000.
        // Recorded time should still read 3000 (= 4000 - 1000), no drift.
        assert_eq!(rel_sample_position(7000, 1000, 0, Some(4000)), 3000);
    }

    #[test]
    fn rel_sample_position_subtracts_accumulated_pauses_after_resume() {
        // 3000 samples of past pause, now running, input at 8000.
        assert_eq!(rel_sample_position(8000, 1000, 3000, None), 4000);
    }

    #[test]
    fn rel_sample_position_subtracts_both_accumulated_and_active_pause() {
        // 3000 samples of past pause + paused again at input 9000.
        // Input drifts to 10000; rel time freezes at 9000 - 1000 - 3000.
        assert_eq!(rel_sample_position(10000, 1000, 3000, Some(9000)), 5000);
    }

    #[test]
    fn rel_sample_position_saturates_when_input_before_start() {
        // Defensive — shouldn't happen, but mustn't wrap around.
        assert_eq!(rel_sample_position(500, 1000, 0, None), 0);
    }
}
