use std::path::{Path, PathBuf};

use chrono::Local;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::bounce::BounceEvent;
use crate::channel::Channel;
use crate::paths;
use crate::settings::Settings;
use crate::timeline::{BounceStatus, Take, Timeline};
use crate::track::Track;
use crate::units::SampleRate;

const TRACKS_DIR: &str = "tracks";

#[derive(Serialize, Deserialize)]
pub struct Session {
    pub name: String,
    #[serde(skip)]
    pub bounces_dir: PathBuf,
    #[serde(skip)]
    pub bounces_filename_prefix: Option<String>,
    #[serde(skip)]
    dir: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    end_sample: Option<u64>,
    #[serde(flatten)]
    timeline: Timeline,
    #[serde(rename = "track", default)]
    tracks: Vec<Track>,
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
            tracks: Vec::new(),
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

    pub fn tracks(&self) -> &[Track] {
        &self.tracks
    }

    pub fn end_sample(&self) -> Option<u64> {
        self.end_sample
    }

    pub fn has_recording(&self) -> bool {
        !self.tracks.is_empty()
    }

    pub fn recording_track_paths(&self) -> Vec<PathBuf> {
        self.tracks()
            .iter()
            .map(|t| self.dir.join(&t.file))
            .collect()
    }

    fn track_subpath(channel: &Channel) -> PathBuf {
        Path::new(TRACKS_DIR).join(track_filename(channel))
    }

    pub fn last_marker_unbound(&self) -> bool {
        self.timeline.last_marker_unbound()
    }

    pub fn drop_marker(&mut self, sample: u64) {
        self.timeline.mark(sample);
        self.persist();
    }

    pub fn delete_last_marker(&mut self) -> bool {
        let deleted = self.timeline.delete_last_marker();
        if deleted {
            self.persist();
        }
        deleted
    }

    pub fn create_take(&mut self, name: String) -> Option<Take> {
        if !self.timeline.create_take(name) {
            return None;
        }
        self.persist();
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
        self.persist();
    }

    pub fn start_recording(&mut self, armed_channels: &[Channel]) -> Option<Vec<Track>> {
        if armed_channels.is_empty() {
            return None;
        }
        let tracks: Vec<Track> = armed_channels
            .iter()
            .map(|c| Track {
                index: c.index,
                label: c.label.clone(),
                file: Self::track_subpath(c),
            })
            .collect();
        self.tracks = tracks.clone();
        self.end_sample = None;
        self.timeline.mark(0);
        self.persist();
        Some(tracks)
    }

    pub fn stop_recording(&mut self, end_sample: u64) {
        if self.has_recording() {
            self.end_sample = Some(end_sample);
        }
        self.timeline.mark(end_sample);
        self.persist();
    }

    fn persist(&self) {
        if !self.has_recording() {
            return;
        }
        let _ = std::fs::create_dir_all(&self.dir);
        let toml = match toml::to_string_pretty(self) {
            Ok(s) => s,
            Err(_) => return,
        };
        let final_path = self.dir.join("session.toml");
        let tmp = self.dir.join("session.toml.tmp");
        if std::fs::write(&tmp, &toml).is_ok() {
            let _ = std::fs::rename(&tmp, &final_path);
        }
    }
}

fn track_filename(channel: &Channel) -> String {
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
        assert!(session.tracks().is_empty());
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
        let armed = vec![Channel::fixture(0, Some("Kick"), true)];

        let recorded = session.start_recording(&armed).expect("non-empty");

        assert_eq!(recorded.len(), 1);
        assert_eq!(recorded[0].index, 0);
        assert_eq!(recorded[0].label.as_deref(), Some("Kick"));
        assert_eq!(recorded[0].file, PathBuf::from("tracks/ch00-Kick.wav"));

        assert!(session.has_recording());
        assert_eq!(session.tracks().len(), 1);
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
        let armed = vec![Channel::fixture(0, None, true)];
        session.start_recording(&armed).expect("armed");

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
        let armed = vec![Channel::fixture(0, None, true)];
        original.start_recording(&armed).expect("armed");
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
    fn track_filename_uses_label_when_present() {
        assert_eq!(
            track_filename(&Channel::fixture(7, Some("Kick"), true)),
            "ch07-Kick.wav"
        );
    }

    #[test]
    fn track_filename_omits_label_when_blank() {
        assert_eq!(track_filename(&Channel::fixture(3, None, true)), "ch03.wav");
        assert_eq!(
            track_filename(&Channel::fixture(3, Some("   "), true)),
            "ch03.wav"
        );
    }

    #[test]
    fn track_filename_sanitizes_unsafe_label_chars() {
        assert_eq!(
            track_filename(&Channel::fixture(0, Some("kick/snare"), true)),
            "ch00-kick_snare.wav"
        );
    }

    #[test]
    fn track_subpath_nests_under_tracks_dir() {
        assert_eq!(
            Session::track_subpath(&Channel::fixture(0, Some("Kick"), true)),
            std::path::PathBuf::from("tracks/ch00-Kick.wav")
        );
    }

    fn read_session_toml(session: &Session) -> String {
        std::fs::read_to_string(session.dir().join("session.toml")).expect("read session.toml")
    }

    fn populated_session_in(dir: &std::path::Path) -> Session {
        let settings = settings_in(dir);
        let mut session = Session::new(SampleRate(48_000), &settings);
        let armed = vec![Channel::fixture(0, Some("Kick"), true)];
        session.start_recording(&armed).expect("non-empty");
        session
    }

    #[test]
    fn persist_no_op_when_dir_missing() {
        // Pre-recording state: dir doesn't exist yet, persist must not panic.
        let dir = tempdir().unwrap();
        let settings = settings_in(dir.path());
        let mut session = Session::new(SampleRate(48_000), &settings);
        // Trigger a path that calls persist (no dir yet).
        session.stop_recording(0);
        assert!(!session.dir().join("session.toml").exists());
    }

    #[test]
    fn persist_roundtrips_through_toml() {
        let dir = tempdir().unwrap();
        let mut session = populated_session_in(dir.path());
        session.drop_marker(48_000);
        session
            .create_take("Verse".to_string())
            .expect("take created");
        session.stop_recording(96_000);

        let toml_text = read_session_toml(&session);
        let parsed: Session = toml::from_str(&toml_text).expect("roundtrip");

        assert_eq!(parsed.name, session.name);
        assert_eq!(parsed.end_sample(), session.end_sample());
        assert_eq!(parsed.sample_rate(), session.sample_rate());
        assert_eq!(parsed.tracks().len(), 1);
        assert_eq!(parsed.tracks()[0].label.as_deref(), Some("Kick"));
        assert_eq!(parsed.timeline().markers().len(), 3);
        assert_eq!(parsed.timeline().takes().len(), 1);
        assert_eq!(parsed.timeline().takes()[0].name, "Verse");
    }

    #[test]
    fn persist_writes_after_each_mutation() {
        // Drift protection: every public mutation that affects persisted
        // state must rewrite session.toml. New mutations need to be
        // added here or this test fails. We delete the file before each
        // call to prove the call itself recreates it (rather than
        // comparing content, which can revert across mutation pairs).
        let dir = tempdir().unwrap();
        let mut session = populated_session_in(dir.path());
        let toml = session.dir().join("session.toml");
        assert!(toml.exists(), "start_recording should write");

        std::fs::remove_file(&toml).unwrap();
        session.drop_marker(48_000);
        assert!(toml.exists(), "drop_marker should write");

        std::fs::remove_file(&toml).unwrap();
        session.create_take("Verse".into()).expect("take");
        assert!(toml.exists(), "create_take should write");

        let take_id = session.timeline().takes()[0].id;

        std::fs::remove_file(&toml).unwrap();
        session.apply_bounce_event(take_id, BounceEvent::Started);
        assert!(toml.exists(), "apply_bounce_event(Started) should write");

        std::fs::remove_file(&toml).unwrap();
        session.apply_bounce_event(take_id, BounceEvent::Done(PathBuf::from("/tmp/v.mp3")));
        assert!(toml.exists(), "apply_bounce_event(Done) should write");

        // Need an unbound marker before delete_last_marker can act.
        session.drop_marker(72_000);
        std::fs::remove_file(&toml).unwrap();
        session.delete_last_marker();
        assert!(toml.exists(), "delete_last_marker should write");

        std::fs::remove_file(&toml).unwrap();
        session.stop_recording(96_000);
        assert!(toml.exists(), "stop_recording should write");
    }
}
