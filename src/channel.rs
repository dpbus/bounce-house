use std::path::PathBuf;

#[derive(Clone)]
pub struct Channel {
    pub index: u16,
    pub label: Option<String>,
    pub file: PathBuf,
}
