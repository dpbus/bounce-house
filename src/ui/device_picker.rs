use std::io;
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, List, ListItem};

use crate::audio::Device;

/// Boot-phase TUI: shows the available input devices and returns the user's pick.
///
/// - 0 devices → returns `NotFound` error
/// - 1 device  → auto-picks, no UI
/// - 2+ devices → renders a picker until the user selects with Enter
pub fn pick(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> io::Result<Device> {
    let mut devices = Device::list();

    if devices.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "No audio input devices found",
        ));
    }

    if devices.len() == 1 {
        return Ok(devices.into_iter().next().unwrap());
    }

    let mut cursor = 0usize;
    loop {
        terminal.draw(|frame| draw(frame, &devices, cursor))?;

        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Up | KeyCode::Char('k') => {
                        cursor = cursor.saturating_sub(1);
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        if cursor + 1 < devices.len() {
                            cursor += 1;
                        }
                    }
                    KeyCode::Enter => {
                        return Ok(devices.swap_remove(cursor));
                    }
                    KeyCode::Esc | KeyCode::Char('q') => {
                        return Err(io::Error::new(
                            io::ErrorKind::Interrupted,
                            "User quit during device selection",
                        ));
                    }
                    _ => {}
                }
            }
        }
    }
}

fn draw(frame: &mut Frame, devices: &[Device], cursor: usize) {
    let items: Vec<ListItem> = devices
        .iter()
        .enumerate()
        .map(|(i, device)| {
            let style = if i == cursor {
                Style::default().fg(Color::Black).bg(Color::Cyan)
            } else {
                Style::default()
            };
            ListItem::new(device.name().to_string()).style(style)
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .title(" Select Audio Device — ↑↓ navigate, Enter to confirm, q to quit ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan)),
    );

    frame.render_widget(list, frame.area());
}
