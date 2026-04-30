use std::collections::VecDeque;
use std::io;

use chrono::{DateTime, Local};

use crate::audio::{Device, EngineHandle, LevelObservation};
use crate::bounce::{BounceJob, BouncePool};
use crate::capture::{Capture, CaptureError};
use crate::project::Project;
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
    pub project: Project,
    pub engine: EngineHandle,
    pub levels_consumer: rtrb::Consumer<LevelObservation>,
    pub bounce_pool: BouncePool,
    pub capture: Option<Capture>,
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
        let project = Project::new(engine.channel_count(), engine.sample_rate(), &settings);
        App {
            settings,
            project,
            engine,
            levels_consumer,
            bounce_pool: BouncePool::start(),
            capture: None,
            display_levels: vec![0.0; n],
            peak_holds: vec![0.0; n],
            level_history: VecDeque::with_capacity(LEVEL_HISTORY_CAPACITY_HINT),
            total_ticks: 0,
            waveform_window_secs: WAVEFORM_WINDOWS_SECS[0],
            started_at: Local::now(),
            tick_peaks: vec![0.0; n],
        }
    }

    pub fn is_recording(&self) -> bool {
        self.capture.is_some()
    }

    pub fn rel_sample_position(&self) -> Option<u64> {
        self.capture.as_ref().map(|c| c.rel_sample_position())
    }

    pub fn relative_to_absolute(&self, rel: u64) -> Option<u64> {
        self.capture.as_ref().map(|c| c.absolute(rel))
    }

    pub fn tick_display(&mut self) {
        self.total_ticks += 1;
        self.apply_bounce_status_updates();
        self.drain_level_observations();
        self.evict_old_level_history();
    }

    fn apply_bounce_status_updates(&mut self) {
        let updates = self.bounce_pool.drain_updates();
        for update in updates {
            self.project
                .timeline
                .set_bounce_status(update.take_id, update.status);
        }
    }

    /// Drains observations into both meter decay state and waveform history.
    fn drain_level_observations(&mut self) {
        let n_channels = self.project.channels.len();
        if self.tick_peaks.len() < n_channels {
            self.tick_peaks.resize(n_channels, 0.0);
        }
        self.tick_peaks[..n_channels].fill(0.0);
        while let Ok(obs) = self.levels_consumer.pop() {
            let mut combined = 0.0f32;
            for (i, &peak) in obs.channel_peaks.iter().take(n_channels).enumerate() {
                self.tick_peaks[i] = self.tick_peaks[i].max(peak);
                if self.project.channels[i].armed {
                    combined = combined.max(peak);
                }
            }
            self.level_history.push_back(LevelSample {
                sample: obs.sample,
                peak: combined,
                recorded: obs.recorded,
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
        // If a recording already exists in this project (we stopped
        // earlier), fork a fresh project so the new capture gets its
        // own dir and timeline. Channels carry over; previous WAVs and
        // bounces stay where they are on disk.
        if self.project.recording.is_some() {
            self.project = self.project.fork_for_new_recording(&self.settings);
        }
        let capture = Capture::start(&self.engine, &mut self.project)?;
        self.capture = Some(capture);
        Ok(())
    }

    pub fn stop_recording(&mut self) {
        if let Some(capture) = self.capture.take() {
            capture.stop(&self.engine, &mut self.project);
        }
    }

    pub fn drop_marker(&mut self) {
        if let Some(rel) = self.rel_sample_position() {
            self.project.timeline.mark(rel);
        }
    }

    pub fn delete_last_marker(&mut self) {
        if !self.is_recording() {
            return;
        }
        self.project.timeline.delete_last_marker();
    }

    pub fn has_unbound_marker(&self) -> bool {
        self.project.timeline.last_marker_unbound()
    }

    /// Promotes the trailing unbound marker into a named take and
    /// dispatches its bounce. Trims the name; silently no-ops for an
    /// empty name or when there's no unbound marker.
    pub fn create_take(&mut self, name: &str) {
        let trimmed = name.trim().to_string();
        if trimmed.is_empty() {
            return;
        }
        let Some(capture) = &self.capture else {
            return;
        };
        if !self.project.timeline.create_take(trimmed) {
            return;
        }
        let take = self.project.timeline.takes().last().cloned();
        let recording = self
            .project
            .recording
            .as_ref()
            .expect("recording exists while capturing");
        let job = take.map(|take| BounceJob {
            take,
            sample_rate: self.project.sample_rate(),
            bounces_dir: self.project.bounces_dir.clone(),
            filename_prefix: self.project.bounces_filename_prefix.clone(),
            channel_files: recording.channel_files.clone(),
            flushed_samples: Some(capture.flushed_samples()),
        });
        if let Some(job) = job {
            self.bounce_pool.dispatch(job);
        }
    }

    pub fn save_template(&mut self, name: &str) -> io::Result<()> {
        let path = template::path_for_name(&self.settings.templates_dir, name);
        let template = Template {
            name: name.to_string(),
            device_name: self.engine.device_name().to_string(),
            channels: self.project.channels.clone(),
        };
        template.save(&path)
    }

    /// Files that fail to deserialize are silently skipped; errors
    /// reading the directory yield an empty list.
    pub fn list_templates(&self) -> Vec<Template> {
        template::list(&self.settings.templates_dir).unwrap_or_default()
    }

    /// Applies `template` to the project. Out-of-range template indices
    /// are silently dropped; channels on the device not covered by the
    /// template keep their fresh defaults.
    pub fn load_template(&mut self, template: &Template) {
        for tmpl_channel in &template.channels {
            if let Some(channel) = self.project.channel_mut(tmpl_channel.index) {
                channel.label = tmpl_channel.label.clone();
                channel.armed = tmpl_channel.armed;
            }
        }
    }

    pub fn toggle_armed(&mut self, channel_index: u16) {
        if self.is_recording() {
            return;
        }
        if let Some(channel) = self.project.channel_mut(channel_index) {
            channel.armed = !channel.armed;
        }
    }

    pub fn set_label(&mut self, channel_index: u16, label: Option<String>) {
        if let Some(channel) = self.project.channel_mut(channel_index) {
            channel.label = label;
        }
    }
}
