use crossterm::event::{KeyCode, KeyEvent};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, List, ListItem};

use crate::ui::Action;
use crate::audio_interface::AudioInterface;
use crate::level_monitor::LevelMonitor;

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

fn to_db(level: f32) -> f32 {
    if level < 0.0001 {
        -80.0
    } else {
        20.0 * level.log10()
    }
}

fn meter_spans(level: f32, width: usize) -> Vec<Span<'static>> {
    // Map dB range (-60..0) to (0..1) for display
    let db = to_db(level);
    let normalized = ((db + 60.0) / 60.0).clamp(0.0, 1.0);
    let filled = (normalized * width as f32).ceil().min(width as f32) as usize;

    let bar_color = if db > -1.0 {
        Color::Red
    } else if db > -6.0 {
        Color::Yellow
    } else if db > -60.0 {
        Color::Green
    } else {
        Color::DarkGray
    };

    let filled_str: String = "█".repeat(filled);
    let empty_str: String = " ".repeat(width.saturating_sub(filled));

    vec![
        Span::raw("│"),
        Span::styled(filled_str, Style::default().fg(bar_color)),
        Span::raw(empty_str),
        Span::raw("│"),
    ]
}

pub fn draw(frame: &mut Frame, state: &ChannelSelectState) {
    let levels = state.level_monitor.levels();

    let items: Vec<ListItem> = state
        .selected
        .iter()
        .enumerate()
        .map(|(i, &on)| {
            let marker = if on { "[x]" } else { "[ ]" };
            let level = levels.get(i).copied().unwrap_or(0.0);

            let row_style = if i == state.cursor {
                Style::default().fg(Color::White).bg(Color::DarkGray)
            } else {
                Style::default()
            };

            let mut spans = vec![Span::styled(format!("{} Ch {:>2}  ", marker, i), row_style)];
            spans.extend(meter_spans(level, 20));

            ListItem::new(Line::from(spans))
        })
        .collect();

    let title = format!(
        " {} — Select Channels (Space to toggle, Enter to record) ",
        state.interface.name()
    );
    let list = List::new(items).block(
        Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan)),
    );
    frame.render_widget(list, frame.area());
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
