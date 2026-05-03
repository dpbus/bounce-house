use crossterm::event::{KeyCode, KeyEvent};
use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

use crate::app::App;
use crate::ui::ModalOutcome;
use crate::ui::view::View;
use crate::ui::widgets::{key_hint, modal};

const CONTENT_WIDTH: u16 = 50;
const KEY_COL_WIDTH: usize = 11;

pub struct HelpModal;

impl HelpModal {
    pub fn new() -> Self {
        Self
    }

    pub fn handle_key(&mut self, key: KeyEvent, _app: &mut App, _view: &mut View) -> ModalOutcome {
        match key.code {
            KeyCode::Esc => ModalOutcome::Close,
            _ => ModalOutcome::Stay,
        }
    }

    pub fn draw(&self, frame: &mut Frame, _app: &App) {
        let lines = body_lines();
        let height = lines.len() as u16 + 4;
        let inner = modal(frame, "Help", CONTENT_WIDTH, height);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .horizontal_margin(3)
            .vertical_margin(1)
            .constraints([
                Constraint::Min(1),
                Constraint::Length(1),
                Constraint::Length(1),
            ])
            .split(inner);

        frame.render_widget(Paragraph::new(lines), chunks[0]);
        frame.render_widget(Paragraph::new(hints_line()).centered(), chunks[2]);
    }
}

fn body_lines() -> Vec<Line<'static>> {
    let mut lines = Vec::new();

    section(&mut lines, "Recording");
    entry(&mut lines, "T", "drop marker and name take");
    entry(&mut lines, "Space", "drop marker");
    entry(&mut lines, "N", "name last take");
    entry(&mut lines, "Backspace", "unmark last marker");
    entry(&mut lines, "P", "pause / resume");
    entry(&mut lines, "Esc", "stop");
    blank(&mut lines);

    section(&mut lines, "Templates");
    entry(&mut lines, "Ctrl+S", "save as template");
    entry(&mut lines, "Ctrl+L", "load template");
    blank(&mut lines);

    section(&mut lines, "Channels");
    entry(&mut lines, "C", "open channel picker");
    blank(&mut lines);

    section(&mut lines, "App");
    entry(&mut lines, "W", "cycle waveform window");
    entry(&mut lines, ",", "settings");
    entry(&mut lines, "Q / Esc", "quit");
    entry(&mut lines, "?", "open this help");

    lines
}

fn section(lines: &mut Vec<Line<'static>>, label: &str) {
    lines.push(Line::from(Span::styled(
        label.to_string(),
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    )));
}

fn entry(lines: &mut Vec<Line<'static>>, key: &str, action: &str) {
    lines.push(Line::from(vec![
        Span::raw("  "),
        Span::styled(
            format!("{:<width$}", format!("[{}]", key), width = KEY_COL_WIDTH),
            Style::default().fg(Color::Cyan),
        ),
        Span::raw("  "),
        Span::raw(action.to_string()),
    ]));
}

fn blank(lines: &mut Vec<Line<'static>>) {
    lines.push(Line::from(""));
}

fn hints_line() -> Line<'static> {
    Line::from(key_hint("Esc", "close", Color::Cyan))
}
