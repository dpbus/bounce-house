use crate::audio::LevelObservation;

const FAST_DECAY: f32 = 0.976;
const SLOW_DECAY: f32 = 0.990;

pub struct Meters {
    display_levels: Vec<f32>,
    peak_holds: Vec<f32>,
    tick_peaks: Vec<f32>,
}

impl Meters {
    pub fn new(channel_count: usize) -> Self {
        Meters {
            display_levels: vec![0.0; channel_count],
            peak_holds: vec![0.0; channel_count],
            tick_peaks: vec![0.0; channel_count],
        }
    }

    pub fn observe(&mut self, obs: &LevelObservation) {
        let n = self.display_levels.len();
        if self.tick_peaks.len() < n {
            self.tick_peaks.resize(n, 0.0);
        }
        for (i, &peak) in obs.channel_peaks.iter().take(n).enumerate() {
            self.tick_peaks[i] = self.tick_peaks[i].max(peak);
        }
    }

    pub fn decay(&mut self) {
        for i in 0..self.display_levels.len() {
            let peak = self.tick_peaks[i];
            self.display_levels[i] = peak.max(self.display_levels[i] * FAST_DECAY);
            self.peak_holds[i] = peak.max(self.peak_holds[i] * SLOW_DECAY);
            self.tick_peaks[i] = 0.0;
        }
    }

    pub fn display_levels(&self) -> &[f32] {
        &self.display_levels
    }

    pub fn peak_holds(&self) -> &[f32] {
        &self.peak_holds
    }
}
