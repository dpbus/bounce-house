use std::path::PathBuf;

use chrono::Local;

use crate::channel::Channel;
use crate::recording::Recording;
use crate::settings::Settings;
use crate::timeline::Timeline;
use crate::units::SampleRate;

/// The body of work the user is creating: channels (with mix), the
/// timeline of markers and takes, the on-disk paths where its audio
/// lives, and the optional in-progress recording. Starting a new
/// recording after one has stopped forks a fresh project (preserving
/// channel state) so each capture gets its own dir and timeline.
pub struct Project {
    pub name: String,
    /// Project root on disk (settings.projects_dir / name). Created
    /// lazily when recording first starts.
    pub dir: PathBuf,
    /// Where this project's bounces (MP3s) go. Either a per-project
    /// subdirectory of settings.bounces_dir (when prefix is None), or
    /// the settings.bounces_dir itself with `bounces_filename_prefix`
    /// prepended to each filename for a flat layout.
    pub bounces_dir: PathBuf,
    pub bounces_filename_prefix: Option<String>,
    pub sample_rate: SampleRate,
    pub channels: Vec<Channel>,
    pub timeline: Timeline,
    pub recording: Option<Recording>,
}

impl Project {
    pub fn new(channel_count: u16, sample_rate: SampleRate, settings: &Settings) -> Self {
        let channels = (0..channel_count).map(Channel::new).collect();
        Self::with_channels(channels, sample_rate, settings)
    }

    /// New project carrying over the current channels (labels, arming,
    /// mix) but with a fresh name, dir, bounces_dir, and timeline.
    /// Called when starting another recording after one has stopped, so
    /// the new capture doesn't collide with the previous one's files
    /// or timeline.
    pub fn fork_for_new_recording(&self, settings: &Settings) -> Self {
        Self::with_channels(self.channels.clone(), self.sample_rate, settings)
    }

    fn with_channels(
        channels: Vec<Channel>,
        sample_rate: SampleRate,
        settings: &Settings,
    ) -> Self {
        let name = Local::now().format("%Y-%m-%d-%H%M%S").to_string();
        let dir = settings.projects_dir.join(&name);
        let bounces_dir = settings.bounces_dir.join(&name);
        Project {
            name,
            dir,
            bounces_dir,
            bounces_filename_prefix: None,
            sample_rate,
            channels,
            timeline: Timeline::new(),
            recording: None,
        }
    }

    pub fn channel_mut(&mut self, index: u16) -> Option<&mut Channel> {
        self.channels.get_mut(index as usize)
    }

    pub fn armed_channels(&self) -> impl Iterator<Item = &Channel> + '_ {
        self.channels.iter().filter(|c| c.armed)
    }

    pub fn start_recording(&mut self, channel_files: Vec<PathBuf>) {
        self.recording = Some(Recording {
            started_at: Local::now(),
            stopped_at: None,
            channel_files,
        });
        self.timeline.mark(0);
    }

    pub fn stop_recording(&mut self, end_rel_sample: u64) {
        if let Some(rec) = &mut self.recording {
            rec.stopped_at = Some(Local::now());
        }
        self.timeline.mark(end_rel_sample);
    }

    pub fn secs_at(&self, sample: u64) -> u64 {
        sample / (self.sample_rate.0 as u64).max(1)
    }

    pub fn duration_secs(&self, start_sample: u64, end_sample: u64) -> u64 {
        self.secs_at(end_sample.saturating_sub(start_sample))
    }

    /// Wall-clock seconds since the recording started; frozen at stop.
    /// Zero when no recording exists yet.
    pub fn elapsed_secs(&self) -> u64 {
        self.recording.as_ref().map(|r| r.elapsed_secs()).unwrap_or(0)
    }

    pub fn since_last_marker_secs(&self, current_rel_sample: u64) -> u64 {
        let last = self
            .timeline
            .markers()
            .last()
            .map(|m| m.sample)
            .unwrap_or(0);
        self.secs_at(current_rel_sample.saturating_sub(last))
    }
}
