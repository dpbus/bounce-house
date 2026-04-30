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
/// time-domain questions about the project's events.
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
