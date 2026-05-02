use std::collections::VecDeque;

const MAX_HISTORY_SECS: usize = 1800;
/// Initial allocation only. Runtime growth is bounded by sample-threshold eviction.
const LEVEL_HISTORY_CAPACITY_HINT: usize = MAX_HISTORY_SECS * 100;

const WAVEFORM_WINDOWS_SECS: &[u64] = &[10, 30, 60, 300, 1800];

#[derive(Clone, Copy, Debug)]
pub struct LevelSample {
    /// Absolute audio-input sample at the moment the entry was captured.
    pub sample: u64,
    pub peak: f32,
    pub recorded: bool,
}

pub struct LevelHistory {
    samples: VecDeque<LevelSample>,
    window_secs: u64,
}

impl LevelHistory {
    pub fn new() -> Self {
        LevelHistory {
            samples: VecDeque::with_capacity(LEVEL_HISTORY_CAPACITY_HINT),
            window_secs: WAVEFORM_WINDOWS_SECS[0],
        }
    }

    pub fn push(&mut self, sample: u64, peak: f32, recorded: bool) {
        self.samples.push_back(LevelSample {
            sample,
            peak,
            recorded,
        });
    }

    pub fn evict_old(&mut self, current_sample: u64, sample_rate: u64) {
        let cutoff = current_sample.saturating_sub(MAX_HISTORY_SECS as u64 * sample_rate);
        while self.samples.front().is_some_and(|e| e.sample < cutoff) {
            self.samples.pop_front();
        }
    }

    pub fn cycle_window(&mut self) {
        let idx = WAVEFORM_WINDOWS_SECS
            .iter()
            .position(|&v| v == self.window_secs)
            .unwrap_or(0);
        self.window_secs = WAVEFORM_WINDOWS_SECS[(idx + 1) % WAVEFORM_WINDOWS_SECS.len()];
    }

    pub fn samples(&self) -> &VecDeque<LevelSample> {
        &self.samples
    }

    pub fn window_secs(&self) -> u64 {
        self.window_secs
    }
}
