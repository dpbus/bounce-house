use crate::audio::{AudioInput, LevelObservation};
use crate::channel::Channel;
use crate::level_history::LevelHistory;
use crate::meters::Meters;

pub struct Mixer {
    pub audio_input: AudioInput,
    pub levels_consumer: rtrb::Consumer<LevelObservation>,
    pub channels: Vec<Channel>,
    pub meters: Meters,
    pub level_history: LevelHistory,
}

impl Mixer {
    pub fn start(
        audio_input: AudioInput,
        levels_consumer: rtrb::Consumer<LevelObservation>,
    ) -> Self {
        let n = audio_input.channel_count() as usize;
        let channels = (0..audio_input.channel_count()).map(Channel::new).collect();
        Mixer {
            audio_input,
            levels_consumer,
            channels,
            meters: Meters::new(n),
            level_history: LevelHistory::new(),
        }
    }

    pub fn armed_channels(&self) -> impl Iterator<Item = &Channel> + '_ {
        self.channels.iter().filter(|c| c.armed)
    }

    pub fn toggle_armed(&mut self, channel_index: u16) {
        if let Some(channel) = self.channels.get_mut(channel_index as usize) {
            channel.armed = !channel.armed;
        }
    }

    pub fn set_label(&mut self, channel_index: u16, label: Option<String>) {
        if let Some(channel) = self.channels.get_mut(channel_index as usize) {
            channel.label = label;
        }
    }

    pub fn cycle_waveform_window(&mut self) {
        self.level_history.cycle_window();
    }

    pub fn drain_observations(&mut self, recorded: bool) {
        while let Ok(obs) = self.levels_consumer.pop() {
            self.meters.observe(&obs);
            let combined = combined_armed_peak(&obs.channel_peaks, &self.channels);
            self.level_history.push(obs.sample, combined, recorded);
        }
        self.meters.decay();
    }

    pub fn evict_old_history(&mut self) {
        self.level_history.evict_old(
            self.audio_input.sample_position(),
            self.audio_input.sample_rate().0 as u64,
        );
    }
}

fn combined_armed_peak(peaks: &[f32], channels: &[Channel]) -> f32 {
    channels
        .iter()
        .zip(peaks)
        .filter(|(c, _)| c.armed)
        .map(|(_, &p)| p)
        .fold(0.0_f32, f32::max)
}
