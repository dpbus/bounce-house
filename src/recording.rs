use std::path::PathBuf;

use chrono::{DateTime, Local};

use crate::timeline::Timeline;

pub struct Recording {
    pub started_at: DateTime<Local>,
    pub stopped_at: Option<DateTime<Local>>,
    pub output_dir: PathBuf,
    pub timeline: Timeline,
}

impl Recording {
    pub fn new(started_at: DateTime<Local>, output_dir: PathBuf) -> Self {
        Self {
            started_at,
            stopped_at: None,
            output_dir,
            timeline: Timeline::new(),
        }
    }

    /// Seconds since `started_at` — frozen at `stopped_at` once stopped.
    pub fn elapsed_secs(&self) -> u64 {
        let end = self.stopped_at.unwrap_or_else(Local::now);
        (end - self.started_at).num_seconds().max(0) as u64
    }
}
