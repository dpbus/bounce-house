use cpal::traits::{DeviceTrait, HostTrait};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, List, ListItem};

use crate::ui::Action;

pub struct DeviceSelectState {
    pub devices: Vec<cpal::Device>,
    pub names: Vec<String>,
    pub cursor: usize,
}

impl DeviceSelectState {
    pub fn new() -> Self {
        let host = cpal::default_host();
        let devices: Vec<cpal::Device> = host
            .input_devices()
            .expect("Failed to get input devices")
            .collect();

        let names: Vec<String> = devices
            .iter()
            .map(|d| d.name().unwrap_or_else(|_| "Unknown".to_string()))
            .collect();

        DeviceSelectState {
            devices,
            names,
            cursor: 0,
        }
    }

    pub fn selected_device(&self) -> &cpal::Device {
        &self.devices[self.cursor]
    }
}

pub fn draw(frame: &mut Frame, state: &DeviceSelectState) {
    let items: Vec<ListItem> = state
        .names
        .iter()
        .enumerate()
        .map(|(i, name)| {
            let style = if i == state.cursor {
                Style::default().fg(Color::Black).bg(Color::Cyan)
            } else {
                Style::default()
            };
            ListItem::new(name.as_str()).style(style)
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
            if state.cursor < state.names.len().saturating_sub(1) {
                state.cursor += 1;
            }
            Action::None
        }
        KeyCode::Enter => Action::NextScreen,
        _ => Action::None,
    }
}
