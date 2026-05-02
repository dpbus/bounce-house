use serde::{Deserialize, Serialize};

/// Sample rate in Hz (e.g., 48000).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SampleRate(pub u32);
