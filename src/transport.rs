use crate::capture::{Capture, CaptureError};
use crate::mixer::Mixer;
use crate::session::Session;

pub struct Transport {
    pub runtime_mode: RuntimeMode,
}

pub enum RuntimeMode {
    Idle,
    Recording(Capture),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecordingState {
    Idle,
    Recording,
    Paused,
}

impl Transport {
    pub fn new() -> Self {
        Transport {
            runtime_mode: RuntimeMode::Idle,
        }
    }

    pub fn is_idle(&self) -> bool {
        matches!(self.runtime_mode, RuntimeMode::Idle)
    }

    pub fn is_recording(&self) -> bool {
        matches!(self.runtime_mode, RuntimeMode::Recording(_))
    }

    pub fn capture(&self) -> Option<&Capture> {
        match &self.runtime_mode {
            RuntimeMode::Recording(c) => Some(c),
            RuntimeMode::Idle => None,
        }
    }

    pub fn capture_mut(&mut self) -> Option<&mut Capture> {
        match &mut self.runtime_mode {
            RuntimeMode::Recording(c) => Some(c),
            RuntimeMode::Idle => None,
        }
    }

    pub fn recording_state(&self) -> RecordingState {
        match self.capture() {
            Some(c) if c.is_paused() => RecordingState::Paused,
            Some(_) => RecordingState::Recording,
            None => RecordingState::Idle,
        }
    }

    pub fn rel_sample_position(&self) -> Option<u64> {
        self.capture().map(|c| c.rel_sample_position())
    }

    pub fn relative_to_absolute(&self, rel: u64) -> Option<u64> {
        self.capture().map(|c| c.absolute(rel))
    }

    pub fn start_recording(
        &mut self,
        session: &mut Session,
        mixer: &Mixer,
    ) -> Result<(), CaptureError> {
        let armed: Vec<_> = mixer.armed_channels().cloned().collect();
        let capture = Capture::start(&mixer.audio_input, session, &armed)?;
        self.runtime_mode = RuntimeMode::Recording(capture);
        Ok(())
    }

    pub fn stop_recording(&mut self, session: &mut Session) {
        if let Some(capture) = self.take_capture() {
            capture.stop(session);
        }
    }

    pub fn toggle_pause(&mut self) {
        let Some(capture) = self.capture_mut() else {
            return;
        };
        if capture.is_paused() {
            capture.resume();
        } else {
            capture.pause();
        }
    }

    fn take_capture(&mut self) -> Option<Capture> {
        match std::mem::replace(&mut self.runtime_mode, RuntimeMode::Idle) {
            RuntimeMode::Recording(c) => Some(c),
            RuntimeMode::Idle => None,
        }
    }
}
