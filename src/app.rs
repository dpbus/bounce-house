use std::cell::Cell;
use std::io;

use chrono::{DateTime, Local};

use crate::audio::{AudioInput, Device, LevelObservation};
use crate::bounce::{BounceJob, BouncePool};
use crate::capture::{Capture, CaptureError};
use crate::channel::Channel;
use crate::level_history::LevelHistory;
use crate::meters::Meters;
use crate::session::Session;
use crate::settings::Settings;
use crate::template::{self, Template};

pub struct App {
    pub settings: Settings,
    pub session: Session,
    pub audio_input: AudioInput,
    pub levels_consumer: rtrb::Consumer<LevelObservation>,
    pub bounce_pool: BouncePool,
    pub runtime_mode: RuntimeMode,
    pub channels: Vec<Channel>,
    pub meters: Meters,
    pub level_history: LevelHistory,
    pub total_ticks: u64,
    /// App-launch time, for the "Session HH:MM:SS" header timer.
    pub started_at: DateTime<Local>,
    /// Leftmost visible channel in the strip panel. Bounded by the
    /// strips panel based on its width — see `last_strip_capacity`.
    pub channel_viewport_offset: usize,
    /// How many strips the panel last had room for; written by the
    /// panel during draw, read by the scroll methods so they can
    /// clamp the offset to the same useful range the panel will show.
    pub last_strip_capacity: Cell<usize>,
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
        let n = audio_input.channel_count() as usize;
        let channels = (0..audio_input.channel_count()).map(Channel::new).collect();
        let session = Session::new(audio_input.sample_rate(), &settings);
        App {
            settings,
            session,
            audio_input,
            levels_consumer,
            bounce_pool: BouncePool::start(),
            runtime_mode: RuntimeMode::Idle,
            channels,
            meters: Meters::new(n),
            level_history: LevelHistory::new(),
            total_ticks: 0,
            started_at: Local::now(),
            channel_viewport_offset: 0,
            last_strip_capacity: Cell::new(0),
        }
    }

    pub fn armed_channels(&self) -> impl Iterator<Item = &Channel> + '_ {
        self.channels.iter().filter(|c| c.armed)
    }

    pub fn scroll_strips_left(&mut self, n: usize) {
        self.channel_viewport_offset = self.channel_viewport_offset.saturating_sub(n);
    }

    pub fn scroll_strips_right(&mut self, n: usize) {
        let visible = self.armed_channels().count();
        let cap = self.last_strip_capacity.get().max(1);
        let max = visible.saturating_sub(cap);
        self.channel_viewport_offset = (self.channel_viewport_offset + n).min(max);
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

    pub fn toggle_pause(&mut self) {
        let Some(capture) = self.runtime_mode.capture_mut() else {
            return;
        };
        if capture.is_paused() {
            capture.resume(&self.audio_input);
        } else {
            capture.pause(&self.audio_input);
        }
    }

    pub fn rel_sample_position(&self) -> Option<u64> {
        self.runtime_mode.capture().map(|c| c.rel_sample_position())
    }

    pub fn relative_to_absolute(&self, rel: u64) -> Option<u64> {
        self.runtime_mode.capture().map(|c| c.absolute(rel))
    }

    pub fn recording_duration_secs(&self) -> Option<u64> {
        let sr = (self.session.sample_rate().0 as u64).max(1);
        let samples = self
            .runtime_mode
            .capture()
            .map(|c| c.rel_sample_position())
            .or(self.session.end_sample())?;
        Some(samples / sr)
    }

    pub fn tick_display(&mut self) {
        self.total_ticks += 1;
        self.apply_bounce_events();
        self.drain_level_observations();
        self.level_history.evict_old(
            self.audio_input.sample_position(),
            self.audio_input.sample_rate().0 as u64,
        );
    }

    fn apply_bounce_events(&mut self) {
        let updates = self.bounce_pool.drain_updates();
        for update in updates {
            self.session
                .apply_bounce_event(update.take_id, update.event);
        }
    }

    fn drain_level_observations(&mut self) {
        let recorded = self.runtime_mode.capture().is_some_and(|c| !c.is_paused());
        while let Ok(obs) = self.levels_consumer.pop() {
            self.meters.observe(&obs);
            let combined = combined_armed_peak(&obs.channel_peaks, &self.channels);
            self.level_history.push(obs.sample, combined, recorded);
        }
        self.meters.decay();
    }

    pub fn cycle_waveform_window(&mut self) {
        self.level_history.cycle_window();
    }

    pub fn start_recording(&mut self) -> Result<(), AppError> {
        if !self.runtime_mode.is_idle() {
            return Err(AppError::NotIdle);
        }
        if self.session.has_recording() {
            self.session = self.session.fork_for_new_recording(&self.settings);
        }
        let armed_channels: Vec<Channel> = self.armed_channels().cloned().collect();
        let capture = Capture::start(&self.audio_input, &mut self.session, &armed_channels)?;
        self.runtime_mode = RuntimeMode::Recording(capture);
        Ok(())
    }

    pub fn stop_recording(&mut self) {
        if let RuntimeMode::Recording(capture) =
            std::mem::replace(&mut self.runtime_mode, RuntimeMode::Idle)
        {
            capture.stop(&self.audio_input, &mut self.session);
        }
    }

    pub fn drop_marker(&mut self) {
        if let Some(rel) = self.rel_sample_position() {
            self.session.drop_marker(rel);
        }
    }

    pub fn delete_last_marker(&mut self) {
        if !self.is_recording() {
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
            flushed_samples: self.runtime_mode.capture().map(|c| c.flushed_samples()),
        };
        self.bounce_pool.dispatch(job);
    }

    pub fn save_template(&mut self, name: &str) -> io::Result<()> {
        let path = template::path_for_name(&self.settings.templates_dir, name);
        let template = Template {
            name: name.to_string(),
            device_name: self.audio_input.device_name().to_string(),
            channels: self.channels.clone(),
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
            if let Some(channel) = self.channels.get_mut(tmpl_channel.index as usize) {
                channel.label = tmpl_channel.label.clone();
                channel.armed = tmpl_channel.armed;
            }
        }
    }

    pub fn toggle_armed(&mut self, channel_index: u16) {
        if self.is_recording() {
            return;
        }
        if let Some(channel) = self.channels.get_mut(channel_index as usize) {
            channel.armed = !channel.armed;
        }
    }

    pub fn set_label(&mut self, channel_index: u16, label: Option<String>) {
        if let Some(channel) = self.channels.get_mut(channel_index as usize) {
            channel.label = label;
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
