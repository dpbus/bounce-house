use crossterm::event::{KeyCode, KeyEvent};
use ratatui::prelude::*;
use ratatui::widgets::{List, ListItem, ListState, Paragraph};

use crate::app::App;
use crate::template::Template;
use crate::ui::Action;
use crate::ui::view::View;
use crate::ui::widgets::{
    channel_preview_row, channels_summary, dim_status, flow_columns, key_hint, labeled, modal,
    truncate_with_ellipsis,
};

const LIST_WIDTH: u16 = 20;
const COL_WIDTH: u16 = 20;
const GRID_COLS: u16 = 3;
const GRID_ROWS: u16 = 20;
const HEADER_ROWS: u16 = 3;
const HINTS_ROW: u16 = 1;
const GAP_ROW: u16 = 1;
const INNER_MARGIN: u16 = 2;
const HORIZONTAL_GAP: u16 = 2; // between list and preview

/// Fixed modal size: list pane + 3-column × 20-row preview grid + chrome.
const CONTENT_WIDTH: u16 =
    LIST_WIDTH + HORIZONTAL_GAP + GRID_COLS * COL_WIDTH + (GRID_COLS - 1) + INNER_MARGIN;
const CONTENT_HEIGHT: u16 = HEADER_ROWS + GAP_ROW + GRID_ROWS + GAP_ROW + HINTS_ROW + INNER_MARGIN;

pub struct LoadTemplateModal {
    entries: Vec<Template>,
    cursor: usize,
}

impl LoadTemplateModal {
    pub fn new(entries: Vec<Template>) -> Self {
        Self { entries, cursor: 0 }
    }

    pub fn handle_key(&mut self, key: KeyEvent, app: &mut App, view: &mut View) -> Action {
        match key.code {
            KeyCode::Esc => Action::Close,
            KeyCode::Up | KeyCode::Char('k') => {
                self.cursor = self.cursor.saturating_sub(1);
                Action::Stay
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let max = self.entries.len().saturating_sub(1);
                if self.cursor < max {
                    self.cursor += 1;
                }
                Action::Stay
            }
            KeyCode::Enter => {
                let Some(template) = self.entries.get(self.cursor) else {
                    return Action::Stay;
                };
                let name = template.name.clone();
                app.load_template(template);
                view.flash_template_load(name);
                Action::Close
            }
            _ => Action::Stay,
        }
    }

    pub fn draw(&self, frame: &mut Frame, _app: &App) {
        let inner = modal(frame, "Load Template", CONTENT_WIDTH, CONTENT_HEIGHT);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([
                Constraint::Min(3), // list/preview area
                Constraint::Length(GAP_ROW),
                Constraint::Length(HINTS_ROW),
            ])
            .split(inner);

        if self.entries.is_empty() {
            frame.render_widget(Paragraph::new(dim_status("No templates yet")), chunks[0]);
        } else {
            self.draw_split(frame, chunks[0]);
        }

        frame.render_widget(
            Paragraph::new(hints_line(self.entries.is_empty())).centered(),
            chunks[2],
        );
    }

    fn draw_split(&self, frame: &mut Frame, area: Rect) {
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(LIST_WIDTH),
                Constraint::Length(HORIZONTAL_GAP),
                Constraint::Min(1),
            ])
            .split(area);

        let items: Vec<ListItem> = self
            .entries
            .iter()
            .map(|t| ListItem::new(truncate_with_ellipsis(&t.name, LIST_WIDTH as usize)))
            .collect();
        let list =
            List::new(items).highlight_style(Style::default().fg(Color::White).bg(Color::DarkGray));
        let mut state = ListState::default();
        state.select(Some(self.cursor));
        frame.render_stateful_widget(list, cols[0], &mut state);

        if let Some(template) = self.entries.get(self.cursor) {
            draw_preview(frame, cols[2], template);
        }
    }
}

fn draw_preview(frame: &mut Frame, area: Rect, template: &Template) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(HEADER_ROWS),
            Constraint::Length(GAP_ROW),
            Constraint::Min(1),
        ])
        .split(area);

    let armed = template.channels.iter().filter(|c| c.armed).count();
    let total = template.channels.len();
    let header = vec![
        labeled("Name:      ", template.name.clone()),
        labeled("Device:    ", template.device_name.clone()),
        labeled("Channels:  ", channels_summary(armed, total)),
    ];
    frame.render_widget(Paragraph::new(header), chunks[0]);

    let mut channels: Vec<Line<'static>> =
        template.channels.iter().map(channel_preview_row).collect();
    let max_visible = (GRID_COLS * GRID_ROWS) as usize;
    if channels.len() > max_visible {
        channels.truncate(max_visible);
        *channels.last_mut().unwrap() =
            Line::from(Span::styled(" ...", Style::default().fg(Color::DarkGray)));
    }
    flow_columns(frame, chunks[2], &channels, GRID_COLS as u32);
}

fn hints_line(empty: bool) -> Line<'static> {
    let mut spans = Vec::new();
    if !empty {
        spans.extend(key_hint("Enter", "load  ", Color::Cyan));
    }
    spans.extend(key_hint("Esc", "cancel", Color::Cyan));
    Line::from(spans)
}
