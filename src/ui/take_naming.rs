use crossterm::event::{KeyCode, KeyEvent};

use crate::app::App;
use crate::ui::Action;
use crate::ui::text_input::TextInput;

/// Why the take-naming overlay is open. Determines cancellation
/// behaviour: T-press marker rollback vs. plain close.
#[derive(Clone, Copy, Debug)]
pub enum TakeOrigin {
    /// T placed a marker; cancel rolls it back.
    Fresh,
    /// N targets an existing marker; cancel just closes.
    Retroactive,
}

/// Inline take-naming overlay. Renders within the recording panel
/// rather than as a centered modal, so it owns no `draw`.
pub struct TakeNaming {
    input: TextInput,
    origin: TakeOrigin,
}

impl TakeNaming {
    pub fn fresh() -> Self {
        Self {
            input: TextInput::new(),
            origin: TakeOrigin::Fresh,
        }
    }

    pub fn retroactive() -> Self {
        Self {
            input: TextInput::new(),
            origin: TakeOrigin::Retroactive,
        }
    }

    pub fn input(&self) -> &TextInput {
        &self.input
    }

    pub fn handle_key(&mut self, key: KeyEvent, app: &mut App) -> Action {
        match key.code {
            KeyCode::Esc => {
                self.cancel(app);
                Action::Close
            }
            KeyCode::Enter => {
                if self.input.value().trim().is_empty() {
                    self.cancel(app);
                } else {
                    app.create_take(self.input.value());
                }
                Action::Close
            }
            _ => {
                self.input.handle_edit_key(key);
                Action::Stay
            }
        }
    }

    fn cancel(&self, app: &mut App) {
        if matches!(self.origin, TakeOrigin::Fresh) {
            app.delete_last_marker();
        }
    }
}
