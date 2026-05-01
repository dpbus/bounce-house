use std::fs;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::audio::{ChannelOutput, DiskWriter, EngineHandle};
use crate::session::Session;

#[derive(Debug)]
pub enum CaptureError {
    NothingArmed,
}

pub struct Capture {
    start_sample: u64,
    engine_position: Arc<AtomicU64>,
    writer: DiskWriter,
    total_paused_samples: u64,
    pause_start_sample: Option<u64>,
}

impl Capture {
    pub fn start(engine: &EngineHandle, session: &mut Session) -> Result<Self, CaptureError> {
        let recorded = session
            .start_recording()
            .ok_or(CaptureError::NothingArmed)?;

        let outputs: Vec<ChannelOutput> = recorded
            .iter()
            .map(|r| ChannelOutput {
                channel: r.index,
                path: session.dir().join(&r.file),
            })
            .collect();

        for output in &outputs {
            if let Some(parent) = output.path.parent() {
                fs::create_dir_all(parent).expect("Failed to create channel dir");
            }
        }

        let start_sample = engine.sample_position();
        let engine_position = engine.sample_position_atomic();
        let consumer = engine.attach_consumer();
        let writer = DiskWriter::start(
            consumer,
            engine.sample_rate(),
            engine.channel_count(),
            outputs,
        );

        Ok(Self {
            start_sample,
            engine_position,
            writer,
            total_paused_samples: 0,
            pause_start_sample: None,
        })
    }

    pub fn stop(self, engine: &EngineHandle, session: &mut Session) {
        engine.detach_consumer();
        let rel_end = self.rel_sample_position();
        session.stop_recording(rel_end);
        // self drops here, joining the writer thread
    }

    #[allow(dead_code)]
    pub fn pause(&mut self, engine: &EngineHandle) -> bool {
        if self.is_paused() {
            return false;
        }
        self.pause_start_sample = Some(self.engine_position.load(Ordering::Relaxed));
        engine.pause();
        true
    }

    #[allow(dead_code)]
    pub fn resume(&mut self, engine: &EngineHandle) -> bool {
        let Some(started) = self.pause_start_sample.take() else {
            return false;
        };
        let now = self.engine_position.load(Ordering::Relaxed);
        self.total_paused_samples += now.saturating_sub(started);
        engine.resume();
        true
    }

    #[allow(dead_code)]
    pub fn is_paused(&self) -> bool {
        self.pause_start_sample.is_some()
    }

    pub fn rel_sample_position(&self) -> u64 {
        rel_sample_position(
            self.engine_position.load(Ordering::Relaxed),
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
    engine_position: u64,
    start_sample: u64,
    total_paused_samples: u64,
    pause_start_sample: Option<u64>,
) -> u64 {
    let position = pause_start_sample.unwrap_or(engine_position);
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
        // Paused at engine sample 4000; engine kept ticking to 7000.
        // Recorded time should still read 3000 (= 4000 - 1000), no drift.
        assert_eq!(rel_sample_position(7000, 1000, 0, Some(4000)), 3000);
    }

    #[test]
    fn rel_sample_position_subtracts_accumulated_pauses_after_resume() {
        // 3000 samples of past pause, now running, engine at 8000.
        assert_eq!(rel_sample_position(8000, 1000, 3000, None), 4000);
    }

    #[test]
    fn rel_sample_position_subtracts_both_accumulated_and_active_pause() {
        // 3000 samples of past pause + paused again at engine 9000.
        // Engine drifts to 10000; rel time freezes at 9000 - 1000 - 3000.
        assert_eq!(rel_sample_position(10000, 1000, 3000, Some(9000)), 5000);
    }

    #[test]
    fn rel_sample_position_saturates_when_engine_before_start() {
        // Defensive — shouldn't happen, but mustn't wrap around.
        assert_eq!(rel_sample_position(500, 1000, 0, None), 0);
    }
}
