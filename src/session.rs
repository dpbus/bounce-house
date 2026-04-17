use crate::input::Input;
use crate::take::Take;
use chrono::{DateTime, Local};
use std::path::PathBuf;

pub struct Session {
    pub sample_rate: u32,
    pub raw_dir: PathBuf,
    pub bounce_dir: PathBuf,
    pub inputs: Vec<Input>,
    pub takes: Vec<Take>,
    pub cursor: u64,
    pub started_at: DateTime<Local>,
}

impl Session {
    pub fn new(sample_rate: u32, raw_dir: PathBuf, bounce_dir: PathBuf, num_inputs: u8) -> Session {
        let started_at = Local::now();
        let inputs = (0..num_inputs).map(Input::new).collect();

        Session {
            sample_rate,
            raw_dir,
            bounce_dir,
            inputs,
            takes: Vec::new(),
            cursor: 0,
            started_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_session_defaults() {
        let session = Session::new(
            48000,
            PathBuf::from("/tmp/raw"),
            PathBuf::from("/tmp/bounces"),
            4,
        );
        assert_eq!(session.sample_rate, 48000);
        assert_eq!(session.inputs.len(), 4);
        assert_eq!(session.takes.len(), 0);
        assert_eq!(session.cursor, 0);
    }
}
