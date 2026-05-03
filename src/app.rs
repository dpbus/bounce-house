use std::io;

use chrono::{DateTime, Local};

use crate::audio::{AudioInput, Device};
use crate::bounce::{BounceJob, BouncePool};
use crate::capture::{Capture, CaptureError};
use crate::channel::Channel;
use crate::mixer::{Mixer, RuntimeMode};
use crate::session::Session;
use crate::settings::Settings;
use crate::template::{self, Template};
use crate::ui::ChannelStrips;

pub struct App {
    pub settings: Settings,
    pub session: Session,
    pub mixer: Mixer,
    pub bounce_pool: BouncePool,
    pub total_ticks: u64,
    /// App-launch time, for the "Session HH:MM:SS" header timer.
    pub started_at: DateTime<Local>,
    pub channel_strips: ChannelStrips,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecordingState {
    Idle,
    Recording,
    Paused,
}

#[derive(Debug)]
pub enum AppError {
    NothingArmed,
    NotIdle,
}

impl From<CaptureError> for AppError {
    fn from(err: CaptureError) -> Self {
        match err {
            CaptureError::NothingArmed => AppError::NothingArmed,
        }
    }
}

impl App {
    pub fn new(device: Device, settings: Settings) -> Self {
        let (audio_input, levels_consumer) = AudioInput::start(device);
        let session = Session::new(audio_input.sample_rate(), &settings);
        App {
            settings,
            session,
            mixer: Mixer::start(audio_input, levels_consumer),
            bounce_pool: BouncePool::start(),
            total_ticks: 0,
            started_at: Local::now(),
            channel_strips: ChannelStrips::new(),
        }
    }

    pub fn scroll_strips_left(&mut self, n: usize) {
        self.channel_strips.scroll_left(n);
    }

    pub fn scroll_strips_right(&mut self, n: usize) {
        let visible = self.mixer.armed_channels().count();
        self.channel_strips.scroll_right(n, visible);
    }

    pub fn recording_duration_secs(&self) -> Option<u64> {
        let sr = (self.session.sample_rate().0 as u64).max(1);
        let samples = self
            .mixer
            .rel_sample_position()
            .or(self.session.end_sample())?;
        Some(samples / sr)
    }

    pub fn tick_display(&mut self) {
        self.total_ticks += 1;
        self.apply_bounce_events();
        self.mixer.drain_observations();
        self.mixer.evict_old_history();
    }

    fn apply_bounce_events(&mut self) {
        let updates = self.bounce_pool.drain_updates();
        for update in updates {
            self.session
                .apply_bounce_event(update.take_id, update.event);
        }
    }

    pub fn start_recording(&mut self) -> Result<(), AppError> {
        if !self.mixer.is_idle() {
            return Err(AppError::NotIdle);
        }
        if self.session.has_recording() {
            self.session = self.session.fork_for_new_recording(&self.settings);
        }
        let armed: Vec<Channel> = self.mixer.armed_channels().cloned().collect();
        let capture = Capture::start(&self.mixer.audio_input, &mut self.session, &armed)?;
        self.mixer.runtime_mode = RuntimeMode::Recording(capture);
        Ok(())
    }

    pub fn stop_recording(&mut self) {
        if let Some(capture) = self.mixer.take_capture() {
            capture.stop(&mut self.session);
        }
    }

    pub fn drop_marker(&mut self) {
        if let Some(rel) = self.mixer.rel_sample_position() {
            self.session.drop_marker(rel);
        }
    }

    pub fn delete_last_marker(&mut self) {
        if !self.mixer.is_recording() {
            return;
        }
        self.session.delete_last_marker();
    }

    pub fn has_unbound_marker(&self) -> bool {
        self.session.last_marker_unbound()
    }

    /// Promotes the trailing unbound marker into a named take and
    /// dispatches its bounce. Trims the name; silently no-ops for an
    /// empty name or when there's no unbound marker.
    pub fn create_take(&mut self, name: &str) {
        let trimmed = name.trim().to_string();
        if trimmed.is_empty() {
            return;
        }
        let Some(take) = self.session.create_take(trimmed) else {
            return;
        };
        let job = BounceJob {
            take,
            sample_rate: self.session.sample_rate(),
            bounces_dir: self.session.bounces_dir.clone(),
            filename_prefix: self.session.bounces_filename_prefix.clone(),
            track_files: self.session.recording_track_paths(),
            flushed_samples: self
                .mixer
                .runtime_mode
                .capture()
                .map(|c| c.flushed_samples()),
        };
        self.bounce_pool.dispatch(job);
    }

    pub fn save_template(&mut self, name: &str) -> io::Result<()> {
        let path = template::path_for_name(&self.settings.templates_dir, name);
        let template = Template {
            name: name.to_string(),
            device_name: self.mixer.audio_input.device_name().to_string(),
            channels: self.mixer.channels.clone(),
        };
        template.save(&path)
    }

    pub fn list_templates(&self) -> Vec<Template> {
        template::list(&self.settings.templates_dir).unwrap_or_default()
    }

    /// Applies `template` to the live mix. Out-of-range template
    /// indices are silently dropped; channels on the device not covered
    /// by the template keep their fresh defaults.
    pub fn load_template(&mut self, template: &Template) {
        for tmpl_channel in &template.channels {
            if let Some(channel) = self.mixer.channels.get_mut(tmpl_channel.index as usize) {
                channel.label = tmpl_channel.label.clone();
                channel.armed = tmpl_channel.armed;
            }
        }
    }

    pub fn toggle_armed(&mut self, channel_index: u16) {
        if self.mixer.is_recording() {
            return;
        }
        self.mixer.toggle_armed(channel_index);
    }

    pub fn set_label(&mut self, channel_index: u16, label: Option<String>) {
        self.mixer.set_label(channel_index, label);
    }

    pub fn toggle_pause(&mut self) {
        self.mixer.toggle_pause();
    }

    pub fn cycle_waveform_window(&mut self) {
        self.mixer.cycle_waveform_window();
    }
}
