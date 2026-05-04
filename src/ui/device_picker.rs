use std::io;
use std::time::Duration;

use chrono::Datelike;
use crossterm::event::{self, Event, KeyCode};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Padding, Paragraph};

use crate::audio::DeviceInfo;
use crate::ui::widgets::{key_hint, truncate_with_ellipsis};

const MODAL_INNER_WIDTH: u16 = 50;
/// Modal block adds 2 cols of border + `Padding::new(2, 2, 1, 1)` →
/// outer width grows by 6, outer height grows by 4 vs. inner content.
const MODAL_HORIZONTAL_CHROME: u16 = 6;
const MODAL_VERTICAL_CHROME: u16 = 4;

const LOGO_ROWS: u16 = 5;

/// Figlet-style "BounceHouse" logo. Line 1 begins with a leading
/// space so the top underscores align over the body strokes; do not
/// add `\` line continuation here (that would strip the leading
/// whitespace off line 1).
const LOGO_ART: &str = " _____                     _____
| __  |___ _ _ ___ ___ ___|  |  |___ _ _ ___ ___
| __ -| . | | |   |  _| -_|     | . | | |_ -| -_|
|_____|___|___|_|_|___|___|__|__|___|___|___|___|
                                                 ";

const DECORATION: &str = "──── ♫ ────";
const TAGLINE: &str = "multitrack capture";
const INSTRUCTION: &str = "Select an input device to begin";

/// Boot-phase TUI: shows the available input devices and returns the user's pick.
///
/// - 0 devices → returns `NotFound` error
/// - 1 device  → auto-picks, no UI
/// - 2+ devices → renders a picker until the user selects with Enter
pub fn pick(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    mut devices: Vec<DeviceInfo>,
) -> io::Result<DeviceInfo> {
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

        if event::poll(Duration::from_millis(50))?
            && let Event::Key(key) = event::read()?
        {
            match key.code {
                KeyCode::Up | KeyCode::Char('k') => {
                    cursor = cursor.saturating_sub(1);
                }
                KeyCode::Down | KeyCode::Char('j') if cursor + 1 < devices.len() => {
                    cursor += 1;
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

fn draw(frame: &mut Frame, devices: &[DeviceInfo], cursor: usize) {
    // Full-screen border, matching the main app's outer chrome.
    let screen_block = Block::default()
        .borders(Borders::ALL)
        .padding(Padding::new(2, 2, 1, 1))
        .border_style(Style::default().fg(Color::Cyan));
    let inner = screen_block.inner(frame.area());
    frame.render_widget(screen_block, frame.area());

    let modal_height = devices.len() as u16 + 2 + MODAL_VERTICAL_CHROME;

    // Reserve the bottom row for the status bar; everything else
    // floats in the remaining area, biased above center via 2:3 fill.
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(inner);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Fill(2),              // top filler — biases content up
            Constraint::Length(1),            // decoration above logo
            Constraint::Length(LOGO_ROWS),    // BounceHouse logo
            Constraint::Length(1),            // tagline
            Constraint::Length(1),            // spacer
            Constraint::Length(1),            // decoration below tagline
            Constraint::Length(2),            // spacer
            Constraint::Length(1),            // instruction line
            Constraint::Length(modal_height), // modal
            Constraint::Fill(3),              // bottom filler
        ])
        .split(outer[0]);

    let dim = Color::DarkGray;
    let accent = Color::Cyan;
    render_centered_line(frame, chunks[1], DECORATION, dim);
    draw_logo(frame, chunks[2]);
    render_centered_line(frame, chunks[3], TAGLINE, accent);
    render_centered_line(frame, chunks[5], DECORATION, dim);
    render_centered_line(frame, chunks[7], INSTRUCTION, dim);
    draw_modal(frame, chunks[8], devices, cursor);
    render_centered_line(frame, outer[1], &status_bar_text(), dim);
}

fn render_centered_line(frame: &mut Frame, area: Rect, text: &str, color: Color) {
    let line = Line::from(Span::styled(text.to_string(), Style::default().fg(color)));
    frame.render_widget(Paragraph::new(line).alignment(Alignment::Center), area);
}

fn status_bar_text() -> String {
    format!(
        "bounce-house v{}  ·  © {} Pixel Bus",
        env!("CARGO_PKG_VERSION"),
        chrono::Local::now().year(),
    )
}

fn draw_modal(frame: &mut Frame, region: Rect, devices: &[DeviceInfo], cursor: usize) {
    let inner_height = devices.len() as u16 + 2; // list + spacer + footer
    let outer = center_rect(
        region,
        MODAL_INNER_WIDTH + MODAL_HORIZONTAL_CHROME,
        inner_height + MODAL_VERTICAL_CHROME,
    );

    let block = Block::default()
        .borders(Borders::ALL)
        .padding(Padding::new(2, 2, 1, 1))
        .border_style(Style::default().fg(Color::Cyan));
    let inner = block.inner(outer);
    frame.render_widget(Clear, outer);
    frame.render_widget(block, outer);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(devices.len() as u16),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(inner);

    draw_list(frame, chunks[0], devices, cursor);
    draw_footer(frame, chunks[2]);
}

fn center_rect(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    )
}

fn draw_logo(frame: &mut Frame, area: Rect) {
    // Pad each line to the logo's max width so per-line centering
    // doesn't shift shorter lines (e.g. the row containing only the
    // capital letters' top edges) horizontally relative to the rest.
    let max_width = LOGO_ART
        .lines()
        .map(|l| l.chars().count())
        .max()
        .unwrap_or(0);
    let lines: Vec<Line> = LOGO_ART
        .lines()
        .map(|line| {
            let pad = max_width.saturating_sub(line.chars().count());
            let padded = format!("{line}{}", " ".repeat(pad));
            Line::from(Span::styled(padded, Style::default().fg(Color::Cyan)))
        })
        .collect();
    frame.render_widget(Paragraph::new(lines).alignment(Alignment::Center), area);
}

fn draw_list(frame: &mut Frame, area: Rect, devices: &[DeviceInfo], cursor: usize) {
    let total_width = area.width as usize;
    let lines: Vec<Line> = devices
        .iter()
        .enumerate()
        .map(|(i, dev)| device_row(dev, i == cursor, total_width))
        .collect();
    frame.render_widget(Paragraph::new(lines), area);
}

fn device_row(dev: &DeviceInfo, selected: bool, total_width: usize) -> Line<'static> {
    let io_summary = match dev.output_channel_count() {
        Some(out) => format!("{} in / {} out", dev.input_channel_count(), out),
        None => format!("{} in", dev.input_channel_count()),
    };
    let metadata = format!("{} · {} kHz", io_summary, dev.input_sample_rate().0 / 1000);
    let bullet = if selected { "▌ " } else { "  " };
    let bullet_color = if selected {
        Color::Cyan
    } else {
        Color::DarkGray
    };

    const BULLET_WIDTH: usize = 2;
    let metadata_width = metadata.chars().count();
    let name_budget = total_width.saturating_sub(BULLET_WIDTH + metadata_width + 2);
    let name_text = if dev.name().chars().count() > name_budget {
        truncate_with_ellipsis(dev.name(), name_budget)
    } else {
        dev.name().to_string()
    };
    let padding = total_width
        .saturating_sub(BULLET_WIDTH)
        .saturating_sub(name_text.chars().count())
        .saturating_sub(metadata_width);

    let name_style = if selected {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };

    Line::from(vec![
        Span::styled(bullet, Style::default().fg(bullet_color)),
        Span::styled(name_text, name_style),
        Span::raw(" ".repeat(padding)),
        Span::styled(metadata, Style::default().fg(Color::DarkGray)),
    ])
}

fn draw_footer(frame: &mut Frame, area: Rect) {
    let mut spans = vec![];
    spans.extend(key_hint("↑↓", "navigate", Color::Cyan));
    spans.push(Span::raw("  "));
    spans.extend(key_hint("Enter", "select", Color::Cyan));
    spans.push(Span::raw("  "));
    spans.extend(key_hint("Esc", "quit", Color::Cyan));
    frame.render_widget(
        Paragraph::new(Line::from(spans)).alignment(Alignment::Center),
        area,
    );
}
