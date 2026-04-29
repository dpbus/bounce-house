use std::cell::Cell;

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph};

use crate::app::App;
use crate::channel::Channel;
use crate::ui::Action;
use crate::ui::text_input::TextInput;
use crate::ui::view::View;
use crate::ui::widgets::{
    MODAL_BORDER_OVERHEAD, horizontal_meter, input_with_cursor, key_hint, truncate_with_ellipsis,
};

const METER_WIDTH: usize = 18;
const LABEL_WIDTH: usize = 18;
const DB_WIDTH: u16 = 8; // "{:>5.1} dB" or "   -∞ dB"
const FOOTER_ROW: u16 = 1;
const GAP_ROW: u16 = 1;
const INNER_MARGIN: u16 = 2;

// marker(3) + " Ch"(3) + id(3) + " "(1) + meter + " "(1) + dB + "  "(2) + label + margin
const CONTENT_WIDTH: u16 =
    3 + 3 + 3 + 1 + METER_WIDTH as u16 + 1 + DB_WIDTH + 2 + LABEL_WIDTH as u16 + INNER_MARGIN;

pub struct ChannelPickerModal {
    cursor: usize,
    /// Persisted scroll offset; using `Cell` so `draw(&self)` can adjust
    /// it without changing the modal's outward mutability.
    scroll_offset: Cell<usize>,
    renaming: Option<TextInput>,
}

impl ChannelPickerModal {
    pub fn new() -> Self {
        Self {
            cursor: 0,
            scroll_offset: Cell::new(0),
            renaming: None,
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent, app: &mut App, _view: &mut View) -> Action {
        if self.renaming.is_some() {
            return self.handle_rename_key(key, app);
        }
        self.handle_browse_key(key, app)
    }

    fn handle_browse_key(&mut self, key: KeyEvent, app: &mut App) -> Action {
        match key.code {
            KeyCode::Esc | KeyCode::Char('c') | KeyCode::Char('C') => Action::Close,
            KeyCode::Up | KeyCode::Char('k') => {
                self.cursor = self.cursor.saturating_sub(1);
                Action::Stay
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let max = app.project.channels.len().saturating_sub(1);
                if self.cursor < max {
                    self.cursor += 1;
                }
                Action::Stay
            }
            KeyCode::Char(' ') => {
                if let Some(index) = self.focused_channel_index(app) {
                    app.toggle_armed(index);
                }
                Action::Stay
            }
            KeyCode::Tab => {
                self.renaming = Some(TextInput::with_value(&self.focused_label(app)));
                Action::Stay
            }
            _ => Action::Stay,
        }
    }

    fn handle_rename_key(&mut self, key: KeyEvent, app: &mut App) -> Action {
        let Some(input) = self.renaming.as_mut() else {
            return Action::Stay;
        };
        match key.code {
            KeyCode::Esc => {
                self.renaming = None;
                Action::Stay
            }
            KeyCode::Enter => {
                let trimmed = input.value().trim();
                let label = if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                };
                if let Some(index) = self.focused_channel_index(app) {
                    app.set_label(index, label);
                }
                self.renaming = None;
                Action::Stay
            }
            _ => {
                input.handle_edit_key(key);
                Action::Stay
            }
        }
    }

    fn focused_channel_index(&self, app: &App) -> Option<u16> {
        app.project.channels.get(self.cursor).map(|c| c.index)
    }

    fn focused_label(&self, app: &App) -> String {
        app.project
            .channels
            .get(self.cursor)
            .and_then(|c| c.label.clone())
            .unwrap_or_default()
    }

    pub fn draw(&self, frame: &mut Frame, app: &App) {
        let frame_area = frame.area();
        let outer_width = (CONTENT_WIDTH + MODAL_BORDER_OVERHEAD).min(frame_area.width);

        // Left-anchored full-height drawer: hugs the left edge of the
        // screen and fills the full vertical space. Other modals stay
        // centered via widgets::modal.
        let area = Rect::new(frame_area.x, frame_area.y, outer_width, frame_area.height);
        frame.render_widget(Clear, area);
        let block = Block::default()
            .title(" Channels ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan));
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([
                Constraint::Min(3),
                Constraint::Length(GAP_ROW),
                Constraint::Length(FOOTER_ROW),
            ])
            .split(inner);

        // Channel rows interleaved with a dim under-meter separator —
        // breathing room plus a subtle anchor right where adjacent
        // armed meters would otherwise visually merge.
        let total = app.project.channels.len();
        let mut items: Vec<ListItem> = Vec::with_capacity(total * 2);
        for (i, channel) in app.project.channels.iter().enumerate() {
            let focused = i == self.cursor;
            let renaming_input = if focused {
                self.renaming.as_ref()
            } else {
                None
            };
            let level = app.display_levels[channel.index as usize];
            let peak = app.peak_holds[channel.index as usize];
            items.push(ListItem::new(channel_row(
                channel,
                level,
                peak,
                focused,
                renaming_input,
            )));
            if i + 1 < total {
                items.push(ListItem::new(separator_row()));
            }
        }

        // Channel rows live at even list indices; separators at odd. We
        // manage `offset` ourselves so it persists across renders;
        // a fresh ListState with offset=0 would re-pin the cursor to
        // the viewport's bottom every frame.
        let visible = chunks[0].height as usize;
        let cursor_row = self.cursor * 2;
        let mut offset = self.scroll_offset.get();
        if cursor_row < offset {
            offset = cursor_row;
        } else if cursor_row >= offset + visible {
            offset = cursor_row + 1 - visible;
        }
        let max_offset = items.len().saturating_sub(visible);
        if offset > max_offset {
            offset = max_offset;
        }
        self.scroll_offset.set(offset);

        let list = List::new(items);
        let mut state = ListState::default();
        state.select(Some(cursor_row));
        *state.offset_mut() = offset;
        frame.render_stateful_widget(list, chunks[0], &mut state);

        frame.render_widget(
            Paragraph::new(footer_line(self.renaming.is_some())),
            chunks[2],
        );
    }
}

/// Dim left-anchored divider — short solid line at the far left of the
/// row. Visually anchors row breaks without competing with the meter.
fn separator_row() -> Line<'static> {
    Line::from(Span::styled(
        "─────",
        Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::DIM),
    ))
}

fn channel_row(
    channel: &Channel,
    level: f32,
    peak: f32,
    focused: bool,
    renaming_input: Option<&TextInput>,
) -> Line<'static> {
    let row_style = if focused {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };
    let marker_style = if channel.armed {
        Style::default().fg(Color::Red)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let armed_marker = if channel.armed { "[●]" } else { "[ ]" };

    let mut spans = vec![
        Span::styled(armed_marker.to_string(), marker_style),
        Span::styled(format!(" Ch{:>3} ", channel.index), row_style),
    ];
    spans.extend(horizontal_meter(level, Some(peak), METER_WIDTH));
    spans.push(Span::raw(" "));
    spans.push(Span::styled(db_label(level), row_style));
    spans.push(Span::raw("  "));

    if let Some(input) = renaming_input {
        spans.push(Span::styled("✏ ", Style::default().fg(Color::Yellow)));
        spans.extend(input_with_cursor(input, Style::default().fg(Color::Yellow)));
    } else {
        let label = channel.label.clone().unwrap_or_else(|| "—".to_string());
        spans.push(Span::styled(
            format!(
                "{:<width$}",
                truncate_with_ellipsis(&label, LABEL_WIDTH),
                width = LABEL_WIDTH
            ),
            row_style,
        ));
    }

    Line::from(spans)
}

fn db_label(level: f32) -> String {
    const SILENCE: f32 = 0.0001;
    if level < SILENCE {
        "   -∞ dB".to_string()
    } else {
        format!("{:>5.1} dB", 20.0 * level.log10())
    }
}


fn footer_line(renaming: bool) -> Line<'static> {
    let mut spans = Vec::new();
    if renaming {
        spans.extend(key_hint("Enter", "save  ", Color::Cyan));
        spans.extend(key_hint("Esc", "cancel", Color::Cyan));
    } else {
        spans.extend(key_hint("Space", "arm  ", Color::Cyan));
        spans.extend(key_hint("Tab", "rename  ", Color::Cyan));
        spans.extend(key_hint("Esc", "close", Color::Cyan));
    }
    Line::from(spans)
}
