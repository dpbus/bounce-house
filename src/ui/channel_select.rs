use std::sync::{Arc, Mutex};

use cpal::Stream;
use cpal::traits::{DeviceTrait, StreamTrait};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, List, ListItem};

use crate::ui::Action;

pub struct ChannelSelectState {
    pub device_name: String,
    pub num_channels: u16,
    pub selected: Vec<bool>,
    pub cursor: usize,
    levels: Arc<Mutex<Vec<f32>>>,
    _stream: Stream,
}

impl ChannelSelectState {
    pub fn new(device: &cpal::Device) -> Self {
        let device_name = device.name().unwrap_or_else(|_| "Unknown".to_string());
        let config = device
            .default_input_config()
            .expect("No default input config");
        let num_channels = config.channels();
        let selected = vec![false; num_channels as usize];

        let levels = Arc::new(Mutex::new(vec![0.0f32; num_channels as usize]));
        let levels_clone = levels.clone();
        let nc = num_channels as usize;
        let decay = 0.75f32;

        let stream = device
            .build_input_stream(
                &config.into(),
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    let mut peaks = vec![0.0f32; nc];
                    for frame in 0..data.len() / nc {
                        for ch in 0..nc {
                            let sample = data[frame * nc + ch].abs();
                            if sample > peaks[ch] {
                                peaks[ch] = sample;
                            }
                        }
                    }
                    if let Ok(mut lvl) = levels_clone.lock() {
                        for ch in 0..nc {
                            lvl[ch] = peaks[ch].max(lvl[ch] * decay);
                        }
                    }
                },
                |err| {
                    eprintln!("Preview stream error: {}", err);
                },
                None,
            )
            .expect("Failed to build preview stream");

        stream.play().expect("Failed to start preview stream");

        ChannelSelectState {
            device_name,
            num_channels,
            selected,
            cursor: 0,
            levels,
            _stream: stream,
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
    let levels = state.levels.lock().map(|l| l.clone()).unwrap_or_default();

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
        state.device_name
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
            if state.cursor < (state.num_channels as usize).saturating_sub(1) {
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
