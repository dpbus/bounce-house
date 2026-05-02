use std::path::{Path, PathBuf};

use chrono::Local;
use uuid::Uuid;

use crate::bounce::BounceEvent;
use crate::channel::Channel;
use crate::live_channel::LiveChannel;
use crate::paths;
use crate::settings::Settings;
use crate::timeline::{BounceStatus, Take, Timeline};
use crate::units::SampleRate;

const CHANNELS_DIR: &str = "channels";

pub struct Session {
    pub name: String,
    pub bounces_dir: PathBuf,
    pub bounces_filename_prefix: Option<String>,
    channels: Vec<Channel>,
    end_sample: Option<u64>,
    dir: PathBuf,
    timeline: Timeline,
}

impl Session {
    pub fn new(sample_rate: SampleRate, settings: &Settings) -> Self {
        let name = Local::now().format("%Y-%m-%d-%H%M%S").to_string();
        let dir = settings.sessions_dir.join(&name);
        let bounces_dir = settings.bounces_dir.join(&name);
        Session {
            name,
            dir,
            bounces_dir,
            bounces_filename_prefix: None,
            channels: Vec::new(),
            end_sample: None,
            timeline: Timeline::new(sample_rate),
        }
    }

    pub fn fork_for_new_recording(&self, settings: &Settings) -> Self {
        Self::new(self.sample_rate(), settings)
    }

    pub fn timeline(&self) -> &Timeline {
        &self.timeline
    }

    pub fn sample_rate(&self) -> SampleRate {
        self.timeline.sample_rate()
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn channels(&self) -> &[Channel] {
        &self.channels
    }

    pub fn end_sample(&self) -> Option<u64> {
        self.end_sample
    }

    pub fn has_recording(&self) -> bool {
        !self.channels.is_empty()
    }

    pub fn recording_channel_paths(&self) -> Vec<PathBuf> {
        self.channels
            .iter()
            .map(|c| self.dir.join(&c.file))
            .collect()
    }

    fn channel_subpath(live: &LiveChannel) -> PathBuf {
        Path::new(CHANNELS_DIR).join(channel_filename(live))
    }

    pub fn last_marker_unbound(&self) -> bool {
        self.timeline.last_marker_unbound()
    }

    pub fn drop_marker(&mut self, sample: u64) {
        self.timeline.mark(sample);
    }

    pub fn delete_last_marker(&mut self) -> bool {
        self.timeline.delete_last_marker()
    }

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

    pub fn start_recording(&mut self, armed_channels: &[LiveChannel]) -> Option<Vec<Channel>> {
        if armed_channels.is_empty() {
            return None;
        }
        let channels: Vec<Channel> = armed_channels
            .iter()
            .map(|c| Channel {
                index: c.index,
                label: c.label.clone(),
                file: Self::channel_subpath(c),
            })
            .collect();
        self.channels = channels.clone();
        self.end_sample = None;
        self.timeline.mark(0);
        Some(channels)
    }

    pub fn stop_recording(&mut self, end_sample: u64) {
        if self.has_recording() {
            self.end_sample = Some(end_sample);
        }
        self.timeline.mark(end_sample);
    }
}

fn channel_filename(channel: &LiveChannel) -> String {
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

    fn live(index: u16, label: Option<&str>, armed: bool) -> LiveChannel {
        LiveChannel {
            index,
            label: label.map(String::from),
            armed,
        }
    }

    #[test]
    fn new_derives_paths_from_settings_and_name() {
        let dir = tempdir().unwrap();
        let settings = settings_in(dir.path());
        let session = Session::new(SampleRate(48_000), &settings);
        assert_eq!(session.dir, settings.sessions_dir.join(&session.name));
        assert_eq!(
            session.bounces_dir,
            settings.bounces_dir.join(&session.name)
        );
    }

    #[test]
    fn new_starts_empty() {
        let dir = tempdir().unwrap();
        let session = Session::new(SampleRate(48_000), &settings_in(dir.path()));
        assert!(!session.has_recording());
        assert!(session.end_sample().is_none());
        assert!(session.channels().is_empty());
        assert!(session.timeline.markers().is_empty());
        assert!(session.timeline.takes().is_empty());
        assert!(session.bounces_filename_prefix.is_none());
    }

    #[test]
    fn sample_rate_delegates_to_timeline() {
        let dir = tempdir().unwrap();
        let session = Session::new(SampleRate(96_000), &settings_in(dir.path()));
        assert_eq!(session.sample_rate(), SampleRate(96_000));
    }

    #[test]
    fn start_recording_snapshots_provided_channels() {
        let dir = tempdir().unwrap();
        let mut session = Session::new(SampleRate(48_000), &settings_in(dir.path()));
        let armed = vec![live(0, Some("Kick"), true)];

        let recorded = session.start_recording(&armed).expect("non-empty");

        assert_eq!(recorded.len(), 1);
        assert_eq!(recorded[0].index, 0);
        assert_eq!(recorded[0].label.as_deref(), Some("Kick"));
        assert_eq!(recorded[0].file, PathBuf::from("channels/ch00-Kick.wav"));

        assert!(session.has_recording());
        assert_eq!(session.channels().len(), 1);
        assert!(session.end_sample().is_none());

        let markers: Vec<u64> = session
            .timeline
            .markers()
            .iter()
            .map(|m| m.sample)
            .collect();
        assert_eq!(markers, vec![0]);
    }

    #[test]
    fn start_recording_returns_none_when_empty() {
        let dir = tempdir().unwrap();
        let mut session = Session::new(SampleRate(48_000), &settings_in(dir.path()));
        assert!(session.start_recording(&[]).is_none());
        assert!(!session.has_recording());
        assert!(session.timeline.markers().is_empty());
    }

    #[test]
    fn stop_recording_stamps_end_sample_and_drops_end_mark() {
        let dir = tempdir().unwrap();
        let mut session = Session::new(SampleRate(48_000), &settings_in(dir.path()));
        let live_channels = vec![live(0, None, true)];
        session.start_recording(&live_channels).expect("armed");

        session.stop_recording(96_000);

        assert_eq!(session.end_sample(), Some(96_000));

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
        let mut session = Session::new(SampleRate(48_000), &settings_in(dir.path()));
        session.stop_recording(48_000);
        assert!(!session.has_recording());
        assert!(session.end_sample().is_none());
        let markers: Vec<u64> = session
            .timeline
            .markers()
            .iter()
            .map(|m| m.sample)
            .collect();
        assert_eq!(markers, vec![48_000]);
    }

    #[test]
    fn fork_for_new_recording_preserves_sample_rate() {
        let dir = tempdir().unwrap();
        let settings = settings_in(dir.path());
        let original = Session::new(SampleRate(96_000), &settings);
        let forked = original.fork_for_new_recording(&settings);
        assert_eq!(forked.sample_rate(), SampleRate(96_000));
    }

    #[test]
    fn fork_for_new_recording_resets_recording_state() {
        let dir = tempdir().unwrap();
        let settings = settings_in(dir.path());
        let mut original = Session::new(SampleRate(48_000), &settings);
        let live_channels = vec![live(0, None, true)];
        original.start_recording(&live_channels).expect("armed");
        original.stop_recording(48_000);

        let forked = original.fork_for_new_recording(&settings);
        assert!(!forked.has_recording());
        assert!(forked.end_sample().is_none());
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
        let original = Session::new(SampleRate(48_000), &settings);
        std::thread::sleep(std::time::Duration::from_millis(1100));
        let forked = original.fork_for_new_recording(&settings);
        assert_ne!(original.name, forked.name);
        assert_ne!(original.dir, forked.dir);
        assert_ne!(original.bounces_dir, forked.bounces_dir);
    }

    #[test]
    fn channel_filename_uses_label_when_present() {
        assert_eq!(
            channel_filename(&live(7, Some("Kick"), true)),
            "ch07-Kick.wav"
        );
    }

    #[test]
    fn channel_filename_omits_label_when_blank() {
        assert_eq!(channel_filename(&live(3, None, true)), "ch03.wav");
        assert_eq!(channel_filename(&live(3, Some("   "), true)), "ch03.wav");
    }

    #[test]
    fn channel_filename_sanitizes_unsafe_label_chars() {
        assert_eq!(
            channel_filename(&live(0, Some("kick/snare"), true)),
            "ch00-kick_snare.wav"
        );
    }

    #[test]
    fn channel_subpath_nests_under_channels_dir() {
        assert_eq!(
            Session::channel_subpath(&live(0, Some("Kick"), true)),
            std::path::PathBuf::from("channels/ch00-Kick.wav")
        );
    }
}
