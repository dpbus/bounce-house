use ratatui::prelude::*;
use ratatui::widgets::{List, ListItem, ListState, Paragraph};

use crate::app::{App, AppState};
use crate::channel::Channel;
use crate::ui::widgets::{horizontal_meter, key_hint, modal, MODAL_BORDER_OVERHEAD};

const METER_WIDTH: usize = 30;
const WIDTH_PCT: u16 = 80;
const HEIGHT_PCT: u16 = 30;

pub fn draw(frame: &mut Frame, app: &App) {
    let AppState::PickingChannel { cursor, renaming } = &app.state else {
        return;
    };

    let frame_area = frame.area();
    let content_width = (frame_area.width * WIDTH_PCT / 100).saturating_sub(MODAL_BORDER_OVERHEAD);
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
            let focused = i == *cursor;
            let renaming_buf = if focused { renaming.as_deref() } else { None };
            channel_row(channel, app, focused, renaming_buf)
        })
        .collect();

    // Stateful render with cursor pre-selected so ratatui auto-adjusts the
    // viewport offset to keep it visible. Manual row highlighting is
    // unaffected because we don't set highlight_style.
    let list = List::new(items);
    let mut state = ListState::default();
    state.select(Some(*cursor));
    frame.render_stateful_widget(list, chunks[0], &mut state);

    frame.render_widget(Paragraph::new(footer_line(renaming)), chunks[1]);
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

