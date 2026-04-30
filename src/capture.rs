use std::fs;
use std::path::PathBuf;
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
}

impl Capture {
    pub fn start(engine: &EngineHandle, session: &mut Session) -> Result<Self, CaptureError> {
        let max_index = engine.channel_count();
        let (outputs, channel_files): (Vec<ChannelOutput>, Vec<PathBuf>) = session
            .armed_channels()
            .filter(|c| c.index < max_index)
            .map(|c| {
                let output = ChannelOutput {
                    channel: c.index,
                    path: session.channel_path(c),
                };
                (output, session.channel_subpath(c))
            })
            .unzip();
        if outputs.is_empty() {
            return Err(CaptureError::NothingArmed);
        }

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
        session.start_recording(channel_files);

        Ok(Self {
            start_sample,
            engine_position,
            writer,
        })
    }

    pub fn stop(self, engine: &EngineHandle, session: &mut Session) {
        engine.detach_consumer();
        let rel_end = self.rel_sample_position();
        session.stop_recording(rel_end);
        // self drops here, joining the writer thread
    }

    pub fn rel_sample_position(&self) -> u64 {
        self.engine_position
            .load(Ordering::Relaxed)
            .saturating_sub(self.start_sample)
    }

    pub fn absolute(&self, rel: u64) -> u64 {
        self.start_sample + rel
    }

    pub fn flushed_samples(&self) -> Arc<AtomicU64> {
        self.writer.flushed_samples()
    }
}
