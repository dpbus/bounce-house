use std::path::PathBuf;

/// `sample` is in samples since recording start (not engine start).
#[derive(Clone, Copy, Debug)]
pub struct Marker {
    pub tick: u64,
    pub sample: u64,
}

#[derive(Clone, Debug)]
pub enum BounceStatus {
    Pending,
    Bouncing,
    Done(PathBuf),
    Failed(String),
}

/// `start_sample`/`end_sample` are in samples since recording start.
#[derive(Clone, Debug)]
pub struct Take {
    pub id: u32,
    pub name: String,
    pub start_tick: u64,
    pub end_tick: u64,
    pub start_sample: u64,
    pub end_sample: u64,
    pub color_index: u8,
    pub bounce_status: BounceStatus,
}

#[derive(Default)]
pub struct Timeline {
    markers: Vec<Marker>,
    takes: Vec<Take>,
    next_take_id: u32,
    next_color: u8,
}

impl Timeline {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn markers(&self) -> &[Marker] { &self.markers }
    pub fn takes(&self) -> &[Take] { &self.takes }

    pub fn mark(&mut self, tick: u64, sample: u64) {
        self.markers.push(Marker { tick, sample });
    }

    /// Whether the last marker exists and isn't part of any take. Shared
    /// precondition for delete and retroactive name — both act on the
    /// literal last marker.
    pub fn last_marker_unbound(&self) -> bool {
        if self.markers.len() <= 1 {
            return false;
        }
        let last = self.markers.last().unwrap();
        !self.takes.iter().any(|t| t.start_tick == last.tick || t.end_tick == last.tick)
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
        let [.., second_last, last] = self.markers.as_slice() else { return false; };
        let take = Take {
            id: self.next_take_id,
            name,
            start_tick: second_last.tick,
            end_tick: last.tick,
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

    /// Color for a marker at `tick` — the bounding take's color if any.
    /// Prefers the take that ends here over one that starts here.
    pub fn marker_color_index(&self, tick: u64) -> Option<u8> {
        self.takes.iter()
            .find(|t| t.end_tick == tick)
            .or_else(|| self.takes.iter().find(|t| t.start_tick == tick))
            .map(|t| t.color_index)
    }
}
