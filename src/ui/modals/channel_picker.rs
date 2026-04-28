use crossterm::event::{KeyCode, KeyEvent};
use ratatui::prelude::*;
use ratatui::widgets::{List, ListItem, ListState, Paragraph};

use crate::app::App;
use crate::channel::Channel;
use crate::ui::modals::Action;
use crate::ui::view::View;
use crate::ui::widgets::{MODAL_BORDER_OVERHEAD, horizontal_meter, key_hint, modal};

const METER_WIDTH: usize = 30;
const WIDTH_PCT: u16 = 80;
const HEIGHT_PCT: u16 = 30;

pub struct ChannelPickerModal {
    cursor: usize,
    renaming: Option<String>,
}

impl ChannelPickerModal {
    pub fn new() -> Self {
        Self {
            cursor: 0,
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
            KeyCode::Esc => Action::Close,
            KeyCode::Up | KeyCode::Char('k') => {
                self.cursor = self.cursor.saturating_sub(1);
                Action::Stay
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let max = app.session.channels.len().saturating_sub(1);
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
                self.renaming = Some(self.focused_label(app).unwrap_or_default());
                Action::Stay
            }
            _ => Action::Stay,
        }
    }

    fn handle_rename_key(&mut self, key: KeyEvent, app: &mut App) -> Action {
        let Some(buf) = self.renaming.as_mut() else {
            return Action::Stay;
        };
        match key.code {
            KeyCode::Esc => {
                self.renaming = None;
                Action::Stay
            }
            KeyCode::Enter => {
                let label = if buf.trim().is_empty() {
                    None
                } else {
                    Some(buf.trim().to_string())
                };
                if let Some(index) = self.focused_channel_index(app) {
                    app.set_label(index, label);
                }
                self.renaming = None;
                Action::Stay
            }
            KeyCode::Backspace => {
                buf.pop();
                Action::Stay
            }
            KeyCode::Char(c) => {
                buf.push(c);
                Action::Stay
            }
            _ => Action::Stay,
        }
    }

    fn focused_channel_index(&self, app: &App) -> Option<u16> {
        app.session.channels.get(self.cursor).map(|c| c.index)
    }

    fn focused_label(&self, app: &App) -> Option<String> {
        app.session
            .channels
            .get(self.cursor)
            .and_then(|c| c.label.clone())
            .or(Some(String::new()))
    }

    pub fn draw(&self, frame: &mut Frame, app: &App) {
        let frame_area = frame.area();
        let content_width =
            (frame_area.width * WIDTH_PCT / 100).saturating_sub(MODAL_BORDER_OVERHEAD);
        let content_height =
            (frame_area.height * HEIGHT_PCT / 100).saturating_sub(MODAL_BORDER_OVERHEAD);
        let inner = modal(frame, "Channels", content_width, content_height);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(3),    // channel list
                Constraint::Length(1), // footer
            ])
            .split(inner);

        let items: Vec<ListItem> = app
            .session
            .channels
            .iter()
            .enumerate()
            .map(|(i, channel)| {
                let focused = i == self.cursor;
                let renaming_buf = if focused {
                    self.renaming.as_deref()
                } else {
                    None
                };
                channel_row(channel, app, focused, renaming_buf)
            })
            .collect();

        // Stateful render with cursor pre-selected so ratatui auto-adjusts the
        // viewport offset to keep it visible. Manual row highlighting is
        // unaffected because we don't set highlight_style.
        let list = List::new(items);
        let mut state = ListState::default();
        state.select(Some(self.cursor));
        frame.render_stateful_widget(list, chunks[0], &mut state);

        frame.render_widget(Paragraph::new(footer_line(&self.renaming)), chunks[1]);
    }
}

fn channel_row<'a>(
    channel: &Channel,
    app: &App,
    focused: bool,
    renaming_buffer: Option<&str>,
) -> ListItem<'a> {
    let row_style = if focused {
        Style::default().fg(Color::White).bg(Color::DarkGray)
    } else {
        Style::default()
    };

    let armed_marker = if channel.armed { "[●]" } else { "[ ]" };
    let label_text = match renaming_buffer {
        Some(buf) => format!("✏  {}", buf),
        None => channel.label.clone().unwrap_or_else(|| "—".to_string()),
    };

    let mut spans = vec![Span::styled(
        format!(
            "{} Ch {:>2}  {:<16}  ",
            armed_marker, channel.index, label_text
        ),
        row_style,
    )];

    let level = app.display_levels[channel.index as usize];
    spans.extend(horizontal_meter(level, None, METER_WIDTH));

    ListItem::new(Line::from(spans))
}

fn footer_line(renaming: &Option<String>) -> Line<'static> {
    if let Some(buf) = renaming {
        Line::from(vec![
            Span::styled("Renaming: ", Style::default().fg(Color::Yellow)),
            Span::raw(buf.clone()),
            Span::styled(
                "_",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::SLOW_BLINK),
            ),
            Span::raw("    "),
            Span::styled("[Enter]", Style::default().fg(Color::Cyan)),
            Span::raw(" save  "),
            Span::styled("[Esc]", Style::default().fg(Color::DarkGray)),
            Span::raw(" cancel"),
        ])
    } else {
        let mut spans = Vec::new();
        spans.extend(key_hint("Space", "arm  ", Color::Cyan));
        spans.extend(key_hint("Tab", "rename  ", Color::Cyan));
        spans.extend(key_hint("Esc", "close", Color::DarkGray));
        Line::from(spans)
    }
}
