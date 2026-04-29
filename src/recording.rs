use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

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
    pub sample_rate: SampleRate,
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
            sample_rate,
            channel_files,
            timeline: Timeline::new(),
            writer: Some(writer),
        };
        recording.mark(start_sample);
        recording
    }

    /// Drop the writer (joins its thread, finalizes WAVs) and freeze the
    /// elapsed timer. Idempotent.
    pub fn stop(&mut self, abs_sample: u64) {
        if self.writer.is_none() {
            return;
        }
        self.mark(abs_sample);
        self.stopped_at = Some(Local::now());
        self.writer = None;
    }

    pub fn is_writing(&self) -> bool {
        self.writer.is_some()
    }

    pub fn flushed_samples(&self) -> Option<Arc<AtomicU64>> {
        self.writer.as_ref().map(|w| w.flushed_samples())
    }

    /// Push a marker at the given absolute engine sample, stored
    /// recording-relative.
    pub fn mark(&mut self, abs_sample: u64) {
        let rel = abs_sample.saturating_sub(self.start_sample);
        self.timeline.mark(rel);
    }

    /// Seconds since `started_at`; frozen at `stopped_at` once stopped.
    pub fn elapsed_secs(&self) -> u64 {
        let end = self.stopped_at.unwrap_or_else(Local::now);
        (end - self.started_at).num_seconds().max(0) as u64
    }

    /// Convert a recording-relative sample to seconds.
    pub fn secs_at(&self, sample: u64) -> u64 {
        sample / (self.sample_rate.0 as u64).max(1)
    }

    pub fn duration_secs(&self, start_sample: u64, end_sample: u64) -> u64 {
        self.secs_at(end_sample.saturating_sub(start_sample))
    }

    /// Seconds since the trailing marker, given the current absolute
    /// engine sample. Only meaningful during active recording — after
    /// stop the engine sample position keeps advancing but markers
    /// don't.
    pub fn since_last_marker_secs(&self, current_abs_sample: u64) -> u64 {
        let last = self
            .timeline
            .markers()
            .last()
            .map(|m| m.sample)
            .unwrap_or(0);
        let rel = current_abs_sample.saturating_sub(self.start_sample);
        self.secs_at(rel.saturating_sub(last))
    }
}
