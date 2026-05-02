use std::path::PathBuf;

/// One channel's slot in a recording: a snapshot at record-start time,
/// plus the WAV file it was captured to (path relative to the session
/// dir). Index and label are frozen — later edits to live channels
/// don't reach back here.
#[derive(Clone)]
pub struct RecordedChannel {
    pub index: u16,
    pub label: Option<String>,
    pub file: PathBuf,
}
