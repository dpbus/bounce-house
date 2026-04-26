use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Padding, Paragraph};

use crate::app::{App, AppState};
use crate::ui::widgets::{key_hint, key_hint_when};
use crate::ui::{meter_panel, recording_panel, session_panel, waveform};

const TOP_BAR_HEIGHT: u16 = 12;
const WAVEFORM_HEIGHT: u16 = 18;
const GAP: u16 = 1; // standard breathing room between sections

pub fn draw(frame: &mut Frame, app: &App) {
    let block = outer_block(app);
    let inner = block.inner(frame.area());
    frame.render_widget(block, frame.area());

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(TOP_BAR_HEIGHT),  // session + recording panels
            Constraint::Length(GAP),
            Constraint::Length(WAVEFORM_HEIGHT), // waveform panel
            Constraint::Length(GAP),
            Constraint::Fill(1),                  // meter strips fill remaining space
            Constraint::Length(GAP),
            Constraint::Length(1),                // footer (key hints)
        ])
        .split(inner);

    let top_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .spacing(2)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(chunks[0]);

    session_panel::draw(frame, top_chunks[0], app);
    recording_panel::draw(frame, top_chunks[1], app);
    waveform::draw(frame, chunks[2], app);
    meter_panel::draw(frame, chunks[4], app);
    frame.render_widget(Paragraph::new(footer_line(app)), chunks[6]);
}

fn outer_block(app: &App) -> Block<'static> {
    let (title, color) = if app.is_recording() {
        (format!(" ● Recording — {} ", app.engine.name()), Color::Red)
    } else {
        (format!(" {} ", app.engine.name()), Color::Cyan)
    };
    Block::default()
        .title(title)
        .borders(Borders::ALL)
        .padding(Padding::new(2, 2, 1, 1))
        .border_style(Style::default().fg(color))
}

fn footer_line(app: &App) -> Line<'static> {
    let mut spans = Vec::new();
    match &app.state {
        AppState::NamingTake { .. } => {
            return Line::from(Span::styled(
                "Naming take",
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
            ));
        }
        AppState::ConfirmingStop => {
            spans.push(Span::styled(
                "Stop recording?  ",
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
            ));
            spans.extend(key_hint("Esc", "yes  ", Color::Cyan));
            spans.extend(key_hint("any other key", "no", Color::DarkGray));
        }
        AppState::PickingChannel { .. } => {
            spans.extend(key_hint("Esc", "close picker", Color::Cyan));
        }
        AppState::Default if app.is_recording() => {
            let last_unbound = app.current_timeline().is_some_and(|t| t.last_marker_unbound());
            spans.extend(key_hint("T", "take  ", Color::Cyan));
            spans.extend(key_hint("Space", "mark  ", Color::Cyan));
            spans.extend(key_hint_when(last_unbound, "Backspace", "unmark  ", Color::Cyan));
            spans.extend(key_hint_when(last_unbound, "N", "name take  ", Color::Cyan));
            spans.extend(key_hint("Esc", "stop", Color::DarkGray));
        }
        AppState::Default => {
            spans.extend(key_hint("R", "record  ", Color::Cyan));
            spans.extend(key_hint("C", "channels  ", Color::Cyan));
            spans.extend(key_hint("Q", "quit", Color::DarkGray));
        }
    }
    Line::from(spans)
}
