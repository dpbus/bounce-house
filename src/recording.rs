use std::path::PathBuf;

use chrono::{DateTime, Local};

use crate::audio::{ArmedChannel, DiskWriter};
use crate::timeline::Timeline;
use crate::units::SampleRate;

pub struct Recording {
    pub started_at: DateTime<Local>,
    pub stopped_at: Option<DateTime<Local>>,
    pub output_dir: PathBuf,
    /// Absolute engine sample at the moment recording started. All marker
    /// and take samples are stored relative to this.
    pub start_sample: u64,
    pub channel_files: Vec<PathBuf>,
    pub timeline: Timeline,
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
        start_tick: u64,
    ) -> Self {
        let writer = DiskWriter::start(
            consumer,
            output_dir.clone(),
            sample_rate,
            total_channel_count,
            armed,
        );
        let channel_files = writer.channel_files().to_vec();

        let mut recording = Self {
            started_at: Local::now(),
            stopped_at: None,
            output_dir,
            start_sample,
            channel_files,
            timeline: Timeline::new(),
            writer: Some(writer),
        };
        recording.mark(start_tick, start_sample);
        recording
    }

    /// Drop the writer (joins its thread, finalizes WAVs) and freeze the
    /// elapsed timer. Idempotent.
    pub fn stop(&mut self, stop_tick: u64, abs_sample: u64) {
        if self.writer.is_none() {
            return;
        }
        self.mark(stop_tick, abs_sample);
        self.stopped_at = Some(Local::now());
        self.writer = None;
    }

    pub fn is_writing(&self) -> bool {
        self.writer.is_some()
    }

    /// Push a marker at the current absolute engine sample, converted to
    /// recording-relative.
    pub fn mark(&mut self, tick: u64, abs_sample: u64) {
        let rel = abs_sample.saturating_sub(self.start_sample);
        self.timeline.mark(tick, rel);
    }

    /// Seconds since `started_at` — frozen at `stopped_at` once stopped.
    pub fn elapsed_secs(&self) -> u64 {
        let end = self.stopped_at.unwrap_or_else(Local::now);
        (end - self.started_at).num_seconds().max(0) as u64
    }
}
