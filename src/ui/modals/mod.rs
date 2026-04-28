mod save_template;

use crossterm::event::KeyEvent;
use ratatui::Frame;

use crate::app::App;
use crate::ui::view::View;

pub use save_template::SaveTemplateModal;

/// One of the modal overlays the user can open. View holds at most one
/// at a time. Each variant carries the modal's own state — buffers,
/// cursors, lists — so AppState stays focused on domain state.
pub enum ActiveModal {
    SaveTemplate(SaveTemplateModal),
}

/// What a modal asks the runtime to do after a key press. Domain
/// mutations happen inside `handle_key` via direct calls to `&mut App`
/// methods; UI feedback (flashes, etc.) happens via `&mut View`. This
/// enum just controls the modal's lifecycle.
pub enum Action {
    Stay,
    Close,
}

impl ActiveModal {
    pub fn handle_key(&mut self, key: KeyEvent, app: &mut App, view: &mut View) -> Action {
        match self {
            ActiveModal::SaveTemplate(m) => m.handle_key(key, app, view),
        }
    }

    pub fn draw(&self, frame: &mut Frame, app: &App) {
        match self {
            ActiveModal::SaveTemplate(m) => m.draw(frame, app),
        }
    }
}
