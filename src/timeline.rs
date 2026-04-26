#[derive(Clone, Copy, Debug)]
pub struct Marker {
    pub tick: u64,
}

#[derive(Clone, Debug)]
pub struct Take {
    pub name: String,
    pub start_tick: u64,
    pub end_tick: u64,
    pub color_index: u8,
}

#[derive(Default)]
pub struct Timeline {
    markers: Vec<Marker>,
    takes: Vec<Take>,
    next_color: u8,
}

impl Timeline {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn markers(&self) -> &[Marker] { &self.markers }
    pub fn takes(&self) -> &[Take] { &self.takes }

    pub fn mark(&mut self, tick: u64) {
        self.markers.push(Marker { tick });
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
            name,
            start_tick: second_last.tick,
            end_tick: last.tick,
            color_index: self.next_color,
        };
        self.next_color = self.next_color.wrapping_add(1);
        self.takes.push(take);
        true
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
