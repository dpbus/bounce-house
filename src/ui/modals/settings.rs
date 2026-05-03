use crossterm::event::{KeyCode, KeyEvent};
use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

use crate::app::App;
use crate::settings::Settings;
use crate::ui::ModalOutcome;
use crate::ui::text_input::TextInput;
use crate::ui::view::View;
use crate::ui::widgets::{input_with_cursor, key_hint, modal};

const FIELDS: [&str; 3] = ["Sessions:  ", "Bounces:   ", "Templates: "];
const CONTENT_WIDTH: u16 = 70;
const FIELD_ROWS: u16 = FIELDS.len() as u16;
const HINTS_ROW: u16 = 1;
const ERROR_ROW: u16 = 1;
const GAP_ROW: u16 = 1;
const INNER_MARGIN: u16 = 2;
const CONTENT_HEIGHT: u16 = FIELD_ROWS + GAP_ROW + ERROR_ROW + GAP_ROW + HINTS_ROW + INNER_MARGIN;

const CONSTRAINTS: [Constraint; 7] = [
    Constraint::Length(1),
    Constraint::Length(1),
    Constraint::Length(1),
    Constraint::Length(GAP_ROW),
    Constraint::Length(ERROR_ROW),
    Constraint::Length(GAP_ROW),
    Constraint::Length(HINTS_ROW),
];
const ERROR_IDX: usize = 4;
const HINTS_IDX: usize = 6;

pub struct SettingsModal {
    bufs: [TextInput; 3],
    focused: usize,
    error: Option<String>,
}

impl SettingsModal {
    pub fn new(settings: &Settings) -> Self {
        Self {
            bufs: [
                TextInput::with_value(&settings.sessions_dir.display().to_string()),
                TextInput::with_value(&settings.bounces_dir.display().to_string()),
                TextInput::with_value(&settings.templates_dir.display().to_string()),
            ],
            focused: 0,
            error: None,
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent, app: &mut App, _view: &mut View) -> ModalOutcome {
        match key.code {
            KeyCode::Esc => ModalOutcome::Close,
            KeyCode::Tab | KeyCode::Down => {
                self.focused = (self.focused + 1) % FIELDS.len();
                ModalOutcome::Stay
            }
            KeyCode::BackTab | KeyCode::Up => {
                self.focused = (self.focused + FIELDS.len() - 1) % FIELDS.len();
                ModalOutcome::Stay
            }
            KeyCode::Enter => {
                match app.settings.update_paths(
                    self.bufs[0].value(),
                    self.bufs[1].value(),
                    self.bufs[2].value(),
                ) {
                    Ok(()) => ModalOutcome::Close,
                    Err(e) => {
                        self.error = Some(e.to_string());
                        ModalOutcome::Stay
                    }
                }
            }
            _ => {
                self.bufs[self.focused].handle_edit_key(key);
                ModalOutcome::Stay
            }
        }
    }

    pub fn draw(&self, frame: &mut Frame, _app: &App) {
        let inner = modal(frame, "Settings", CONTENT_WIDTH, CONTENT_HEIGHT);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints(CONSTRAINTS)
            .split(inner);

        for (i, label) in FIELDS.iter().enumerate() {
            frame.render_widget(
                Paragraph::new(field_line(label, &self.bufs[i], i == self.focused)),
                chunks[i],
            );
        }
        frame.render_widget(
            Paragraph::new(error_line(self.error.as_deref())),
            chunks[ERROR_IDX],
        );
        frame.render_widget(Paragraph::new(hints_line()).centered(), chunks[HINTS_IDX]);
    }
}

fn field_line(label: &'static str, input: &TextInput, focused: bool) -> Line<'static> {
    let label_style = if focused {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let mut spans = vec![Span::styled(label, label_style)];
    if focused {
        spans.extend(input_with_cursor(input, Style::default()));
    } else {
        spans.push(Span::raw(input.value().to_string()));
    }
    Line::from(spans)
}

fn error_line(error: Option<&str>) -> Line<'static> {
    match error {
        Some(msg) => Line::from(Span::styled(
            msg.to_string(),
            Style::default().fg(Color::Red),
        )),
        None => Line::from(""),
    }
}

fn hints_line() -> Line<'static> {
    let mut spans = Vec::new();
    spans.extend(key_hint("Tab", "next  ", Color::Cyan));
    spans.extend(key_hint("Enter", "save  ", Color::Cyan));
    spans.extend(key_hint("Esc", "cancel", Color::Cyan));
    Line::from(spans)
}
