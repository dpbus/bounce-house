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
    creating: Option<CreatingTake>,
    next_color: u8,
}

enum CreatingTake {
    Fresh(String),
    Retroactive(String),
}

impl CreatingTake {
    fn buf(&self) -> &str {
        match self {
            Self::Fresh(b) | Self::Retroactive(b) => b,
        }
    }
    fn buf_mut(&mut self) -> &mut String {
        match self {
            Self::Fresh(b) | Self::Retroactive(b) => b,
        }
    }
}

impl Timeline {
    pub fn new() -> Self {
        Self {
            markers: Vec::new(),
            takes: Vec::new(),
            creating: None,
            next_color: 0,
        }
    }

    pub fn markers(&self) -> &[Marker] { &self.markers }
    pub fn takes(&self) -> &[Take] { &self.takes }
    pub fn is_naming_take(&self) -> bool { self.creating.is_some() }
    pub fn take_name_buf(&self) -> Option<&str> {
        self.creating.as_ref().map(|c| c.buf())
    }

    pub fn mark(&mut self, tick: u64) {
        if self.creating.is_some() { return; }
        self.markers.push(Marker { tick });
    }

    pub fn mark_and_name(&mut self, tick: u64) {
        if self.creating.is_some() { return; }
        self.markers.push(Marker { tick });
        self.creating = Some(CreatingTake::Fresh(String::new()));
    }

    pub fn name_take(&mut self) {
        if !self.last_marker_unbound() { return; }
        self.creating = Some(CreatingTake::Retroactive(String::new()));
    }

    /// Whether the last marker exists and isn't part of any take. Shared
    /// precondition for delete and retroactive name — both act on the
    /// literal last marker.
    pub fn last_marker_unbound(&self) -> bool {
        if self.creating.is_some() || self.markers.len() <= 1 {
            return false;
        }
        let last = self.markers.last().unwrap();
        !self.takes.iter().any(|t| t.start_tick == last.tick || t.end_tick == last.tick)
    }

    /// Pop the last marker if it isn't part of any take. The recording-start
    /// marker is protected.
    pub fn delete_last_marker(&mut self) -> bool {
        if !self.last_marker_unbound() {
            return false;
        }
        self.markers.pop();
        true
    }

    pub fn cancel(&mut self) {
        if let Some(CreatingTake::Fresh(_)) = self.creating.take() {
            self.markers.pop();
        }
    }

    pub fn commit(&mut self) {
        let trimmed = match &self.creating {
            Some(c) => c.buf().trim().to_string(),
            None => return,
        };
        if trimmed.is_empty() {
            self.cancel();
            return;
        }
        self.creating = None;

        // Both Fresh (T just placed it) and Retroactive (only opened if
        // unbound) target the literal last marker.
        let [.., second_last, last] = self.markers.as_slice() else { return; };
        let take = Take {
            name: trimmed,
            start_tick: second_last.tick,
            end_tick: last.tick,
            color_index: self.next_color,
        };
        self.next_color = self.next_color.wrapping_add(1);
        self.takes.push(take);
    }

    pub fn append_char(&mut self, c: char) {
        if let Some(creating) = &mut self.creating {
            creating.buf_mut().push(c);
        }
    }

    pub fn backspace(&mut self) {
        if let Some(creating) = &mut self.creating {
            creating.buf_mut().pop();
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
