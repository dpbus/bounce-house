use std::collections::VecDeque;

use chrono::{DateTime, Duration, Local};

use crate::audio::{ArmedChannel, Device, EngineHandle, LevelObservation};
use crate::bounce::{BounceJob, BouncePool};
use crate::config::Config;
use crate::recording::Recording;
use crate::session::Session;
use crate::template::{self, Template};
use crate::timeline::Timeline;

const FAST_DECAY: f32 = 0.976;
const SLOW_DECAY: f32 = 0.990;

pub const WAVEFORM_WINDOWS_SECS: &[u64] = &[10, 30, 60, 300, 1800];

const MAX_HISTORY_SECS: usize = 1800;
/// Initial allocation only. Runtime growth is bounded by sample-threshold eviction.
const LEVEL_HISTORY_CAPACITY_HINT: usize = MAX_HISTORY_SECS * 100;

pub struct App {
    pub config: Config,
    pub session: Session,
    pub engine: EngineHandle,
    pub levels_consumer: rtrb::Consumer<LevelObservation>,
    pub recording: Option<Recording>,
    pub state: AppState,
    pub bounce_pool: BouncePool,
    pub display_levels: Vec<f32>,
    pub peak_holds: Vec<f32>,
    pub level_history: VecDeque<LevelSample>,
    pub total_ticks: u64,
    pub waveform_window_secs: u64,
    last_template_save: Option<Flash<String>>,
    last_template_load: Option<Flash<String>>,
}

/// Rails-style "flash": pairs a value with the moment it was set, so
/// callers can render time-windowed UI status (e.g. "saved 'foo'" for
/// a few seconds after a save). Read access goes through `fresh_within`,
/// which gates on a caller-supplied window.
#[derive(Clone)]
pub struct Flash<T> {
    value: T,
    at: DateTime<Local>,
}

impl<T> Flash<T> {
    pub fn now(value: T) -> Self {
        Self {
            value,
            at: Local::now(),
        }
    }

    /// Returns the wrapped value if it was set within the last `secs` seconds.
    pub fn fresh_within(&self, secs: i64) -> Option<&T> {
        (Local::now() - self.at < Duration::seconds(secs)).then_some(&self.value)
    }
}

const TEMPLATE_FEEDBACK_SECS: i64 = 3;

#[derive(Clone, Copy, Debug)]
pub struct LevelSample {
    /// Absolute engine sample at the moment the entry was captured.
    pub sample: u64,
    pub peak: f32,
    pub recorded: bool,
}

pub enum AppState {
    Default,
    NamingTake {
        buf: String,
        origin: TakeOrigin,
    },
    ConfirmingStop,
    PickingChannel {
        cursor: usize,
        renaming: Option<String>,
    },
    SavingTemplate {
        buf: String,
    },
}

#[derive(Clone, Copy, Debug)]
pub enum TakeOrigin {
    /// T placed a marker; cancel rolls it back.
    Fresh,
    /// N targets an existing marker; cancel just closes.
    Retroactive,
}

#[derive(Debug)]
pub enum AppError {
    NothingArmed,
    NotIdle,
}

impl App {
    pub fn new(device: Device, config: Config) -> Self {
        let (engine, levels_consumer) = EngineHandle::start(device);
        let n = engine.channel_count() as usize;
        let session = Session::new(engine.channel_count());
        App {
            config,
            session,
            engine,
            levels_consumer,
            recording: None,
            state: AppState::Default,
            bounce_pool: BouncePool::start(),
            display_levels: vec![0.0; n],
            peak_holds: vec![0.0; n],
            level_history: VecDeque::with_capacity(LEVEL_HISTORY_CAPACITY_HINT),
            total_ticks: 0,
            waveform_window_secs: WAVEFORM_WINDOWS_SECS[0],
            last_template_save: None,
            last_template_load: None,
        }
    }

    /// Name of the most recently saved template, while its feedback window
    /// is still open. UI uses this for the transient "saved" confirmation.
    pub fn recent_template_save(&self) -> Option<&str> {
        self.last_template_save
            .as_ref()
            .and_then(|t| t.fresh_within(TEMPLATE_FEEDBACK_SECS))
            .map(String::as_str)
    }

    /// Name of the most recently loaded template, while its feedback window
    /// is still open. UI uses this for the transient "loaded" confirmation.
    pub fn recent_template_load(&self) -> Option<&str> {
        self.last_template_load
            .as_ref()
            .and_then(|t| t.fresh_within(TEMPLATE_FEEDBACK_SECS))
            .map(String::as_str)
    }

    pub fn is_recording(&self) -> bool {
        self.recording.as_ref().is_some_and(|r| r.is_writing())
    }

    pub fn current_timeline(&self) -> Option<&Timeline> {
        self.recording.as_ref().map(|r| &r.timeline)
    }

    pub fn current_timeline_mut(&mut self) -> Option<&mut Timeline> {
        self.recording.as_mut().map(|r| &mut r.timeline)
    }

    pub fn tick_display(&mut self) {
        self.total_ticks += 1;
        self.apply_bounce_status_updates();
        self.drain_level_observations();
        self.evict_old_level_history();
    }

    fn apply_bounce_status_updates(&mut self) {
        for update in self.bounce_pool.drain_updates() {
            if let Some(r) = &mut self.recording {
                r.timeline.set_bounce_status(update.take_id, update.status);
            }
        }
    }

    /// Drains observations into both meter decay state and waveform history.
    fn drain_level_observations(&mut self) {
        let n_channels = self.session.channels.len();
        let mut tick_max = vec![0.0f32; n_channels];
        while let Ok(obs) = self.levels_consumer.pop() {
            let mut combined = 0.0f32;
            for (i, &peak) in obs.channel_peaks.iter().take(n_channels).enumerate() {
                tick_max[i] = tick_max[i].max(peak);
                if self.session.channels[i].armed {
                    combined = combined.max(peak);
                }
            }
            self.level_history.push_back(LevelSample {
                sample: obs.sample,
                peak: combined,
                recorded: obs.recorded,
            });
        }
        for (i, &peak) in tick_max.iter().enumerate() {
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
        if self.is_recording() || !matches!(self.state, AppState::Default) {
            return Err(AppError::NotIdle);
        }
        let armed: Vec<ArmedChannel> = self
            .session
            .armed()
            .map(|c| ArmedChannel {
                index: c.index,
                label: c.label.clone(),
            })
            .collect();
        if armed.is_empty() {
            return Err(AppError::NothingArmed);
        }

        let timestamp = Local::now().format("%Y-%m-%d-%H%M%S").to_string();
        let output_dir = self.config.projects_dir.join(&timestamp);

        let consumer = self.engine.start_recording();
        let recording = Recording::start(
            output_dir,
            consumer,
            self.engine.sample_rate(),
            self.engine.channel_count(),
            armed,
            self.sample_position(),
        );
        self.recording = Some(recording);
        Ok(())
    }

    pub fn stop_recording(&mut self) {
        if !self.is_recording() {
            return;
        }
        // Detach producer first: engine.stop_recording is synchronous, so no
        // further samples land in the rtrb after it returns.
        self.engine.stop_recording();
        let sample = self.sample_position();
        if let Some(r) = &mut self.recording {
            r.stop(sample);
        }
        self.state = AppState::Default;
    }

    pub fn drop_marker(&mut self) {
        if !self.can_mark() {
            return;
        }
        let sample = self.sample_position();
        if let Some(r) = &mut self.recording {
            r.mark(sample);
        }
    }

    pub fn mark_and_name(&mut self) {
        if !self.can_mark() {
            return;
        }
        let sample = self.sample_position();
        if let Some(r) = &mut self.recording {
            r.mark(sample);
        }
        self.state = AppState::NamingTake {
            buf: String::new(),
            origin: TakeOrigin::Fresh,
        };
    }

    pub fn name_take(&mut self) {
        if !matches!(self.state, AppState::Default) {
            return;
        }
        if !self
            .current_timeline()
            .is_some_and(|t| t.last_marker_unbound())
        {
            return;
        }
        self.state = AppState::NamingTake {
            buf: String::new(),
            origin: TakeOrigin::Retroactive,
        };
    }

    pub fn delete_last_marker(&mut self) {
        if !self.can_mark() {
            return;
        }
        if let Some(t) = self.current_timeline_mut() {
            t.delete_last_marker();
        }
    }

    pub fn begin_confirm_stop(&mut self) {
        if self.is_recording() && matches!(self.state, AppState::Default) {
            self.state = AppState::ConfirmingStop;
        }
    }

    pub fn cancel_confirm_stop(&mut self) {
        if matches!(self.state, AppState::ConfirmingStop) {
            self.state = AppState::Default;
        }
    }

    pub fn cancel_take_naming(&mut self) {
        let AppState::NamingTake { origin, .. } = self.state else {
            return;
        };
        if matches!(origin, TakeOrigin::Fresh) {
            if let Some(t) = self.current_timeline_mut() {
                t.delete_last_marker();
            }
        }
        self.state = AppState::Default;
    }

    pub fn commit_take_naming(&mut self) {
        let AppState::NamingTake { buf, .. } = &self.state else {
            return;
        };
        let trimmed = buf.trim().to_string();
        if trimmed.is_empty() {
            self.cancel_take_naming();
            return;
        }

        let sample_rate = self.engine.sample_rate();
        let mut new_job: Option<BounceJob> = None;
        if let Some(r) = &mut self.recording {
            if r.timeline.create_take(trimmed) {
                let flushed_samples = r.flushed_samples();
                if let Some(take) = r.timeline.takes().last() {
                    new_job = Some(BounceJob {
                        take: take.clone(),
                        sample_rate,
                        output_dir: self.config.bounces_dir.clone(),
                        recording_timestamp: r.started_at.format("%Y-%m-%d-%H%M%S").to_string(),
                        channel_files: r.channel_files.clone(),
                        flushed_samples,
                    });
                }
            }
        }
        if let Some(job) = new_job {
            self.bounce_pool.dispatch(job);
        }

        self.state = AppState::Default;
    }

    pub fn take_name_append_char(&mut self, c: char) {
        if let AppState::NamingTake { buf, .. } = &mut self.state {
            buf.push(c);
        }
    }

    pub fn take_name_backspace(&mut self) {
        if let AppState::NamingTake { buf, .. } = &mut self.state {
            buf.pop();
        }
    }

    pub fn begin_save_template(&mut self) {
        if self.is_recording() || !matches!(self.state, AppState::Default) {
            return;
        }
        self.state = AppState::SavingTemplate { buf: String::new() };
    }

    pub fn cancel_save_template(&mut self) {
        if matches!(self.state, AppState::SavingTemplate { .. }) {
            self.state = AppState::Default;
        }
    }

    pub fn commit_save_template(&mut self) {
        let AppState::SavingTemplate { buf } = &self.state else {
            return;
        };
        let name = buf.trim();
        if !template::is_valid_name(name) {
            return;
        }
        let path = template::path_for_name(&self.config.templates_dir, name);
        let template = Template {
            name: name.to_string(),
            device_name: self.engine.device_name().to_string(),
            channels: self.session.channels.clone(),
        };
        if template.save(&path).is_ok() {
            self.last_template_save = Some(Flash::now(template.name));
        }
        self.state = AppState::Default;
    }

    pub fn save_template_append_char(&mut self, c: char) {
        if let AppState::SavingTemplate { buf } = &mut self.state {
            buf.push(c);
        }
    }

    pub fn save_template_backspace(&mut self) {
        if let AppState::SavingTemplate { buf } = &mut self.state {
            buf.pop();
        }
    }

    /// Applies `template` to the session and flashes a "loaded"
    /// confirmation. Out-of-range template indices are silently dropped;
    /// channels on the device not covered by the template keep their
    /// fresh defaults.
    pub fn load_template(&mut self, template: &Template) {
        for tmpl_channel in &template.channels {
            if let Some(channel) = self.session.channel_mut(tmpl_channel.index) {
                channel.label = tmpl_channel.label.clone();
                channel.armed = tmpl_channel.armed;
            }
        }
        self.last_template_load = Some(Flash::now(template.name.clone()));
    }

    pub fn open_picker(&mut self) {
        if !self.is_recording() && matches!(self.state, AppState::Default) {
            self.state = AppState::PickingChannel {
                cursor: 0,
                renaming: None,
            };
        }
    }

    pub fn close_picker(&mut self) {
        if matches!(self.state, AppState::PickingChannel { .. }) {
            self.state = AppState::Default;
        }
    }

    pub fn toggle_armed(&mut self, channel_index: u16) {
        if self.is_recording() {
            return;
        }
        if let Some(channel) = self.session.channel_mut(channel_index) {
            channel.armed = !channel.armed;
        }
    }

    pub fn set_label(&mut self, channel_index: u16, label: Option<String>) {
        if let Some(channel) = self.session.channel_mut(channel_index) {
            channel.label = label;
        }
    }

    pub fn sample_position(&self) -> u64 {
        self.engine.sample_position()
    }

    /// Whether marker-list mutations (Space, T, Backspace) are allowed:
    /// actively recording with no overlay open.
    fn can_mark(&self) -> bool {
        self.is_recording() && matches!(self.state, AppState::Default)
    }
}
