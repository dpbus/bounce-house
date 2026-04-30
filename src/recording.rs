use std::path::PathBuf;

use chrono::{DateTime, Local};

pub struct Recording {
    pub started_at: DateTime<Local>,
    pub stopped_at: Option<DateTime<Local>>,
    pub channel_files: Vec<PathBuf>,
}

impl Recording {
    pub fn elapsed_secs(&self) -> u64 {
        let end = self.stopped_at.unwrap_or_else(Local::now);
        (end - self.started_at).num_seconds().max(0) as u64
    }
}
