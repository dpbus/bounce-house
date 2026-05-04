use crate::audio::{DeviceInfo, InputDevice, LevelObservation, OutputDevice};
use crate::channel::Channel;
use crate::level_history::LevelHistory;
use crate::meters::Meters;
use crate::template::Template;

pub struct Mixer {
    pub input_device: InputDevice,
    /// `None` for input-only devices (debug fakes).
    #[allow(dead_code)]
    pub output_device: Option<OutputDevice>,
    pub levels_consumer: rtrb::Consumer<LevelObservation>,
    pub channels: Vec<Channel>,
    pub meters: Meters,
    pub level_history: LevelHistory,
}

impl Mixer {
    pub fn start(info: &DeviceInfo) -> Self {
        let (input_device, levels_consumer) = InputDevice::start(info);
        let output_device = OutputDevice::start(info);
        let n = input_device.channel_count() as usize;
        let channels = (0..input_device.channel_count())
            .map(Channel::new)
            .collect();
        Mixer {
            input_device,
            output_device,
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
            self.input_device.sample_position(),
            self.input_device.sample_rate().0 as u64,
        );
    }

    pub fn snapshot_template(&self, name: &str) -> Template {
        Template {
            name: name.to_string(),
            device_name: self.input_device.name().to_string(),
            channels: self.channels.clone(),
        }
    }

    pub fn apply_template(&mut self, template: &Template) {
        for tmpl_channel in &template.channels {
            if let Some(channel) = self.channels.get_mut(tmpl_channel.index as usize) {
                channel.label = tmpl_channel.label.clone();
                channel.armed = tmpl_channel.armed;
            }
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
