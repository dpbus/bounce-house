use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Padding, Paragraph};

use crate::app::App;
use crate::channel::Channel;
use crate::ui::widgets::{key_hint, vertical_meter};

const STRIP_WIDTH: u16 = 14;
const METER_WIDTH: usize = 3;

pub fn draw(frame: &mut Frame, area: Rect, app: &App) {
    let channels: Vec<&Channel> = app.session.channels().iter().filter(|c| c.armed).collect();
    let total = app.engine.channel_count();

    let block = Block::default()
        .borders(Borders::ALL)
        .padding(Padding::new(2, 2, 1, 1))
        .border_style(Style::default().fg(Color::DarkGray));
    let inner = block.inner(area);

    // When the list overflows the panel, reserve 1 col on each side
    // for the scroll chevrons so strips and chevrons never collide.
    let needs_scroll = channels.len() > (inner.width / STRIP_WIDTH) as usize;
    let strip_area = if needs_scroll {
        Rect::new(
            inner.x + 1,
            inner.y,
            inner.width.saturating_sub(2),
            inner.height,
        )
    } else {
        inner
    };

    let capacity = (strip_area.width / STRIP_WIDTH) as usize;
    app.last_strip_capacity.set(capacity);
    let max_offset = channels.len().saturating_sub(capacity);
    let offset = app.channel_viewport_offset.min(max_offset);
    let end = (offset + capacity).min(channels.len());
    let off_left = offset;
    let off_right = channels.len().saturating_sub(end);

    let title = format!(" Channels — {}/{} armed ", channels.len(), total);
    frame.render_widget(block.title(title).title(title_hint()), area);

    if off_left > 0 {
        draw_edge_chevrons(frame, inner.x, inner, "◀");
    }
    if off_right > 0 {
        draw_edge_chevrons(frame, inner.x + inner.width - 1, inner, "▶");
    }

    let visible = &channels[offset..end];
    let constraints: Vec<Constraint> =
        std::iter::repeat_n(Constraint::Length(STRIP_WIDTH), visible.len()).collect();
    let strips = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(constraints)
        .split(strip_area);

    for (i, channel) in visible.iter().enumerate() {
        channel_strip(frame, strips[i], channel, app);
    }
}

fn title_hint() -> Line<'static> {
    let mut spans = Vec::new();
    spans.extend(key_hint("C", "select  ", Color::Cyan));
    spans.extend(key_hint("[]", "scroll ", Color::Cyan));
    Line::from(spans).right_aligned()
}

/// Marks the top and bottom rows of a 1-col gutter with a chevron
/// glyph. Cheaper than rendering a Paragraph into two 1×1 rects.
fn draw_edge_chevrons(frame: &mut Frame, x: u16, inner: Rect, glyph: &str) {
    let style = Style::default().fg(Color::DarkGray);
    let buf = frame.buffer_mut();
    let bottom = inner.y + inner.height.saturating_sub(1);
    buf.set_string(x, inner.y, glyph, style);
    buf.set_string(x, bottom, glyph, style);
}

fn channel_strip(frame: &mut Frame, area: Rect, channel: &Channel, app: &App) {
    let chunks = strip_chunks(area);
    let i = channel.index as usize;

    let level = app.display_levels[i];
    let peak = app.peak_holds[i];
    let lines = vertical_meter(
        level,
        Some(peak),
        METER_WIDTH,
        chunks[0].height as usize,
        !channel.armed,
    );
    frame.render_widget(
        Paragraph::new(lines).alignment(Alignment::Center),
        chunks[0],
    );

    let (glyph, glyph_color) = if channel.armed {
        ("●", Color::Red)
    } else {
        ("○", Color::DarkGray)
    };
    let name_style = if channel.armed {
        Style::default()
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let header = Line::from(vec![
        Span::styled(glyph, Style::default().fg(glyph_color)),
        Span::raw(" "),
        Span::styled(format!("Ch {:>2}", channel.index), name_style),
    ]);
    frame.render_widget(
        Paragraph::new(header).alignment(Alignment::Center),
        chunks[1],
    );

    let label_text = channel.label.as_deref().unwrap_or("—");
    let label = Paragraph::new(label_text.to_string())
        .style(Style::default().fg(Color::DarkGray))
        .alignment(Alignment::Center);
    frame.render_widget(label, chunks[2]);
}

fn strip_chunks(area: Rect) -> std::rc::Rc<[Rect]> {
    Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Fill(1),   // meter fills available height
            Constraint::Length(1), // channel number
            Constraint::Length(1), // label
        ])
        .split(area)
}
