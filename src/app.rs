use std::path::PathBuf;
use std::time::Instant;

use chrono::Local;

use crate::audio::{Device, Engine, Recording, RecordingConfig};
use crate::session::Session;
use crate::units::{ChannelIndex, SamplePosition};

const FAST_DECAY: f32 = 0.976;
const SLOW_DECAY: f32 = 0.990;

pub struct App {
    pub session: Session,
    pub engine: Engine,
    pub state: AppState,
    pub display_levels: Vec<f32>,
    pub peak_holds: Vec<f32>,
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
        }
    }

    pub fn tick_display(&mut self) {
        for (i, level) in self.engine.levels().iter().enumerate() {
            let current = level.current();
            self.display_levels[i] = current.max(self.display_levels[i] * FAST_DECAY);
            self.peak_holds[i] = current.max(self.peak_holds[i] * SLOW_DECAY);
        }
    }

    pub fn start_recording(&mut self) -> Result<(), AppError> {
        if !matches!(self.state, AppState::Idle) {
            return Err(AppError::NotRecording);
        }
        let armed: Vec<ChannelIndex> = self.session.armed().map(|c| c.index).collect();
        if armed.is_empty() {
            return Err(AppError::NothingArmed);
        }

        let timestamp = Local::now().format("%Y-%m-%d-%H%M%S");
        let output_path = self
            .session
            .raw_dir
            .join(format!("recording-{}.wav", timestamp));

        let consumer = self.engine.start_recording();
        let recording = Recording::start(
            consumer,
            RecordingConfig {
                output_path,
                sample_rate: self.engine.sample_rate(),
                total_channel_count: self.engine.channel_count(),
                armed,
            },
        );

        self.state = AppState::Recording {
            recording,
            started_at: Instant::now(),
            confirming_stop: false,
        };
        Ok(())
    }

    pub fn stop_recording(&mut self) {
        if !matches!(self.state, AppState::Recording { .. }) {
            return;
        }
        // Detach producer from audio thread first; this is synchronous so by the
        // time it returns, no more samples will land in the rtrb.
        self.engine.stop_recording();
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
