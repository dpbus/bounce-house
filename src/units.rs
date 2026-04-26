/// Sample rate in Hz (e.g., 48000).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct SampleRate(pub u32);

/// Index of a channel on the audio interface (0-based).
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ChannelIndex(pub u16);

impl ChannelIndex {
    pub fn as_usize(self) -> usize {
        self.0 as usize
    }
}
