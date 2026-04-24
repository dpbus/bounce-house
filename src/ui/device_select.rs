use crossterm::event::{KeyCode, KeyEvent};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, List, ListItem};

use crate::ui::Action;
use crate::audio_interface::AudioInterface;

pub struct DeviceSelectState {
    pub interfaces: Vec<AudioInterface>,
    pub cursor: usize,
}

impl DeviceSelectState {
    pub fn new() -> Self {
        let interfaces = AudioInterface::list();

        DeviceSelectState {
            interfaces,
            cursor: 0,
        }
    }

    pub fn take_selected(mut self) -> AudioInterface {
        self.interfaces.swap_remove(self.cursor)
    }
}

pub fn draw(frame: &mut Frame, state: &DeviceSelectState) {
    let items: Vec<ListItem> = state
        .interfaces
        .iter()
        .enumerate()
        .map(|(i, interface)| {
            let style = if i == state.cursor {
                Style::default().fg(Color::Black).bg(Color::Cyan)
            } else {
                Style::default()
            };
            ListItem::new(interface.name()).style(style)
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .title(" Select Audio Device ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan)),
    );

    frame.render_widget(list, frame.area());
}

pub fn handle_input(state: &mut DeviceSelectState, key: KeyEvent) -> Action {
    match key.code {
        KeyCode::Char('q') | KeyCode::Esc => Action::Quit,
        KeyCode::Up | KeyCode::Char('k') => {
            if state.cursor > 0 {
                state.cursor -= 1;
            }
            Action::None
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if state.cursor < state.interfaces.len().saturating_sub(1) {
                state.cursor += 1;
            }
            Action::None
        }
        KeyCode::Enter => Action::NextScreen,
        _ => Action::None,
    }
}
