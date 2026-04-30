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

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn ctrl(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::CONTROL)
    }

    #[test]
    fn new_is_empty_with_cursor_at_zero() {
        let t = TextInput::new();
        assert_eq!(t.value(), "");
        assert_eq!(t.split(), ("", "", ""));
    }

    #[test]
    fn with_value_places_cursor_at_end() {
        let t = TextInput::with_value("hello");
        assert_eq!(t.value(), "hello");
        assert_eq!(t.split(), ("hello", "", ""));
    }

    #[test]
    fn insert_char_appends_when_at_end() {
        let mut t = TextInput::new();
        t.insert_char('a');
        t.insert_char('b');
        assert_eq!(t.value(), "ab");
        assert_eq!(t.split(), ("ab", "", ""));
    }

    #[test]
    fn insert_char_inserts_mid_string() {
        let mut t = TextInput::with_value("ac");
        t.move_left();
        t.insert_char('b');
        assert_eq!(t.value(), "abc");
        // Cursor between b and c.
        assert_eq!(t.split(), ("ab", "c", ""));
    }

    #[test]
    fn backspace_removes_char_before_cursor() {
        let mut t = TextInput::with_value("hello");
        t.backspace();
        assert_eq!(t.value(), "hell");
        t.backspace();
        t.backspace();
        assert_eq!(t.value(), "he");
    }

    #[test]
    fn backspace_at_start_is_noop() {
        let mut t = TextInput::with_value("abc");
        t.move_home();
        t.backspace();
        assert_eq!(t.value(), "abc");
    }

    #[test]
    fn backspace_handles_multibyte_char() {
        let mut t = TextInput::with_value("café");
        t.backspace();
        assert_eq!(t.value(), "caf");
    }

    #[test]
    fn move_left_and_right_traverse_chars() {
        let mut t = TextInput::with_value("abc");
        t.move_home();
        assert_eq!(t.split(), ("", "a", "bc"));
        t.move_right();
        assert_eq!(t.split(), ("a", "b", "c"));
        t.move_right();
        assert_eq!(t.split(), ("ab", "c", ""));
        t.move_right();
        // Past end — stays at end.
        assert_eq!(t.split(), ("abc", "", ""));
        t.move_left();
        assert_eq!(t.split(), ("ab", "c", ""));
    }

    #[test]
    fn move_left_at_start_is_noop() {
        let mut t = TextInput::with_value("a");
        t.move_home();
        t.move_left();
        assert_eq!(t.split(), ("", "a", ""));
    }

    #[test]
    fn move_home_and_end_jump_to_extremes() {
        let mut t = TextInput::with_value("abc");
        t.move_home();
        assert_eq!(t.split(), ("", "a", "bc"));
        t.move_end();
        assert_eq!(t.split(), ("abc", "", ""));
    }

    #[test]
    fn split_keeps_cursor_on_char_boundary_for_unicode() {
        let mut t = TextInput::with_value("café");
        t.move_home();
        t.move_right(); // c
        t.move_right(); // a
        t.move_right(); // f
        let (before, at, after) = t.split();
        assert_eq!(before, "caf");
        assert_eq!(at, "é");
        assert_eq!(after, "");
    }

    #[test]
    fn handle_edit_key_routes_typing_and_editing() {
        let mut t = TextInput::new();
        t.handle_edit_key(key(KeyCode::Char('h')));
        t.handle_edit_key(key(KeyCode::Char('i')));
        t.handle_edit_key(key(KeyCode::Backspace));
        t.handle_edit_key(key(KeyCode::Char('!')));
        assert_eq!(t.value(), "h!");
    }

    #[test]
    fn handle_edit_key_ignores_control_modified_chars() {
        // Ctrl+A shouldn't insert an 'a'.
        let mut t = TextInput::new();
        t.handle_edit_key(ctrl(KeyCode::Char('a')));
        assert_eq!(t.value(), "");
    }

    #[test]
    fn handle_edit_key_supports_arrow_keys() {
        let mut t = TextInput::with_value("abc");
        t.handle_edit_key(key(KeyCode::Left));
        t.handle_edit_key(key(KeyCode::Char('Z')));
        assert_eq!(t.value(), "abZc");
    }

    #[test]
    fn handle_edit_key_ignores_unknown_keys() {
        let mut t = TextInput::with_value("abc");
        t.handle_edit_key(key(KeyCode::F(5)));
        t.handle_edit_key(key(KeyCode::Esc));
        t.handle_edit_key(key(KeyCode::Enter));
        // Untouched.
        assert_eq!(t.value(), "abc");
    }
}
