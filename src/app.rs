use std::collections::VecDeque;
use std::io;

use chrono::{DateTime, Local};

use crate::audio::{ArmedChannel, Device, EngineHandle, LevelObservation};
use crate::bounce::{BounceJob, BouncePool};
use crate::project::Project;
use crate::recording::Recording;
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
        self.project
            .recording
            .as_ref()
            .is_some_and(|r| r.is_writing())
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
        if self.is_recording() {
            return Err(AppError::NotIdle);
        }
        // If a recording already exists in this project (we stopped
        // earlier), fork a fresh project so the new capture gets its
        // own dir and timeline. Channels carry over; previous WAVs and
        // bounces stay where they are on disk.
        if self.project.recording.is_some() {
            self.project = self.project.fork_for_new_recording(&self.settings);
        }
        // Defensive: drop armed channels whose index is outside the engine's
        // real channel count. Production sessions never produce out-of-range
        // indices; the filter exists to keep DEBUG_CHANNELS-padded channels
        // from reaching the disk writer (which sizes its frame to the engine).
        let max_index = self.engine.channel_count();
        let armed: Vec<ArmedChannel> = self
            .project
            .armed_channels()
            .filter(|c| c.index < max_index)
            .map(|c| ArmedChannel {
                index: c.index,
                label: c.label.clone(),
            })
            .collect();
        if armed.is_empty() {
            return Err(AppError::NothingArmed);
        }

        let consumer = self.engine.start_recording();
        let start_sample = self.engine.sample_position();
        let recording = Recording::start(
            self.project.dir.clone(),
            consumer,
            self.project.sample_rate,
            self.engine.channel_count(),
            armed,
            start_sample,
        );
        self.project.recording = Some(recording);
        // Auto-mark recording start (rel sample 0).
        self.project.timeline.mark(0);
        Ok(())
    }

    pub fn stop_recording(&mut self) {
        if !self.is_recording() {
            return;
        }
        // Detach producer first: engine.stop_recording is synchronous, so no
        // further samples land in the rtrb after it returns.
        self.engine.stop_recording();
        let abs_sample = self.engine.sample_position();
        if let Some(r) = &mut self.project.recording {
            r.stop();
            let rel = abs_sample.saturating_sub(r.start_sample);
            self.project.timeline.mark(rel);
        }
    }

    pub fn drop_marker(&mut self) {
        if !self.is_recording() {
            return;
        }
        let abs_sample = self.engine.sample_position();
        if let Some(r) = &self.project.recording {
            let rel = abs_sample.saturating_sub(r.start_sample);
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
        if self.project.recording.is_none() {
            return;
        }
        if !self.project.timeline.create_take(trimmed) {
            return;
        }
        let take = self.project.timeline.takes().last().cloned();
        let r = self.project.recording.as_ref().unwrap();
        let job = take.map(|take| BounceJob {
            take,
            sample_rate: self.project.sample_rate,
            bounces_dir: self.project.bounces_dir.clone(),
            filename_prefix: self.project.bounces_filename_prefix.clone(),
            channel_files: r.channel_files.clone(),
            flushed_samples: r.flushed_samples(),
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
