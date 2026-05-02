use std::cell::Cell;
use std::collections::VecDeque;
use std::io;

use chrono::{DateTime, Local};

use crate::audio::{Device, EngineHandle, LevelObservation};
use crate::bounce::{BounceJob, BouncePool};
use crate::capture::{Capture, CaptureError};
use crate::live_channel::LiveChannel;
use crate::session::Session;
use crate::settings::Settings;
use crate::template::{self, Template};

const FAST_DECAY: f32 = 0.976;
const SLOW_DECAY: f32 = 0.990;

pub const WAVEFORM_WINDOWS_SECS: &[u64] = &[10, 30, 60, 300, 1800];

const MAX_HISTORY_SECS: usize = 1800;
/// Initial allocation only. Runtime growth is bounded by sample-threshold eviction.
const LEVEL_HISTORY_CAPACITY_HINT: usize = MAX_HISTORY_SECS * 100;

pub struct App {
    pub settings: Settings,
    pub session: Session,
    pub engine: EngineHandle,
    pub levels_consumer: rtrb::Consumer<LevelObservation>,
    pub bounce_pool: BouncePool,
    pub capture: Option<Capture>,
    pub live_channels: Vec<LiveChannel>,
    pub display_levels: Vec<f32>,
    pub peak_holds: Vec<f32>,
    pub level_history: VecDeque<LevelSample>,
    pub total_ticks: u64,
    pub waveform_window_secs: u64,
    /// App-launch time, for the "Session HH:MM:SS" header timer.
    pub started_at: DateTime<Local>,
    /// Per-channel max peak observed across the level observations
    /// drained this tick — fed into the meter decay. Reused as a
    /// scratch buffer so the 60Hz tick path doesn't allocate.
    tick_peaks: Vec<f32>,
    /// Leftmost visible channel in the strip panel. Bounded by the
    /// strips panel based on its width — see `last_strip_capacity`.
    pub channel_viewport_offset: usize,
    /// How many strips the panel last had room for; written by the
    /// panel during draw, read by the scroll methods so they can
    /// clamp the offset to the same useful range the panel will show.
    pub last_strip_capacity: Cell<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecordingState {
    Idle,
    Recording,
    Paused,
}

#[derive(Clone, Copy, Debug)]
pub struct LevelSample {
    /// Absolute engine sample at the moment the entry was captured.
    pub sample: u64,
    pub peak: f32,
    pub recorded: bool,
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
        let (engine, levels_consumer) = EngineHandle::start(device);
        let n = engine.channel_count() as usize;
        let live_channels = (0..engine.channel_count()).map(LiveChannel::new).collect();
        let session = Session::new(engine.sample_rate(), &settings);
        App {
            settings,
            session,
            engine,
            levels_consumer,
            bounce_pool: BouncePool::start(),
            capture: None,
            live_channels,
            display_levels: vec![0.0; n],
            peak_holds: vec![0.0; n],
            level_history: VecDeque::with_capacity(LEVEL_HISTORY_CAPACITY_HINT),
            total_ticks: 0,
            waveform_window_secs: WAVEFORM_WINDOWS_SECS[0],
            started_at: Local::now(),
            tick_peaks: vec![0.0; n],
            channel_viewport_offset: 0,
            last_strip_capacity: Cell::new(0),
        }
    }

    pub fn armed_channels(&self) -> impl Iterator<Item = &LiveChannel> + '_ {
        self.live_channels.iter().filter(|c| c.armed)
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
        self.capture.is_some()
    }

    pub fn recording_state(&self) -> RecordingState {
        match &self.capture {
            Some(c) if c.is_paused() => RecordingState::Paused,
            Some(_) => RecordingState::Recording,
            None => RecordingState::Idle,
        }
    }

    pub fn toggle_pause(&mut self) {
        let Some(capture) = &mut self.capture else {
            return;
        };
        if capture.is_paused() {
            capture.resume(&self.engine);
        } else {
            capture.pause(&self.engine);
        }
    }

    pub fn rel_sample_position(&self) -> Option<u64> {
        self.capture.as_ref().map(|c| c.rel_sample_position())
    }

    pub fn relative_to_absolute(&self, rel: u64) -> Option<u64> {
        self.capture.as_ref().map(|c| c.absolute(rel))
    }

    pub fn recording_duration_secs(&self) -> Option<u64> {
        let sr = (self.session.sample_rate().0 as u64).max(1);
        let samples = self
            .capture
            .as_ref()
            .map(|c| c.rel_sample_position())
            .or(self.session.end_sample())?;
        Some(samples / sr)
    }

    pub fn tick_display(&mut self) {
        self.total_ticks += 1;
        self.apply_bounce_events();
        self.drain_level_observations();
        self.evict_old_level_history();
    }

    fn apply_bounce_events(&mut self) {
        let updates = self.bounce_pool.drain_updates();
        for update in updates {
            self.session
                .apply_bounce_event(update.take_id, update.event);
        }
    }

    /// Drains observations into both meter decay state and waveform history.
    fn drain_level_observations(&mut self) {
        let n_channels = self.live_channels.len();
        if self.tick_peaks.len() < n_channels {
            self.tick_peaks.resize(n_channels, 0.0);
        }
        self.tick_peaks[..n_channels].fill(0.0);
        let recorded = self.capture.as_ref().is_some_and(|c| !c.is_paused());
        while let Ok(obs) = self.levels_consumer.pop() {
            let mut combined = 0.0f32;
            for (i, &peak) in obs.channel_peaks.iter().take(n_channels).enumerate() {
                self.tick_peaks[i] = self.tick_peaks[i].max(peak);
                if self.live_channels[i].armed {
                    combined = combined.max(peak);
                }
            }
            self.level_history.push_back(LevelSample {
                sample: obs.sample,
                peak: combined,
                recorded,
            });
        }
        for i in 0..n_channels {
            let peak = self.tick_peaks[i];
            self.display_levels[i] = peak.max(self.display_levels[i] * FAST_DECAY);
            self.peak_holds[i] = peak.max(self.peak_holds[i] * SLOW_DECAY);
        }
    }

    fn evict_old_level_history(&mut self) {
        let sample_rate = self.engine.sample_rate().0 as u64;
        let cutoff = self
            .engine
            .sample_position()
            .saturating_sub(MAX_HISTORY_SECS as u64 * sample_rate);
        while self
            .level_history
            .front()
            .is_some_and(|e| e.sample < cutoff)
        {
            self.level_history.pop_front();
        }
    }

    pub fn cycle_waveform_window(&mut self) {
        let idx = WAVEFORM_WINDOWS_SECS
            .iter()
            .position(|&v| v == self.waveform_window_secs)
            .unwrap_or(0);
        self.waveform_window_secs = WAVEFORM_WINDOWS_SECS[(idx + 1) % WAVEFORM_WINDOWS_SECS.len()];
    }

    pub fn start_recording(&mut self) -> Result<(), AppError> {
        if self.capture.is_some() {
            return Err(AppError::NotIdle);
        }
        if self.session.has_recording() {
            self.session = self.session.fork_for_new_recording(&self.settings);
        }
        let armed_channels: Vec<LiveChannel> = self.armed_channels().cloned().collect();
        let capture = Capture::start(&self.engine, &mut self.session, &armed_channels)?;
        self.capture = Some(capture);
        Ok(())
    }

    pub fn stop_recording(&mut self) {
        if let Some(capture) = self.capture.take() {
            capture.stop(&self.engine, &mut self.session);
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
            flushed_samples: self.capture.as_ref().map(|c| c.flushed_samples()),
        };
        self.bounce_pool.dispatch(job);
    }

    pub fn save_template(&mut self, name: &str) -> io::Result<()> {
        let path = template::path_for_name(&self.settings.templates_dir, name);
        let template = Template {
            name: name.to_string(),
            device_name: self.engine.device_name().to_string(),
            channels: self.live_channels.clone(),
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
            if let Some(channel) = self.live_channels.get_mut(tmpl_channel.index as usize) {
                channel.label = tmpl_channel.label.clone();
                channel.armed = tmpl_channel.armed;
            }
        }
    }

    pub fn toggle_armed(&mut self, channel_index: u16) {
        if self.is_recording() {
            return;
        }
        if let Some(channel) = self.live_channels.get_mut(channel_index as usize) {
            channel.armed = !channel.armed;
        }
    }

    pub fn set_label(&mut self, channel_index: u16, label: Option<String>) {
        if let Some(channel) = self.live_channels.get_mut(channel_index as usize) {
            channel.label = label;
        }
    }
}
