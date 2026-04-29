use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

use chrono::{DateTime, Local};

use crate::audio::{ArmedChannel, DiskWriter};
use crate::units::SampleRate;

/// A single capture of audio to disk for a project. Holds the writer
/// while active and lifecycle metadata after stop. The project owns
/// the timeline, output paths, and sample rate — Recording is just
/// "this capture event."
pub struct Recording {
    pub started_at: DateTime<Local>,
    pub stopped_at: Option<DateTime<Local>>,
    /// Absolute engine sample at the moment recording started. All
    /// marker and take samples on the project's timeline are stored
    /// relative to this.
    pub start_sample: u64,
    pub channel_files: Vec<PathBuf>,
    writer: Option<DiskWriter>,
}

impl Recording {
    pub fn start(
        output_dir: PathBuf,
        consumer: rtrb::Consumer<f32>,
        sample_rate: SampleRate,
        total_channel_count: u16,
        armed: Vec<ArmedChannel>,
        start_sample: u64,
    ) -> Self {
        let writer = DiskWriter::start(
            consumer,
            output_dir,
            sample_rate,
            total_channel_count,
            armed,
        );
        let channel_files = writer.channel_files().to_vec();
        Self {
            started_at: Local::now(),
            stopped_at: None,
            start_sample,
            channel_files,
            writer: Some(writer),
        }
    }

    /// Drop the writer (joins its thread, finalizes WAVs) and freeze
    /// the elapsed timer. Idempotent.
    pub fn stop(&mut self) {
        if self.writer.is_none() {
            return;
        }
        self.stopped_at = Some(Local::now());
        self.writer = None;
    }

    pub fn is_writing(&self) -> bool {
        self.writer.is_some()
    }

    pub fn flushed_samples(&self) -> Option<Arc<AtomicU64>> {
        self.writer.as_ref().map(|w| w.flushed_samples())
    }

    /// Wall-clock seconds since `started_at`; frozen at `stopped_at`
    /// once stopped.
    pub fn elapsed_secs(&self) -> u64 {
        let end = self.stopped_at.unwrap_or_else(Local::now);
        (end - self.started_at).num_seconds().max(0) as u64
    }
}
