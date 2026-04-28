use crossterm::event::{KeyCode, KeyEvent};
use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

use crate::app::App;
use crate::channel::Channel;
use crate::template;
use crate::ui::modals::Action;
use crate::ui::view::View;
use crate::ui::widgets::{MODAL_BORDER_OVERHEAD, flow_columns, key_hint, labeled, modal};

const COL_WIDTH: u16 = 18;
const MAX_COLS: u16 = 4;
const MAX_HEIGHT_PCT: u16 = 90;

const MIN_MODAL_WIDTH: u16 = 60;
const MIN_MODAL_HEIGHT: u16 = 18;
const MIN_CONTENT_WIDTH: u16 = MIN_MODAL_WIDTH - MODAL_BORDER_OVERHEAD;
const MIN_CONTENT_HEIGHT: u16 = MIN_MODAL_HEIGHT - MODAL_BORDER_OVERHEAD;

const HEADER_ROWS: u16 = 2;
const GAP_ROW: u16 = 1;
const INPUT_ROW: u16 = 1;
const HINTS_ROW: u16 = 1;
const NUM_GAPS: u16 = 3;
const INNER_MARGIN: u16 = 2;

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
        let layout = pick_layout(n_channels, frame.area());
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
        let channels = channel_lines(app);
        flow_columns(frame, chunks[2], &channels, layout.cols as u32);
        frame.render_widget(Paragraph::new(save_as_line(&self.buf)), chunks[4]);
        frame.render_widget(Paragraph::new(hints_line()).centered(), chunks[6]);
    }
}

fn pick_layout(n_channels: u16, frame_area: Rect) -> ContentLayout {
    let max_modal_height = (frame_area.height * MAX_HEIGHT_PCT / 100)
        .max(CONTENT_CHROME_ROWS + MODAL_BORDER_OVERHEAD + 1);
    let max_content_height = max_modal_height - MODAL_BORDER_OVERHEAD;
    let max_cols = (frame_area.width / COL_WIDTH).clamp(1, MAX_COLS);

    let cols = (1..=max_cols)
        .find(|&cols| {
            let rows = n_channels.div_ceil(cols);
            (CONTENT_CHROME_ROWS + rows).max(MIN_CONTENT_HEIGHT) <= max_content_height
        })
        .unwrap_or(max_cols);

    let rows = n_channels.div_ceil(cols);
    let height = (CONTENT_CHROME_ROWS + rows).clamp(MIN_CONTENT_HEIGHT, max_content_height);
    ContentLayout {
        cols,
        width: content_width(cols),
        height,
    }
}

fn content_width(cols: u16) -> u16 {
    (cols * COL_WIDTH + cols.saturating_sub(1) + INNER_MARGIN).max(MIN_CONTENT_WIDTH)
}

fn header_lines(app: &App) -> Vec<Line<'static>> {
    let total = app.engine.channel_count();
    let armed = app.session.armed().count();
    vec![
        labeled("Device:    ", app.engine.device_name().to_string()),
        labeled("Channels:  ", format!("{} armed / {}", armed, total)),
    ]
}

fn channel_lines(app: &App) -> Vec<Line<'static>> {
    app.session.channels.iter().map(channel_row).collect()
}

fn channel_row(channel: &Channel) -> Line<'static> {
    let (marker, marker_style, row_style) = if channel.armed {
        ("●", Style::default().fg(Color::Red), Style::default())
    } else {
        (
            "○",
            Style::default().fg(Color::DarkGray),
            Style::default().fg(Color::DarkGray),
        )
    };
    let label = channel.label.clone().unwrap_or_else(|| "—".to_string());
    Line::from(vec![
        Span::styled(format!(" {} ", marker), marker_style),
        Span::styled(format!("Ch {:>2}  ", channel.index), row_style),
        Span::styled(label, row_style),
    ])
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
