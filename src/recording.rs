use std::path::PathBuf;

use chrono::{DateTime, Local};

use crate::timeline::Timeline;

pub struct Recording {
    pub started_at: DateTime<Local>,
    pub output_dir: PathBuf,
    pub timeline: Timeline,
}

impl Recording {
    pub fn new(started_at: DateTime<Local>, output_dir: PathBuf) -> Self {
        Self {
            started_at,
            output_dir,
            timeline: Timeline::new(),
        }
    }
}
