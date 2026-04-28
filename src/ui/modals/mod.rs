mod channel_picker;
mod load_template;
mod save_template;
mod settings;

use crossterm::event::KeyEvent;
use ratatui::Frame;

use crate::app::App;
use crate::ui::Action;
use crate::ui::view::View;

pub use channel_picker::ChannelPickerModal;
pub use load_template::LoadTemplateModal;
pub use save_template::SaveTemplateModal;
pub use settings::SettingsModal;

/// One of the centered modal overlays the user can open. View holds at
/// most one at a time. Each variant carries the modal's own state —
/// buffers, cursors, lists.
pub enum ActiveModal {
    SaveTemplate(SaveTemplateModal),
    LoadTemplate(LoadTemplateModal),
    ChannelPicker(ChannelPickerModal),
    Settings(SettingsModal),
}

impl ActiveModal {
    pub fn handle_key(&mut self, key: KeyEvent, app: &mut App, view: &mut View) -> Action {
        match self {
            ActiveModal::SaveTemplate(m) => m.handle_key(key, app, view),
            ActiveModal::LoadTemplate(m) => m.handle_key(key, app, view),
            ActiveModal::ChannelPicker(m) => m.handle_key(key, app, view),
            ActiveModal::Settings(m) => m.handle_key(key, app, view),
        }
    }

    pub fn draw(&self, frame: &mut Frame, app: &App) {
        match self {
            ActiveModal::SaveTemplate(m) => m.draw(frame, app),
            ActiveModal::LoadTemplate(m) => m.draw(frame, app),
            ActiveModal::ChannelPicker(m) => m.draw(frame, app),
            ActiveModal::Settings(m) => m.draw(frame, app),
        }
    }
}
