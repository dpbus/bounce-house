use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph};

use crate::app::{App, AppState};
use crate::channel::Channel;
use crate::ui::widgets::{horizontal_meter, key_hint};

const METER_WIDTH: usize = 30;

pub fn draw(frame: &mut Frame, app: &App) {
    let AppState::PickingChannel { cursor, renaming } = &app.state else {
        return;
    };

    let area = centered_rect(frame.area(), 80, 30);

    // Clear what was underneath, draw the modal block.
    frame.render_widget(Clear, area);
    let block = Block::default()
        .title(" Channels ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(3),       // channel list
            Constraint::Length(1),    // footer
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

    let list = List::new(items);
    frame.render_widget(list, chunks[0]);

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
            armed_marker, channel.index.0, label_text
        ),
        row_style,
    )];

    let level = app.display_levels[channel.index.as_usize()];
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
                Style::default().fg(Color::Yellow).add_modifier(Modifier::SLOW_BLINK),
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

fn centered_rect(area: Rect, percent_x: u16, percent_y: u16) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
