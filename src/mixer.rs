use crate::app::RecordingState;
use crate::audio::{AudioInput, LevelObservation};
use crate::capture::Capture;
use crate::channel::Channel;
use crate::level_history::LevelHistory;
use crate::meters::Meters;

pub struct Mixer {
    pub audio_input: AudioInput,
    pub levels_consumer: rtrb::Consumer<LevelObservation>,
    pub runtime_mode: RuntimeMode,
    pub channels: Vec<Channel>,
    pub meters: Meters,
    pub level_history: LevelHistory,
}

pub enum RuntimeMode {
    Idle,
    Recording(Capture),
}

impl RuntimeMode {
    pub fn is_idle(&self) -> bool {
        matches!(self, RuntimeMode::Idle)
    }

    pub fn is_recording(&self) -> bool {
        matches!(self, RuntimeMode::Recording(_))
    }

    pub fn capture(&self) -> Option<&Capture> {
        match self {
            RuntimeMode::Recording(c) => Some(c),
            RuntimeMode::Idle => None,
        }
    }

    pub fn capture_mut(&mut self) -> Option<&mut Capture> {
        match self {
            RuntimeMode::Recording(c) => Some(c),
            RuntimeMode::Idle => None,
        }
    }
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
            runtime_mode: RuntimeMode::Idle,
            channels,
            meters: Meters::new(n),
            level_history: LevelHistory::new(),
        }
    }

    pub fn armed_channels(&self) -> impl Iterator<Item = &Channel> + '_ {
        self.channels.iter().filter(|c| c.armed)
    }

    pub fn is_idle(&self) -> bool {
        self.runtime_mode.is_idle()
    }

    pub fn is_recording(&self) -> bool {
        self.runtime_mode.is_recording()
    }

    pub fn recording_state(&self) -> RecordingState {
        match self.runtime_mode.capture() {
            Some(c) if c.is_paused() => RecordingState::Paused,
            Some(_) => RecordingState::Recording,
            None => RecordingState::Idle,
        }
    }

    pub fn rel_sample_position(&self) -> Option<u64> {
        self.runtime_mode.capture().map(|c| c.rel_sample_position())
    }

    pub fn relative_to_absolute(&self, rel: u64) -> Option<u64> {
        self.runtime_mode.capture().map(|c| c.absolute(rel))
    }

    pub fn toggle_pause(&mut self) {
        let Some(capture) = self.runtime_mode.capture_mut() else {
            return;
        };
        if capture.is_paused() {
            capture.resume();
        } else {
            capture.pause();
        }
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

    pub fn drain_observations(&mut self) {
        let recorded = self.runtime_mode.capture().is_some_and(|c| !c.is_paused());
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

    pub fn take_capture(&mut self) -> Option<Capture> {
        match std::mem::replace(&mut self.runtime_mode, RuntimeMode::Idle) {
            RuntimeMode::Recording(c) => Some(c),
            RuntimeMode::Idle => None,
        }
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
