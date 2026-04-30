use std::path::PathBuf;

use crate::units::SampleRate;

#[derive(Clone, Copy, Debug)]
pub struct Marker {
    pub sample: u64,
}

/// Lifecycle of a bounce job. The `Done`/`Failed` payloads aren't read
/// yet, but they exist so the UI can surface the file path or error
/// message later.
#[derive(Clone, Debug)]
#[allow(dead_code)]
pub enum BounceStatus {
    Pending,
    Bouncing,
    Done(PathBuf),
    Failed(String),
}

#[derive(Clone, Debug)]
pub struct Take {
    pub id: u32,
    pub name: String,
    pub start_sample: u64,
    pub end_sample: u64,
    pub color_index: u8,
    pub bounce_status: BounceStatus,
}

/// Marker/take structure laid down against a recording, in
/// recording-relative samples. Owns the sample rate and answers all
/// time-domain questions about the session's events.
pub struct Timeline {
    sample_rate: SampleRate,
    markers: Vec<Marker>,
    takes: Vec<Take>,
    next_take_id: u32,
    next_color: u8,
}

impl Timeline {
    pub fn new(sample_rate: SampleRate) -> Self {
        Self {
            sample_rate,
            markers: Vec::new(),
            takes: Vec::new(),
            next_take_id: 0,
            next_color: 0,
        }
    }

    pub fn sample_rate(&self) -> SampleRate {
        self.sample_rate
    }

    pub fn markers(&self) -> &[Marker] {
        &self.markers
    }

    pub fn takes(&self) -> &[Take] {
        &self.takes
    }

    pub fn secs_at(&self, sample: u64) -> u64 {
        sample / (self.sample_rate.0 as u64).max(1)
    }

    pub fn duration_secs(&self, start_sample: u64, end_sample: u64) -> u64 {
        self.secs_at(end_sample.saturating_sub(start_sample))
    }

    pub fn since_last_marker_secs(&self, current_rel_sample: u64) -> u64 {
        let last = self.markers.last().map(|m| m.sample).unwrap_or(0);
        self.secs_at(current_rel_sample.saturating_sub(last))
    }

    /// Color index the next take will be assigned.
    pub fn next_take_color(&self) -> u8 {
        self.next_color
    }

    pub fn mark(&mut self, sample: u64) {
        self.markers.push(Marker { sample });
    }

    /// Whether the last marker exists and isn't part of any take. Shared
    /// precondition for delete and retroactive name — both act on the
    /// literal last marker.
    pub fn last_marker_unbound(&self) -> bool {
        if self.markers.len() <= 1 {
            return false;
        }
        let last = self.markers.last().unwrap();
        !self
            .takes
            .iter()
            .any(|t| t.start_sample == last.sample || t.end_sample == last.sample)
    }

    pub fn delete_last_marker(&mut self) -> bool {
        if !self.last_marker_unbound() {
            return false;
        }
        self.markers.pop();
        true
    }

    /// Create a take spanning the last two markers, if the last is
    /// unbound. Returns true on success.
    pub fn create_take(&mut self, name: String) -> bool {
        if !self.last_marker_unbound() {
            return false;
        }
        let [.., second_last, last] = self.markers.as_slice() else {
            return false;
        };
        let take = Take {
            id: self.next_take_id,
            name,
            start_sample: second_last.sample,
            end_sample: last.sample,
            color_index: self.next_color,
            bounce_status: BounceStatus::Pending,
        };
        self.next_color = self.next_color.wrapping_add(1);
        self.next_take_id = self.next_take_id.wrapping_add(1);
        self.takes.push(take);
        true
    }

    pub fn set_bounce_status(&mut self, take_id: u32, status: BounceStatus) -> bool {
        if let Some(take) = self.takes.iter_mut().find(|t| t.id == take_id) {
            take.bounce_status = status;
            true
        } else {
            false
        }
    }

    /// Color for a marker at `sample` — the bounding take's color if any.
    /// Prefers the take that ends here over one that starts here.
    pub fn marker_color_index(&self, sample: u64) -> Option<u8> {
        self.takes
            .iter()
            .find(|t| t.end_sample == sample)
            .or_else(|| self.takes.iter().find(|t| t.start_sample == sample))
            .map(|t| t.color_index)
    }

    pub fn is_marker_bound(&self, sample: u64) -> bool {
        self.marker_color_index(sample).is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn timeline() -> Timeline {
        Timeline::new(SampleRate(48_000))
    }

    #[test]
    fn new_starts_empty() {
        let t = timeline();
        assert!(t.markers().is_empty());
        assert!(t.takes().is_empty());
        assert_eq!(t.next_take_color(), 0);
    }

    #[test]
    fn mark_appends_in_order() {
        let mut t = timeline();
        t.mark(0);
        t.mark(48_000);
        t.mark(96_000);
        let samples: Vec<u64> = t.markers().iter().map(|m| m.sample).collect();
        assert_eq!(samples, vec![0, 48_000, 96_000]);
    }

    #[test]
    fn last_marker_unbound_needs_at_least_two_markers() {
        let mut t = timeline();
        t.mark(0);
        // Single marker — not unbound (no preceding marker to span from).
        assert!(!t.last_marker_unbound());
        t.mark(48_000);
        assert!(t.last_marker_unbound());
    }

    #[test]
    fn last_marker_unbound_false_after_take_consumes_it() {
        let mut t = timeline();
        t.mark(0);
        t.mark(48_000);
        t.create_take("v1".into());
        // Take consumed the trailing marker as its end.
        assert!(!t.last_marker_unbound());
    }

    #[test]
    fn delete_last_marker_only_removes_unbound() {
        let mut t = timeline();
        t.mark(0);
        t.mark(48_000);
        assert!(t.delete_last_marker());
        assert_eq!(t.markers().len(), 1);

        t.mark(96_000);
        t.create_take("v1".into());
        // Last marker is now bound to a take — refuse delete.
        assert!(!t.delete_last_marker());
        assert_eq!(t.markers().len(), 2);
    }

    #[test]
    fn create_take_uses_last_two_markers() {
        let mut t = timeline();
        t.mark(0);
        t.mark(48_000);
        t.mark(96_000);
        assert!(t.create_take("verse".into()));
        let take = &t.takes()[0];
        assert_eq!(take.start_sample, 48_000);
        assert_eq!(take.end_sample, 96_000);
        assert_eq!(take.name, "verse");
    }

    #[test]
    fn create_take_rejects_when_last_marker_bound() {
        let mut t = timeline();
        t.mark(0);
        t.mark(48_000);
        assert!(t.create_take("first".into()));
        // Now the last marker is bound; another create_take should fail
        // until a new marker is dropped.
        assert!(!t.create_take("second".into()));
    }

    #[test]
    fn create_take_increments_color_index_and_id() {
        let mut t = timeline();
        t.mark(0);
        t.mark(48_000);
        t.create_take("a".into());
        t.mark(96_000);
        t.create_take("b".into());

        let takes = t.takes();
        assert_eq!(takes[0].id, 0);
        assert_eq!(takes[0].color_index, 0);
        assert_eq!(takes[1].id, 1);
        assert_eq!(takes[1].color_index, 1);
        assert_eq!(t.next_take_color(), 2);
    }

    #[test]
    fn marker_color_index_finds_take_at_endpoint() {
        let mut t = timeline();
        t.mark(0);
        t.mark(48_000);
        t.create_take("v1".into());

        // Sample at end of take returns its color.
        assert_eq!(t.marker_color_index(48_000), Some(0));
        assert_eq!(t.marker_color_index(0), Some(0));
        // Random sample — not bound.
        assert_eq!(t.marker_color_index(12345), None);
    }

    #[test]
    fn marker_color_index_prefers_end_over_start() {
        let mut t = timeline();
        t.mark(0);
        t.mark(48_000); // end of take 0
        t.create_take("a".into());
        // Add another take that starts where the previous ended.
        t.mark(96_000);
        t.create_take("b".into());
        // Sample 48_000 is BOTH end of take 0 AND start of take 1.
        // Should prefer the end (take 0 → color 0).
        assert_eq!(t.marker_color_index(48_000), Some(0));
    }

    #[test]
    fn is_marker_bound_mirrors_marker_color_index() {
        let mut t = timeline();
        t.mark(0);
        t.mark(48_000);
        t.create_take("v1".into());
        assert!(t.is_marker_bound(48_000));
        assert!(!t.is_marker_bound(99_999));
    }

    #[test]
    fn set_bounce_status_updates_existing_take() {
        let mut t = timeline();
        t.mark(0);
        t.mark(48_000);
        t.create_take("v1".into());
        let id = t.takes()[0].id;
        let path = std::path::PathBuf::from("/tmp/v1.mp3");

        assert!(t.set_bounce_status(id, BounceStatus::Done(path.clone())));
        assert!(matches!(t.takes()[0].bounce_status, BounceStatus::Done(_)));
    }

    #[test]
    fn set_bounce_status_returns_false_for_unknown_take() {
        let mut t = timeline();
        assert!(!t.set_bounce_status(999, BounceStatus::Pending));
    }

    #[test]
    fn secs_at_divides_by_sample_rate() {
        let t = timeline();
        assert_eq!(t.secs_at(0), 0);
        assert_eq!(t.secs_at(48_000), 1);
        assert_eq!(t.secs_at(96_000), 2);
        // Truncates toward zero (integer division).
        assert_eq!(t.secs_at(47_999), 0);
    }

    #[test]
    fn secs_at_handles_zero_sample_rate() {
        let t = Timeline::new(SampleRate(0));
        // Doesn't panic; treats rate as 1 so the value passes through.
        assert_eq!(t.secs_at(48_000), 48_000);
    }

    #[test]
    fn duration_secs_computes_span() {
        let t = timeline();
        assert_eq!(t.duration_secs(48_000, 96_000), 1);
        assert_eq!(t.duration_secs(0, 240_000), 5);
    }

    #[test]
    fn duration_secs_saturates_inverted_range_to_zero() {
        let t = timeline();
        assert_eq!(t.duration_secs(96_000, 48_000), 0);
    }

    #[test]
    fn since_last_marker_secs_uses_trailing_marker_as_anchor() {
        let mut t = timeline();
        t.mark(0);
        t.mark(96_000); // 2 seconds in
        // Current rel sample 144_000 (3s) — 1 sec since last marker.
        assert_eq!(t.since_last_marker_secs(144_000), 1);
    }

    #[test]
    fn since_last_marker_secs_zero_when_no_markers() {
        let t = timeline();
        // No markers — last is treated as 0; current_rel 0 → 0 secs.
        assert_eq!(t.since_last_marker_secs(0), 0);
        // Even with no markers, current_rel still measures from 0.
        assert_eq!(t.since_last_marker_secs(48_000), 1);
    }

    #[test]
    fn since_last_marker_secs_saturates_when_current_before_last_marker() {
        let mut t = timeline();
        t.mark(0);
        t.mark(96_000);
        // Querying with a current rel before the last marker
        // (shouldn't happen in practice) saturates rather than panicking.
        assert_eq!(t.since_last_marker_secs(48_000), 0);
    }
}
