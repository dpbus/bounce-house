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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn fake(started_at: DateTime<Local>, stopped_at: Option<DateTime<Local>>) -> Recording {
        Recording {
            started_at,
            stopped_at,
            channel_files: vec![],
        }
    }

    #[test]
    fn elapsed_secs_is_difference_when_stopped() {
        let start = Local::now();
        let stop = start + Duration::seconds(42);
        let rec = fake(start, Some(stop));
        assert_eq!(rec.elapsed_secs(), 42);
    }

    #[test]
    fn elapsed_secs_uses_now_when_still_recording() {
        // Started 2 seconds ago, still recording — should be at least 2.
        let start = Local::now() - Duration::seconds(2);
        let rec = fake(start, None);
        let secs = rec.elapsed_secs();
        assert!((2..5).contains(&secs), "expected ~2, got {secs}");
    }

    #[test]
    fn elapsed_secs_clamps_negative_durations_to_zero() {
        // Defensive: if stopped_at is somehow before started_at, return 0
        // rather than wrapping to a huge number.
        let start = Local::now();
        let stop = start - Duration::seconds(5);
        let rec = fake(start, Some(stop));
        assert_eq!(rec.elapsed_secs(), 0);
    }
}
