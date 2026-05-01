use std::path::PathBuf;

pub struct Recording {
    pub channels: Vec<RecordedChannel>,
    pub end_sample: Option<u64>,
}

/// One channel's slot in a Recording: a snapshot at record-start time,
/// plus the WAV file it was captured to (path relative to the session
/// dir). Index and label are frozen — later edits to Session.channels
/// don't reach back here.
#[allow(dead_code)] // index + label become read sites when persistence lands
#[derive(Clone)]
pub struct RecordedChannel {
    pub index: u16,
    pub label: Option<String>,
    pub file: PathBuf,
}
