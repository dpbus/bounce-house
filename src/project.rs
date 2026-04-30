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
        Self::with_channels(self.channels.clone(), self.sample_rate(), settings)
    }

    fn with_channels(channels: Vec<Channel>, sample_rate: SampleRate, settings: &Settings) -> Self {
        let name = Local::now().format("%Y-%m-%d-%H%M%S").to_string();
        let dir = settings.projects_dir.join(&name);
        let bounces_dir = settings.bounces_dir.join(&name);
        Project {
            name,
            dir,
            bounces_dir,
            bounces_filename_prefix: None,
            channels,
            timeline: Timeline::new(sample_rate),
            recording: None,
        }
    }

    pub fn sample_rate(&self) -> SampleRate {
        self.timeline.sample_rate()
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

    /// Wall-clock seconds since the recording started; frozen at stop.
    /// Zero when no recording exists yet.
    pub fn elapsed_secs(&self) -> u64 {
        self.recording
            .as_ref()
            .map(|r| r.elapsed_secs())
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn settings_in(dir: &std::path::Path) -> Settings {
        Settings {
            projects_dir: dir.join("projects"),
            bounces_dir: dir.join("bounces"),
            templates_dir: dir.join("templates"),
        }
    }

    #[test]
    fn new_creates_default_channels() {
        let dir = tempdir().unwrap();
        let settings = settings_in(dir.path());
        let project = Project::new(4, SampleRate(48_000), &settings);
        assert_eq!(project.channels.len(), 4);
        assert!(
            project
                .channels
                .iter()
                .enumerate()
                .all(|(i, c)| c.index == i as u16)
        );
        assert!(project.channels.iter().all(|c| !c.armed));
        assert!(project.channels.iter().all(|c| c.label.is_none()));
    }

    #[test]
    fn new_derives_paths_from_settings_and_name() {
        let dir = tempdir().unwrap();
        let settings = settings_in(dir.path());
        let project = Project::new(2, SampleRate(48_000), &settings);
        assert_eq!(project.dir, settings.projects_dir.join(&project.name));
        assert_eq!(
            project.bounces_dir,
            settings.bounces_dir.join(&project.name)
        );
    }

    #[test]
    fn new_starts_with_no_recording_and_empty_timeline() {
        let dir = tempdir().unwrap();
        let project = Project::new(1, SampleRate(48_000), &settings_in(dir.path()));
        assert!(project.recording.is_none());
        assert!(project.timeline.markers().is_empty());
        assert!(project.timeline.takes().is_empty());
        assert!(project.bounces_filename_prefix.is_none());
    }

    #[test]
    fn sample_rate_delegates_to_timeline() {
        let dir = tempdir().unwrap();
        let project = Project::new(1, SampleRate(96_000), &settings_in(dir.path()));
        assert_eq!(project.sample_rate(), SampleRate(96_000));
    }

    #[test]
    fn channel_mut_returns_some_for_valid_index() {
        let dir = tempdir().unwrap();
        let mut project = Project::new(3, SampleRate(48_000), &settings_in(dir.path()));
        assert!(project.channel_mut(0).is_some());
        assert!(project.channel_mut(2).is_some());
        assert!(project.channel_mut(3).is_none());
        assert!(project.channel_mut(99).is_none());
    }

    #[test]
    fn armed_channels_filters_to_armed_only() {
        let dir = tempdir().unwrap();
        let mut project = Project::new(4, SampleRate(48_000), &settings_in(dir.path()));
        project.channels[0].armed = true;
        project.channels[2].armed = true;

        let armed: Vec<u16> = project.armed_channels().map(|c| c.index).collect();
        assert_eq!(armed, vec![0, 2]);
    }

    #[test]
    fn start_recording_populates_recording_and_drops_zero_mark() {
        let dir = tempdir().unwrap();
        let mut project = Project::new(1, SampleRate(48_000), &settings_in(dir.path()));
        let files = vec![PathBuf::from("/tmp/ch0.wav")];

        project.start_recording(files.clone());

        let rec = project.recording.as_ref().expect("recording set");
        assert_eq!(rec.channel_files, files);
        assert!(rec.stopped_at.is_none());

        let markers: Vec<u64> = project
            .timeline
            .markers()
            .iter()
            .map(|m| m.sample)
            .collect();
        assert_eq!(markers, vec![0]);
    }

    #[test]
    fn stop_recording_stamps_stopped_at_and_drops_end_mark() {
        let dir = tempdir().unwrap();
        let mut project = Project::new(1, SampleRate(48_000), &settings_in(dir.path()));
        project.start_recording(vec![]);

        project.stop_recording(96_000);

        let rec = project.recording.as_ref().expect("recording set");
        assert!(rec.stopped_at.is_some());

        let markers: Vec<u64> = project
            .timeline
            .markers()
            .iter()
            .map(|m| m.sample)
            .collect();
        assert_eq!(markers, vec![0, 96_000]);
    }

    #[test]
    fn stop_recording_without_active_recording_still_marks_timeline() {
        // Defensive — no panic, marker still drops.
        let dir = tempdir().unwrap();
        let mut project = Project::new(1, SampleRate(48_000), &settings_in(dir.path()));
        project.stop_recording(48_000);
        assert!(project.recording.is_none());
        let markers: Vec<u64> = project
            .timeline
            .markers()
            .iter()
            .map(|m| m.sample)
            .collect();
        assert_eq!(markers, vec![48_000]);
    }

    #[test]
    fn elapsed_secs_zero_when_no_recording() {
        let dir = tempdir().unwrap();
        let project = Project::new(1, SampleRate(48_000), &settings_in(dir.path()));
        assert_eq!(project.elapsed_secs(), 0);
    }

    #[test]
    fn fork_for_new_recording_preserves_channels_and_sample_rate() {
        let dir = tempdir().unwrap();
        let settings = settings_in(dir.path());
        let mut original = Project::new(3, SampleRate(96_000), &settings);
        original.channels[1].armed = true;
        original.channels[1].label = Some("Snare".into());

        let forked = original.fork_for_new_recording(&settings);
        assert_eq!(forked.sample_rate(), SampleRate(96_000));
        assert_eq!(forked.channels.len(), 3);
        assert!(forked.channels[1].armed);
        assert_eq!(forked.channels[1].label.as_deref(), Some("Snare"));
    }

    #[test]
    fn fork_for_new_recording_resets_recording_state() {
        let dir = tempdir().unwrap();
        let settings = settings_in(dir.path());
        let mut original = Project::new(1, SampleRate(48_000), &settings);
        original.start_recording(vec![PathBuf::from("/tmp/old.wav")]);
        original.stop_recording(48_000);

        let forked = original.fork_for_new_recording(&settings);
        assert!(forked.recording.is_none());
        assert!(forked.timeline.markers().is_empty());
        assert!(forked.timeline.takes().is_empty());
    }

    #[test]
    fn fork_for_new_recording_produces_distinct_dirs_when_seconds_differ() {
        // The timestamp-based name uses second resolution. If two forks
        // happen within the same second they'll collide — that's a real
        // limitation we accept (users don't fork twice per second).
        // This test guards the cross-second case.
        let dir = tempdir().unwrap();
        let settings = settings_in(dir.path());
        let original = Project::new(1, SampleRate(48_000), &settings);
        std::thread::sleep(std::time::Duration::from_millis(1100));
        let forked = original.fork_for_new_recording(&settings);
        assert_ne!(original.name, forked.name);
        assert_ne!(original.dir, forked.dir);
        assert_ne!(original.bounces_dir, forked.bounces_dir);
    }
}
