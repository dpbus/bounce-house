use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// A single-line text buffer with cursor position. Cursor is a byte index
/// kept on a UTF-8 char boundary at all times.
#[derive(Debug, Clone, Default)]
pub struct TextInput {
    value: String,
    cursor: usize,
}

impl TextInput {
    pub fn new() -> Self {
        Self::default()
    }

    /// Cursor is placed at the end so the user types after the prefilled value.
    pub fn with_value(initial: &str) -> Self {
        let value = initial.to_string();
        let cursor = value.len();
        Self { value, cursor }
    }

    pub fn value(&self) -> &str {
        &self.value
    }

    pub fn insert_char(&mut self, c: char) {
        self.value.insert(self.cursor, c);
        self.cursor += c.len_utf8();
    }

    pub fn backspace(&mut self) {
        if let Some((idx, _)) = self.value[..self.cursor].char_indices().next_back() {
            self.value.replace_range(idx..self.cursor, "");
            self.cursor = idx;
        }
    }

    pub fn move_left(&mut self) {
        if let Some((idx, _)) = self.value[..self.cursor].char_indices().next_back() {
            self.cursor = idx;
        }
    }

    pub fn move_right(&mut self) {
        if let Some(ch) = self.value[self.cursor..].chars().next() {
            self.cursor += ch.len_utf8();
        }
    }

    pub fn move_home(&mut self) {
        self.cursor = 0;
    }

    pub fn move_end(&mut self) {
        self.cursor = self.value.len();
    }

    /// Splits the value into `(before, at, after)` for cursor-aware
    /// rendering. `at` is the single char under the cursor or `""` when
    /// the cursor sits past the last char.
    pub fn split(&self) -> (&str, &str, &str) {
        match self.value[self.cursor..].chars().next() {
            Some(ch) => {
                let end = self.cursor + ch.len_utf8();
                (
                    &self.value[..self.cursor],
                    &self.value[self.cursor..end],
                    &self.value[end..],
                )
            }
            None => (&self.value, "", ""),
        }
    }

    /// Routes an edit key (Char, Backspace, Left/Right/Home/End) to the
    /// appropriate mutation. Caller owns control-flow keys (Esc, Enter, Tab).
    pub fn handle_edit_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.insert_char(c)
            }
            KeyCode::Backspace => self.backspace(),
            KeyCode::Left => self.move_left(),
            KeyCode::Right => self.move_right(),
            KeyCode::Home => self.move_home(),
            KeyCode::End => self.move_end(),
            _ => {}
        }
    }
}
