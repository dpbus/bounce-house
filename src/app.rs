use std::io;

use crate::audio::DeviceInfo;
use crate::bounce::{BounceJob, BouncePool};
use crate::capture::CaptureError;
use crate::mixer::Mixer;
use crate::session::Session;
use crate::settings::Settings;
use crate::template::{self, Template};
use crate::transport::Transport;

pub struct App {
    pub settings: Settings,
    pub session: Session,
    pub mixer: Mixer,
    pub transport: Transport,
    pub bounce_pool: BouncePool,
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
    pub fn new(info: DeviceInfo, settings: Settings) -> Self {
        let session = Session::new(info.input_sample_rate(), &settings);
        App {
            settings,
            session,
            mixer: Mixer::start(&info),
            transport: Transport::new(),
            bounce_pool: BouncePool::start(),
        }
    }

    pub fn recording_duration_secs(&self) -> Option<u64> {
        let sr = (self.session.sample_rate().0 as u64).max(1);
        let samples = self
            .transport
            .rel_sample_position()
            .or(self.session.end_sample())?;
        Some(samples / sr)
    }

    pub fn tick(&mut self) {
        self.apply_bounce_events();
        let recorded = self.transport.capture().is_some_and(|c| !c.is_paused());
        self.mixer.drain_observations(recorded);
        self.mixer.evict_old_history();
    }

    fn apply_bounce_events(&mut self) {
        let updates = self.bounce_pool.drain_updates();
        for update in updates {
            self.session
                .apply_bounce_event(update.take_id, update.event);
        }
    }

    pub fn start_recording(&mut self) -> Result<(), AppError> {
        if !self.transport.is_idle() {
            return Err(AppError::NotIdle);
        }
        if self.session.has_recording() {
            self.session = self.session.fork_for_new_recording(&self.settings);
        }
        self.transport
            .start_recording(&mut self.session, &self.mixer)?;
        Ok(())
    }

    pub fn stop_recording(&mut self) {
        self.transport.stop_recording(&mut self.session);
    }

    pub fn drop_marker(&mut self) {
        if let Some(rel) = self.transport.rel_sample_position() {
            self.session.drop_marker(rel);
        }
    }

    pub fn delete_last_marker(&mut self) {
        if !self.transport.is_recording() {
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
            flushed_samples: self.transport.capture().map(|c| c.flushed_samples()),
        };
        self.bounce_pool.dispatch(job);
    }

    pub fn save_template(&mut self, name: &str) -> io::Result<()> {
        let path = template::path_for_name(&self.settings.templates_dir, name);
        self.mixer.snapshot_template(name).save(&path)
    }

    pub fn list_templates(&self) -> Vec<Template> {
        template::list(&self.settings.templates_dir).unwrap_or_default()
    }

    pub fn load_template(&mut self, template: &Template) {
        self.mixer.apply_template(template);
    }

    pub fn toggle_armed(&mut self, channel_index: u16) {
        if self.transport.is_recording() {
            return;
        }
        self.mixer.toggle_armed(channel_index);
    }

    pub fn set_label(&mut self, channel_index: u16, label: Option<String>) {
        self.mixer.set_label(channel_index, label);
    }

    pub fn toggle_pause(&mut self) {
        self.transport.toggle_pause();
    }

    pub fn cycle_waveform_window(&mut self) {
        self.mixer.cycle_waveform_window();
    }
}
