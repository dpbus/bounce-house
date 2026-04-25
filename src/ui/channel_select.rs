use crossterm::event::{KeyCode, KeyEvent};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, List, ListItem};

use crate::ui::Action;
use crate::ui::widgets::horizontal_meter;
use crate::audio_interface::AudioInterface;
use crate::level_monitor::LevelMonitor;

const METER_WIDTH: usize = 50;

pub struct ChannelSelectState {
    pub interface: AudioInterface,
    pub selected: Vec<bool>,
    pub cursor: usize,
    level_monitor: LevelMonitor,
}

impl ChannelSelectState {
    pub fn new(interface: AudioInterface) -> Self {
        let num_channels = interface.channel_count();
        let level_monitor = LevelMonitor::new(&interface);
        ChannelSelectState {
            interface,
            selected: vec![false; num_channels],
            cursor: 0,
            level_monitor,
        }
    }

    pub fn selected_channels(&self) -> Vec<u8> {
        self.selected
            .iter()
            .enumerate()
            .filter(|&(_, &on)| on)
            .map(|(i, _)| i as u8)
            .collect()
    }
}

pub fn draw(frame: &mut Frame, state: &ChannelSelectState) {
    let levels = state.level_monitor.levels();

    let items: Vec<ListItem> = state
        .selected
        .iter()
        .enumerate()
        .map(|(i, &on)| {
            let level = levels.get(i).map(|cl| cl.current()).unwrap_or(0.0);
            channel_row(i, on, i == state.cursor, level)
        })
        .collect();

    let list = List::new(items).block(channel_select_block(state));
    frame.render_widget(list, frame.area());
}

fn channel_row(idx: usize, selected: bool, focused: bool, level: f32) -> ListItem<'static> {
    let marker = if selected { "[x]" } else { "[ ]" };
    let row_style = if focused {
        Style::default().fg(Color::White).bg(Color::DarkGray)
    } else {
        Style::default()
    };
    let mut spans = vec![Span::styled(format!("{} Ch {:>2}  ", marker, idx), row_style)];
    spans.extend(horizontal_meter(level, None, METER_WIDTH));
    ListItem::new(Line::from(spans))
}

fn channel_select_block(state: &ChannelSelectState) -> Block<'static> {
    let title = format!(
        " {} — Select Channels (Space to toggle, Enter to record) ",
        state.interface.name()
    );
    Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
}

pub fn handle_input(state: &mut ChannelSelectState, key: KeyEvent) -> Action {
    match key.code {
        KeyCode::Char('q') | KeyCode::Esc => Action::Quit,
        KeyCode::Up | KeyCode::Char('k') => {
            if state.cursor > 0 {
                state.cursor -= 1;
            }
            Action::None
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if state.cursor < state.selected.len().saturating_sub(1) {
                state.cursor += 1;
            }
            Action::None
        }
        KeyCode::Char(' ') => {
            state.selected[state.cursor] = !state.selected[state.cursor];
            Action::None
        }
        KeyCode::Char('a') => {
            let all_selected = state.selected.iter().all(|&s| s);
            for s in &mut state.selected {
                *s = !all_selected;
            }
            Action::None
        }
        KeyCode::Enter => {
            if state.selected.iter().any(|&s| s) {
                Action::NextScreen
            } else {
                Action::None
            }
        }
        _ => Action::None,
    }
}
