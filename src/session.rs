use std::path::{Path, PathBuf};

use chrono::Local;
use uuid::Uuid;

use crate::bounce::BounceEvent;
use crate::channel::Channel;
use crate::paths;
use crate::recording::{RecordedChannel, Recording};
use crate::settings::Settings;
use crate::timeline::{BounceStatus, Take, Timeline};
use crate::units::SampleRate;

const CHANNELS_DIR: &str = "channels";

/// A single recording session: channels (with mix), the timeline of
/// markers and takes, the on-disk paths where its audio lives, and the
/// optional in-progress recording. Starting a new recording after one
/// has stopped forks a fresh session (preserving channel state) so each
/// capture gets its own dir and timeline.
///
/// All mutations to channels and the timeline route through Session
/// methods so a future persistence hook has a single chokepoint to fire
/// from. Direct field access from outside the module is read-only.
pub struct Session {
    pub name: String,
    /// Where this session's bounces (MP3s) go. Either a per-session
    /// subdirectory of settings.bounces_dir (when prefix is None), or
    /// the settings.bounces_dir itself with `bounces_filename_prefix`
    /// prepended to each filename for a flat layout.
    pub bounces_dir: PathBuf,
    pub bounces_filename_prefix: Option<String>,
    pub recording: Option<Recording>,
    /// Session root on disk (settings.sessions_dir / name). Created
    /// lazily when recording first starts.
    dir: PathBuf,
    channels: Vec<Channel>,
    timeline: Timeline,
}

impl Session {
    pub fn new(channel_count: u16, sample_rate: SampleRate, settings: &Settings) -> Self {
        let channels = (0..channel_count).map(Channel::new).collect();
        Self::with_channels(channels, sample_rate, settings)
    }

    /// New session carrying over the current channels (labels, arming,
    /// mix) but with a fresh name, dir, bounces_dir, and timeline.
    /// Called when starting another recording after one has stopped, so
    /// the new capture doesn't collide with the previous one's files
    /// or timeline.
    pub fn fork_for_new_recording(&self, settings: &Settings) -> Self {
        Self::with_channels(self.channels.clone(), self.sample_rate(), settings)
    }

    fn with_channels(channels: Vec<Channel>, sample_rate: SampleRate, settings: &Settings) -> Self {
        let name = Local::now().format("%Y-%m-%d-%H%M%S").to_string();
        let dir = settings.sessions_dir.join(&name);
        let bounces_dir = settings.bounces_dir.join(&name);
        Session {
            name,
            dir,
            bounces_dir,
            bounces_filename_prefix: None,
            channels,
            timeline: Timeline::new(sample_rate),
            recording: None,
        }
    }

    pub fn channels(&self) -> &[Channel] {
        &self.channels
    }

    pub fn timeline(&self) -> &Timeline {
        &self.timeline
    }

    pub fn sample_rate(&self) -> SampleRate {
        self.timeline.sample_rate()
    }

    pub fn armed_channels(&self) -> impl Iterator<Item = &Channel> + '_ {
        self.channels.iter().filter(|c| c.armed)
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Absolute paths to the recording's channel WAV files, in order.
    /// Empty if no recording exists.
    pub fn recording_channel_paths(&self) -> Vec<PathBuf> {
        self.recording
            .as_ref()
            .map(|r| r.channels.iter().map(|c| self.dir.join(&c.file)).collect())
            .unwrap_or_default()
    }

    /// Path relative to `self.dir` — the form stored on `Recording`.
    fn channel_subpath(&self, channel: &Channel) -> PathBuf {
        Path::new(CHANNELS_DIR).join(channel_filename(channel))
    }

    pub fn last_marker_unbound(&self) -> bool {
        self.timeline.last_marker_unbound()
    }

    pub fn set_channel_label(&mut self, index: u16, label: Option<String>) {
        if let Some(channel) = self.channels.get_mut(index as usize) {
            channel.label = label;
        }
    }

    pub fn set_channel_armed(&mut self, index: u16, armed: bool) {
        if let Some(channel) = self.channels.get_mut(index as usize) {
            channel.armed = armed;
        }
    }

    pub fn toggle_channel_armed(&mut self, index: u16) {
        if let Some(channel) = self.channels.get_mut(index as usize) {
            channel.armed = !channel.armed;
        }
    }

    pub fn drop_marker(&mut self, sample: u64) {
        self.timeline.mark(sample);
    }

    pub fn delete_last_marker(&mut self) -> bool {
        self.timeline.delete_last_marker()
    }

    /// Promotes the trailing unbound marker into a named take. Returns
    /// the new take by value, or None if there's no unbound marker.
    pub fn create_take(&mut self, name: String) -> Option<Take> {
        if !self.timeline.create_take(name) {
            return None;
        }
        self.timeline.takes().last().cloned()
    }

    pub fn apply_bounce_event(&mut self, take_id: Uuid, event: BounceEvent) {
        match event {
            BounceEvent::Started => {
                self.timeline
                    .set_bounce_status(take_id, BounceStatus::Bouncing);
            }
            BounceEvent::Done(path) => {
                self.timeline.set_bounce_path(take_id, path);
                self.timeline.set_bounce_status(take_id, BounceStatus::Done);
            }
            BounceEvent::Failed => {
                self.timeline
                    .set_bounce_status(take_id, BounceStatus::Failed);
            }
        }
    }

    /// Snapshots the currently armed channels into a Recording. Returns
    /// the snapshot, or None if nothing's armed (no Recording is created).
    pub fn start_recording(&mut self) -> Option<Vec<RecordedChannel>> {
        let channels: Vec<RecordedChannel> = self
            .armed_channels()
            .map(|c| RecordedChannel {
                index: c.index,
                label: c.label.clone(),
                file: self.channel_subpath(c),
            })
            .collect();
        if channels.is_empty() {
            return None;
        }
        self.recording = Some(Recording {
            started_at: Local::now(),
            stopped_at: None,
            channels: channels.clone(),
        });
        self.timeline.mark(0);
        Some(channels)
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

fn channel_filename(channel: &Channel) -> String {
    match channel.label.as_deref().map(str::trim) {
        Some(label) if !label.is_empty() => {
            format!("ch{:02}-{}.wav", channel.index, paths::filename_safe(label))
        }
        _ => format!("ch{:02}.wav", channel.index),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn settings_in(dir: &std::path::Path) -> Settings {
        Settings {
            sessions_dir: dir.join("sessions"),
            bounces_dir: dir.join("bounces"),
            templates_dir: dir.join("templates"),
        }
    }

    #[test]
    fn new_creates_default_channels() {
        let dir = tempdir().unwrap();
        let settings = settings_in(dir.path());
        let session = Session::new(4, SampleRate(48_000), &settings);
        assert_eq!(session.channels.len(), 4);
        assert!(
            session
                .channels
                .iter()
                .enumerate()
                .all(|(i, c)| c.index == i as u16)
        );
        assert!(session.channels.iter().all(|c| !c.armed));
        assert!(session.channels.iter().all(|c| c.label.is_none()));
    }

    #[test]
    fn new_derives_paths_from_settings_and_name() {
        let dir = tempdir().unwrap();
        let settings = settings_in(dir.path());
        let session = Session::new(2, SampleRate(48_000), &settings);
        assert_eq!(session.dir, settings.sessions_dir.join(&session.name));
        assert_eq!(
            session.bounces_dir,
            settings.bounces_dir.join(&session.name)
        );
    }

    #[test]
    fn new_starts_with_no_recording_and_empty_timeline() {
        let dir = tempdir().unwrap();
        let session = Session::new(1, SampleRate(48_000), &settings_in(dir.path()));
        assert!(session.recording.is_none());
        assert!(session.timeline.markers().is_empty());
        assert!(session.timeline.takes().is_empty());
        assert!(session.bounces_filename_prefix.is_none());
    }

    #[test]
    fn sample_rate_delegates_to_timeline() {
        let dir = tempdir().unwrap();
        let session = Session::new(1, SampleRate(96_000), &settings_in(dir.path()));
        assert_eq!(session.sample_rate(), SampleRate(96_000));
    }

    #[test]
    fn set_channel_label_updates_only_the_indexed_channel() {
        let dir = tempdir().unwrap();
        let mut session = Session::new(3, SampleRate(48_000), &settings_in(dir.path()));
        session.set_channel_label(1, Some("Snare".into()));
        assert_eq!(session.channels()[1].label.as_deref(), Some("Snare"));
        assert!(session.channels()[0].label.is_none());
        assert!(session.channels()[2].label.is_none());
    }

    #[test]
    fn set_channel_armed_out_of_range_is_silent() {
        let dir = tempdir().unwrap();
        let mut session = Session::new(2, SampleRate(48_000), &settings_in(dir.path()));
        session.set_channel_armed(99, true);
        assert!(session.channels().iter().all(|c| !c.armed));
    }

    #[test]
    fn toggle_channel_armed_flips_state() {
        let dir = tempdir().unwrap();
        let mut session = Session::new(2, SampleRate(48_000), &settings_in(dir.path()));
        session.toggle_channel_armed(0);
        assert!(session.channels()[0].armed);
        session.toggle_channel_armed(0);
        assert!(!session.channels()[0].armed);
    }

    #[test]
    fn armed_channels_filters_to_armed_only() {
        let dir = tempdir().unwrap();
        let mut session = Session::new(4, SampleRate(48_000), &settings_in(dir.path()));
        session.set_channel_armed(0, true);
        session.set_channel_armed(2, true);

        let armed: Vec<u16> = session.armed_channels().map(|c| c.index).collect();
        assert_eq!(armed, vec![0, 2]);
    }

    #[test]
    fn start_recording_snapshots_armed_channels() {
        let dir = tempdir().unwrap();
        let mut session = Session::new(2, SampleRate(48_000), &settings_in(dir.path()));
        session.set_channel_armed(0, true);
        session.set_channel_label(0, Some("Kick".into()));

        let recorded = session.start_recording().expect("armed channel exists");

        assert_eq!(recorded.len(), 1);
        assert_eq!(recorded[0].index, 0);
        assert_eq!(recorded[0].label.as_deref(), Some("Kick"));
        assert_eq!(recorded[0].file, PathBuf::from("channels/ch00-Kick.wav"));

        let rec = session.recording.as_ref().expect("recording set");
        assert_eq!(rec.channels.len(), 1);
        assert!(rec.stopped_at.is_none());

        let markers: Vec<u64> = session
            .timeline
            .markers()
            .iter()
            .map(|m| m.sample)
            .collect();
        assert_eq!(markers, vec![0]);
    }

    #[test]
    fn start_recording_returns_none_when_nothing_armed() {
        let dir = tempdir().unwrap();
        let mut session = Session::new(2, SampleRate(48_000), &settings_in(dir.path()));
        assert!(session.start_recording().is_none());
        assert!(session.recording.is_none());
        assert!(session.timeline.markers().is_empty());
    }

    #[test]
    fn stop_recording_stamps_stopped_at_and_drops_end_mark() {
        let dir = tempdir().unwrap();
        let mut session = Session::new(1, SampleRate(48_000), &settings_in(dir.path()));
        session.set_channel_armed(0, true);
        session.start_recording().expect("armed");

        session.stop_recording(96_000);

        let rec = session.recording.as_ref().expect("recording set");
        assert!(rec.stopped_at.is_some());

        let markers: Vec<u64> = session
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
        let mut session = Session::new(1, SampleRate(48_000), &settings_in(dir.path()));
        session.stop_recording(48_000);
        assert!(session.recording.is_none());
        let markers: Vec<u64> = session
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
        let session = Session::new(1, SampleRate(48_000), &settings_in(dir.path()));
        assert_eq!(session.elapsed_secs(), 0);
    }

    #[test]
    fn fork_for_new_recording_preserves_channels_and_sample_rate() {
        let dir = tempdir().unwrap();
        let settings = settings_in(dir.path());
        let mut original = Session::new(3, SampleRate(96_000), &settings);
        original.set_channel_armed(1, true);
        original.set_channel_label(1, Some("Snare".into()));

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
        let mut original = Session::new(1, SampleRate(48_000), &settings);
        original.set_channel_armed(0, true);
        original.start_recording().expect("armed");
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
        let original = Session::new(1, SampleRate(48_000), &settings);
        std::thread::sleep(std::time::Duration::from_millis(1100));
        let forked = original.fork_for_new_recording(&settings);
        assert_ne!(original.name, forked.name);
        assert_ne!(original.dir, forked.dir);
        assert_ne!(original.bounces_dir, forked.bounces_dir);
    }

    fn ch(index: u16, label: Option<&str>) -> Channel {
        Channel {
            index,
            label: label.map(String::from),
            armed: true,
        }
    }

    #[test]
    fn channel_filename_uses_label_when_present() {
        assert_eq!(channel_filename(&ch(7, Some("Kick"))), "ch07-Kick.wav");
    }

    #[test]
    fn channel_filename_omits_label_when_blank() {
        assert_eq!(channel_filename(&ch(3, None)), "ch03.wav");
        assert_eq!(channel_filename(&ch(3, Some("   "))), "ch03.wav");
    }

    #[test]
    fn channel_filename_sanitizes_unsafe_label_chars() {
        assert_eq!(
            channel_filename(&ch(0, Some("kick/snare"))),
            "ch00-kick_snare.wav"
        );
    }

    #[test]
    fn channel_subpath_nests_under_channels_dir() {
        let dir = tempdir().unwrap();
        let session = Session::new(1, SampleRate(48_000), &settings_in(dir.path()));
        assert_eq!(
            session.channel_subpath(&ch(0, Some("Kick"))),
            std::path::PathBuf::from("channels/ch00-Kick.wav")
        );
    }
}
