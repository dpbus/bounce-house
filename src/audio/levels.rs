pub const MAX_CHANNELS: usize = 128;

#[derive(Clone, Copy)]
pub struct LevelObservation {
    pub sample: u64,
    pub recorded: bool,
    pub channel_peaks: [f32; MAX_CHANNELS],
}
