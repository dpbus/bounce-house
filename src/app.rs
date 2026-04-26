use std::collections::VecDeque;
use std::path::PathBuf;

use chrono::Local;

use crate::audio::{ArmedChannel, Device, DiskWriter, DiskWriterConfig, Engine};
use crate::recording::Recording;
use crate::session::Session;
use crate::timeline::Timeline;
use crate::units::{ChannelIndex, SamplePosition};

const FAST_DECAY: f32 = 0.976;
const SLOW_DECAY: f32 = 0.990;

/// Approximate UI tick rate. Used to size waveform history relative to wall-clock time.
pub const TICK_FPS: usize = 60;

/// Available waveform window sizes in seconds. Cycled through with the W key.
pub const WAVEFORM_WINDOWS_SECS: &[u64] = &[10, 30, 60, 300, 1800];

const MAX_HISTORY_SECS: usize = 1800;
const MAX_HISTORY_ENTRIES: usize = MAX_HISTORY_SECS * TICK_FPS;

pub struct App {
    pub session: Session,
    pub engine: Engine,
    pub recording: Option<Recording>,
    pub writer: Option<DiskWriter>,
    pub state: AppState,
    pub display_levels: Vec<f32>,
    pub peak_holds: Vec<f32>,
    pub level_history: VecDeque<(f32, bool)>,
    pub total_ticks: u64,
    pub waveform_window_secs: u64,
}

pub enum AppState {
    Default,
    NamingTake { buf: String, origin: TakeOrigin },
    ConfirmingStop,
    PickingChannel { cursor: usize, renaming: Option<String> },
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
    pub fn new(device: Device, raw_dir: PathBuf) -> Self {
        let engine = Engine::start(device);
        let n = engine.channel_count() as usize;
        let session = Session::new(engine.channel_count(), raw_dir);
        App {
            session,
            engine,
            recording: None,
            writer: None,
            state: AppState::Default,
            display_levels: vec![0.0; n],
            peak_holds: vec![0.0; n],
            level_history: VecDeque::with_capacity(MAX_HISTORY_ENTRIES + 1),
            total_ticks: 0,
            waveform_window_secs: WAVEFORM_WINDOWS_SECS[0],
        }
    }

    pub fn is_recording(&self) -> bool {
        self.writer.is_some()
    }

    pub fn current_timeline(&self) -> Option<&Timeline> {
        self.recording.as_ref().map(|r| &r.timeline)
    }

    pub fn current_timeline_mut(&mut self) -> Option<&mut Timeline> {
        self.recording.as_mut().map(|r| &mut r.timeline)
    }

    pub fn tick_display(&mut self) {
        self.total_ticks += 1;
        let recording = self.is_recording();
        let mut combined_peak = 0.0f32;
        for (i, level) in self.engine.levels().iter().enumerate() {
            let peak = level.take_current();
            self.display_levels[i] = peak.max(self.display_levels[i] * FAST_DECAY);
            self.peak_holds[i] = peak.max(self.peak_holds[i] * SLOW_DECAY);
            if self.session.channels[i].armed {
                combined_peak = combined_peak.max(peak);
            }
        }
        self.level_history.push_back((combined_peak, recording));
        while self.level_history.len() > MAX_HISTORY_ENTRIES {
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

        let started_at = Local::now();
        let timestamp = started_at.format("%Y-%m-%d-%H%M%S").to_string();
        let output_dir = self.session.raw_dir.join(&timestamp);

        let consumer = self.engine.start_recording();
        let writer = DiskWriter::start(
            consumer,
            DiskWriterConfig {
                output_dir: output_dir.clone(),
                sample_rate: self.engine.sample_rate(),
                total_channel_count: self.engine.channel_count(),
                armed,
            },
        );

        let mut recording = Recording::new(started_at, output_dir);
        recording.timeline.mark(self.total_ticks);
        self.recording = Some(recording);
        self.writer = Some(writer);
        Ok(())
    }

    pub fn stop_recording(&mut self) {
        if !self.is_recording() {
            return;
        }
        // Detach producer from audio thread first; this is synchronous so by
        // the time it returns, no more samples will land in the rtrb.
        self.engine.stop_recording();
        if let Some(r) = &mut self.recording {
            r.timeline.mark(self.total_ticks);
        }
        // Dropping the writer joins its thread (drains rtrb, finalizes WAV).
        self.writer = None;
        self.state = AppState::Default;
    }

    pub fn drop_marker(&mut self) {
        if !self.can_mutate_timeline() {
            return;
        }
        let tick = self.total_ticks;
        if let Some(t) = self.current_timeline_mut() {
            t.mark(tick);
        }
    }

    pub fn mark_and_name(&mut self) {
        if !self.can_mutate_timeline() {
            return;
        }
        let tick = self.total_ticks;
        if let Some(t) = self.current_timeline_mut() {
            t.mark(tick);
        }
        self.state = AppState::NamingTake {
            buf: String::new(),
            origin: TakeOrigin::Fresh,
        };
    }

    pub fn name_take(&mut self) {
        if !self.can_mutate_timeline() {
            return;
        }
        if !self.current_timeline().is_some_and(|t| t.last_marker_unbound()) {
            return;
        }
        self.state = AppState::NamingTake {
            buf: String::new(),
            origin: TakeOrigin::Retroactive,
        };
    }

    pub fn delete_last_marker(&mut self) {
        if !self.can_mutate_timeline() {
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
        let AppState::NamingTake { origin, .. } = self.state else { return; };
        if matches!(origin, TakeOrigin::Fresh) {
            if let Some(t) = self.current_timeline_mut() {
                t.delete_last_marker();
            }
        }
        self.state = AppState::Default;
    }

    pub fn commit_take_naming(&mut self) {
        let AppState::NamingTake { buf, .. } = &self.state else { return; };
        let trimmed = buf.trim().to_string();
        if trimmed.is_empty() {
            self.cancel_take_naming();
            return;
        }
        if let Some(t) = self.current_timeline_mut() {
            t.create_take(trimmed);
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

    pub fn open_picker(&mut self) {
        if !self.is_recording() && matches!(self.state, AppState::Default) {
            self.state = AppState::PickingChannel { cursor: 0, renaming: None };
        }
    }

    pub fn close_picker(&mut self) {
        if matches!(self.state, AppState::PickingChannel { .. }) {
            self.state = AppState::Default;
        }
    }

    pub fn toggle_armed(&mut self, idx: ChannelIndex) {
        if self.is_recording() {
            return;
        }
        if let Some(channel) = self.session.channel_mut(idx) {
            channel.armed = !channel.armed;
        }
    }

    pub fn set_label(&mut self, idx: ChannelIndex, label: Option<String>) {
        if let Some(channel) = self.session.channel_mut(idx) {
            channel.label = label;
        }
    }

    pub fn sample_position(&self) -> SamplePosition {
        self.engine.sample_position()
    }

    /// Whether marker/take mutations from key actions (Space/T/N/Backspace)
    /// are allowed: must be actively recording with no overlay open.
    fn can_mutate_timeline(&self) -> bool {
        self.is_recording() && matches!(self.state, AppState::Default)
    }
}
