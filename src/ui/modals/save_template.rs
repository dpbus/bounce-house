use crossterm::event::{KeyCode, KeyEvent};
use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

use crate::app::App;
use crate::template;
use crate::ui::modals::Action;
use crate::ui::view::View;
use crate::ui::widgets::{channel_preview_row, flow_columns, key_hint, labeled, modal};

const COL_WIDTH: u16 = 20;
const MAX_COLS: u16 = 4;
const HEADER_ROWS: u16 = 2;
const INPUT_ROW: u16 = 1;
const HINTS_ROW: u16 = 1;
const GAP_ROW: u16 = 1;
const NUM_GAPS: u16 = 3; // before grid, after grid, after input
const INNER_MARGIN: u16 = 2;
const MIN_CONTENT_WIDTH: u16 = 50;

const CONTENT_CHROME_ROWS: u16 =
    HEADER_ROWS + INPUT_ROW + HINTS_ROW + (GAP_ROW * NUM_GAPS) + INNER_MARGIN;

struct ContentLayout {
    cols: u16,
    width: u16,
    height: u16,
}

pub struct SaveTemplateModal {
    buf: String,
}

impl SaveTemplateModal {
    pub fn new() -> Self {
        Self { buf: String::new() }
    }

    pub fn handle_key(&mut self, key: KeyEvent, app: &mut App, view: &mut View) -> Action {
        match key.code {
            KeyCode::Esc => Action::Close,
            KeyCode::Enter => {
                let name = self.buf.trim();
                if !template::is_valid_name(name) {
                    return Action::Stay;
                }
                if app.save_template(name).is_ok() {
                    view.flash_template_save(name.to_string());
                }
                Action::Close
            }
            KeyCode::Backspace => {
                self.buf.pop();
                Action::Stay
            }
            KeyCode::Char(c) => {
                self.buf.push(c);
                Action::Stay
            }
            _ => Action::Stay,
        }
    }

    pub fn draw(&self, frame: &mut Frame, app: &App) {
        let n_channels = app.session.channels.len() as u16;
        let layout = pick_layout(n_channels);
        let inner = modal(frame, "Save Template", layout.width, layout.height);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([
                Constraint::Length(HEADER_ROWS),
                Constraint::Length(GAP_ROW),
                Constraint::Min(1),
                Constraint::Length(GAP_ROW),
                Constraint::Length(INPUT_ROW),
                Constraint::Length(GAP_ROW),
                Constraint::Length(HINTS_ROW),
            ])
            .split(inner);

        frame.render_widget(Paragraph::new(header_lines(app)), chunks[0]);
        let channels: Vec<Line<'static>> = app
            .session
            .channels
            .iter()
            .map(channel_preview_row)
            .collect();
        flow_columns(frame, chunks[2], &channels, layout.cols as u32);
        frame.render_widget(Paragraph::new(save_as_line(&self.buf)), chunks[4]);
        frame.render_widget(Paragraph::new(hints_line()).centered(), chunks[6]);
    }
}

/// Picks the layout that fits all channels: more columns for higher counts,
/// capped at MAX_COLS. Width clamped to MIN_CONTENT_WIDTH so the header
/// and hints lines don't get squeezed at low channel counts.
fn pick_layout(n_channels: u16) -> ContentLayout {
    let cols = match n_channels {
        0..=8 => 1,
        9..=20 => 2,
        21..=40 => 3,
        _ => MAX_COLS,
    };
    let rows = n_channels.div_ceil(cols).max(1);
    let raw_width = cols * COL_WIDTH + cols.saturating_sub(1) + INNER_MARGIN;
    ContentLayout {
        cols,
        width: raw_width.max(MIN_CONTENT_WIDTH),
        height: CONTENT_CHROME_ROWS + rows,
    }
}

fn header_lines(app: &App) -> Vec<Line<'static>> {
    let total = app.engine.channel_count();
    let armed = app.session.armed().count();
    vec![
        labeled("Device:    ", app.engine.device_name().to_string()),
        labeled("Channels:  ", format!("{} armed / {}", armed, total)),
    ]
}

fn save_as_line(buf: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled("Save as:   ", Style::default().fg(Color::Yellow)),
        Span::raw(buf.to_string()),
        Span::styled(
            "_",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::SLOW_BLINK),
        ),
    ])
}

fn hints_line() -> Line<'static> {
    let mut spans = Vec::new();
    spans.extend(key_hint("Enter", "save  ", Color::Cyan));
    spans.extend(key_hint("Esc", "cancel", Color::DarkGray));
    Line::from(spans)
}
