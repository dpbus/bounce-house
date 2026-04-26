use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

use crate::app::App;
use crate::channel::Channel;
use crate::ui::widgets::{panel, vertical_meter};

pub fn draw(frame: &mut Frame, area: Rect, app: &App) {
    let inner = panel(frame, area, "Meters", None);

    let armed: Vec<&Channel> = app.session.armed().collect();
    if armed.is_empty() {
        let msg = Paragraph::new(Line::from(vec![
            Span::styled("No channels armed.  ", Style::default().fg(Color::DarkGray)),
            Span::styled("[C]", Style::default().fg(Color::Cyan)),
            Span::raw(" open channel picker to arm channels"),
        ]))
        .alignment(Alignment::Center);
        frame.render_widget(msg, inner);
        return;
    }

    let n = armed.len();
    let strips = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(vec![Constraint::Ratio(1, n as u32); n])
        .split(inner);

    let meter_width = compute_meter_width(strips[0].width);

    for (i, channel) in armed.iter().enumerate() {
        channel_strip(frame, strips[i], channel, app, meter_width);
    }
}

/// Pick a sensible meter width given how wide each strip is.
fn compute_meter_width(strip_width: u16) -> usize {
    match strip_width {
        0..=5 => 1,
        6..=10 => 2,
        11..=18 => 3,
        19..=30 => 4,
        31..=50 => 6,
        _ => 8,
    }
}

fn channel_strip(
    frame: &mut Frame,
    area: Rect,
    channel: &Channel,
    app: &App,
    meter_width: usize,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Fill(1),     // meter fills available height
            Constraint::Length(1),   // channel number
            Constraint::Length(1),   // label
        ])
        .split(area);

    let i = channel.index.as_usize();
    let level = app.display_levels[i];
    let peak = app.peak_holds[i];
    let lines = vertical_meter(level, Some(peak), meter_width, chunks[0].height as usize);
    let meter = Paragraph::new(lines).alignment(Alignment::Center);
    frame.render_widget(meter, chunks[0]);

    let header = Paragraph::new(format!("Ch {:>2}", channel.index.0)).alignment(Alignment::Center);
    frame.render_widget(header, chunks[1]);

    let label_text = channel.label.as_deref().unwrap_or("—");
    let label = Paragraph::new(label_text.to_string())
        .style(Style::default().fg(Color::DarkGray))
        .alignment(Alignment::Center);
    frame.render_widget(label, chunks[2]);
}
