use crate::capture::{Capture, CaptureError};
use crate::mixer::Mixer;
use crate::playback::Playback;
use crate::session::Session;

pub struct Transport {
    state: TransportState,
}

pub enum TransportState {
    Idle,
    Recording(Capture),
    Playing(Playback),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransportMode {
    Idle,
    Recording,
    Paused,
    Playing,
}

#[derive(Debug)]
pub enum RecordingError {
    NotIdle,
    NothingArmed,
}

impl From<CaptureError> for RecordingError {
    fn from(e: CaptureError) -> Self {
        match e {
            CaptureError::NothingArmed => RecordingError::NothingArmed,
        }
    }
}

#[derive(Debug)]
pub enum PlaybackError {
    NotIdle,
    NoRecording,
    NoOutput,
}

impl Transport {
    pub fn new() -> Self {
        Transport {
            state: TransportState::Idle,
        }
    }

    pub fn is_idle(&self) -> bool {
        matches!(self.state, TransportState::Idle)
    }

    pub fn is_recording(&self) -> bool {
        matches!(self.state, TransportState::Recording(_))
    }

    pub fn is_playing(&self) -> bool {
        matches!(self.state, TransportState::Playing(_))
    }

    pub fn capture(&self) -> Option<&Capture> {
        match &self.state {
            TransportState::Recording(c) => Some(c),
            _ => None,
        }
    }

    pub fn capture_mut(&mut self) -> Option<&mut Capture> {
        match &mut self.state {
            TransportState::Recording(c) => Some(c),
            _ => None,
        }
    }

    #[allow(dead_code)]
    pub fn playback(&self) -> Option<&Playback> {
        match &self.state {
            TransportState::Playing(p) => Some(p),
            _ => None,
        }
    }

    pub fn mode(&self) -> TransportMode {
        match &self.state {
            TransportState::Idle => TransportMode::Idle,
            TransportState::Recording(c) if c.is_paused() => TransportMode::Paused,
            TransportState::Recording(_) => TransportMode::Recording,
            TransportState::Playing(_) => TransportMode::Playing,
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
    ) -> Result<(), RecordingError> {
        if !self.is_idle() {
            return Err(RecordingError::NotIdle);
        }
        let armed: Vec<_> = mixer.armed_channels().cloned().collect();
        let capture = Capture::start(&mixer.input_device, session, &armed)?;
        self.state = TransportState::Recording(capture);
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

    pub fn start_playback(
        &mut self,
        session: &Session,
        mixer: &Mixer,
    ) -> Result<(), PlaybackError> {
        if !self.is_idle() {
            return Err(PlaybackError::NotIdle);
        }
        if !session.has_recording() {
            return Err(PlaybackError::NoRecording);
        }
        let Some(output_device) = &mixer.output_device else {
            return Err(PlaybackError::NoOutput);
        };
        self.state = TransportState::Playing(Playback::start(output_device, session));
        Ok(())
    }

    pub fn stop_playback(&mut self) {
        if matches!(self.state, TransportState::Playing(_)) {
            self.state = TransportState::Idle;
        }
    }

    fn take_capture(&mut self) -> Option<Capture> {
        match std::mem::replace(&mut self.state, TransportState::Idle) {
            TransportState::Recording(c) => Some(c),
            other => {
                self.state = other;
                None
            }
        }
    }
}
