use std::collections::VecDeque;
use std::path::PathBuf;
use std::time::Instant;

use chrono::Local;

use crate::audio::{ArmedChannel, Device, Engine, Recording, RecordingConfig};
use crate::session::Session;
use crate::timeline::Timeline;
use crate::units::{ChannelIndex, SamplePosition};

const FAST_DECAY: f32 = 0.976;
const SLOW_DECAY: f32 = 0.990;

/// Approximate UI tick rate. Used to size waveform history relative to wall-clock time.
pub const TICK_FPS: usize = 60;

/// Available waveform window sizes in seconds. Cycled through with the W key.
pub const WAVEFORM_WINDOWS_SECS: &[u64] = &[10, 30, 60, 300, 1800];

/// History buffer is bounded at the longest configurable window.
const MAX_HISTORY_SECS: usize = 1800;
const MAX_HISTORY_ENTRIES: usize = MAX_HISTORY_SECS * TICK_FPS;

pub struct App {
    pub session: Session,
    pub engine: Engine,
    pub state: AppState,
    pub display_levels: Vec<f32>,
    pub peak_holds: Vec<f32>,
    pub level_history: VecDeque<(f32, bool)>,
    pub total_ticks: u64,
    pub timeline: Timeline,
    pub waveform_window_secs: u64,
}

pub enum AppState {
    Idle,
    Recording {
        recording: Recording,
        started_at: Instant,
        confirming_stop: bool,
    },
    PickingChannel {
        cursor: usize,
        renaming: Option<String>,
    },
}

#[derive(Debug)]
pub enum AppError {
    NothingArmed,
    NotRecording,
}

impl App {
    pub fn new(device: Device, raw_dir: PathBuf) -> Self {
        let engine = Engine::start(device);
        let n = engine.channel_count() as usize;
        let session = Session::new(engine.channel_count(), raw_dir);
        App {
            session,
            engine,
            state: AppState::Idle,
            display_levels: vec![0.0; n],
            peak_holds: vec![0.0; n],
            level_history: VecDeque::with_capacity(MAX_HISTORY_ENTRIES + 1),
            total_ticks: 0,
            timeline: Timeline::new(),
            waveform_window_secs: WAVEFORM_WINDOWS_SECS[0],
        }
    }

    pub fn tick_display(&mut self) {
        self.total_ticks += 1;
        let recording = matches!(self.state, AppState::Recording { .. });
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
        if !matches!(self.state, AppState::Idle) {
            return Err(AppError::NotRecording);
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
        let output_dir = self.session.raw_dir.join(&timestamp);

        let consumer = self.engine.start_recording();
        let recording = Recording::start(
            consumer,
            RecordingConfig {
                output_dir,
                sample_rate: self.engine.sample_rate(),
                total_channel_count: self.engine.channel_count(),
                armed,
            },
        );

        self.timeline.mark(self.total_ticks);
        self.state = AppState::Recording {
            recording,
            started_at: Instant::now(),
            confirming_stop: false,
        };
        Ok(())
    }

    pub fn drop_marker(&mut self) {
        if self.is_recording_active() {
            self.timeline.mark(self.total_ticks);
        }
    }

    pub fn mark_and_name(&mut self) {
        if self.is_recording_active() {
            self.timeline.mark_and_name(self.total_ticks);
        }
    }

    pub fn name_take(&mut self) {
        if self.is_recording_active() {
            self.timeline.name_take();
        }
    }

    pub fn delete_last_marker(&mut self) {
        if self.is_recording_active() {
            self.timeline.delete_last_marker();
        }
    }

    fn is_recording_active(&self) -> bool {
        matches!(self.state, AppState::Recording { confirming_stop: false, .. })
    }

    pub fn stop_recording(&mut self) {
        if !matches!(self.state, AppState::Recording { .. }) {
            return;
        }
        // Detach producer from audio thread first; this is synchronous so by the
        // time it returns, no more samples will land in the rtrb.
        self.engine.stop_recording();
        self.timeline.mark(self.total_ticks);
        // Replacing the state drops the Recording, whose Drop joins the writer
        // thread (which drains the rtrb and finalizes the WAV).
        self.state = AppState::Idle;
    }

    pub fn toggle_armed(&mut self, idx: ChannelIndex) {
        if matches!(self.state, AppState::Recording { .. }) {
            return; // Locked while recording
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
}
