use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Padding, Paragraph};

use crate::app::App;
use crate::channel::Channel;
use crate::ui::widgets::vertical_meter;

const STRIP_WIDTH: u16 = 14;

pub fn draw(frame: &mut Frame, area: Rect, app: &App) {
    let channels = app.session.channels();
    let armed_count = channels.iter().filter(|c| c.armed).count();
    let total = app.engine.channel_count();
    let title = format!(" Channels — {}/{} armed ", armed_count, total);
    let hint = Line::from(vec![
        Span::styled("[C]", Style::default().fg(Color::Cyan)),
        Span::raw(" select "),
    ])
    .right_aligned();
    let block = Block::default()
        .title(title)
        .title(hint)
        .borders(Borders::ALL)
        .padding(Padding::new(2, 2, 1, 1))
        .border_style(Style::default().fg(Color::DarkGray));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if channels.is_empty() {
        return;
    }

    let constraints: Vec<Constraint> = std::iter::repeat_n(Constraint::Max(STRIP_WIDTH), channels.len())
        .chain(std::iter::once(Constraint::Fill(1)))
        .collect();
    let strips = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(constraints)
        .split(inner);

    let meter_width = compute_meter_width(strips[0].width);

    for (i, channel) in channels.iter().enumerate() {
        if channel.armed {
            channel_strip(frame, strips[i], channel, app, meter_width);
        } else {
            unarmed_strip(frame, strips[i], channel);
        }
    }
}

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

fn channel_strip(frame: &mut Frame, area: Rect, channel: &Channel, app: &App, meter_width: usize) {
    let chunks = strip_chunks(area);

    let i = channel.index as usize;
    let level = app.display_levels[i];
    let peak = app.peak_holds[i];
    let lines = vertical_meter(level, Some(peak), meter_width, chunks[0].height as usize);
    let meter = Paragraph::new(lines).alignment(Alignment::Center);
    frame.render_widget(meter, chunks[0]);

    let header = Paragraph::new(format!("Ch {:>2}", channel.index)).alignment(Alignment::Center);
    frame.render_widget(header, chunks[1]);

    let label_text = channel.label.as_deref().unwrap_or("—");
    let label = Paragraph::new(label_text.to_string())
        .style(Style::default().fg(Color::DarkGray))
        .alignment(Alignment::Center);
    frame.render_widget(label, chunks[2]);
}

fn unarmed_strip(frame: &mut Frame, area: Rect, channel: &Channel) {
    let chunks = strip_chunks(area);

    let dim = Style::default().fg(Color::DarkGray);
    let header = Paragraph::new(format!("Ch {:>2}", channel.index))
        .style(dim)
        .alignment(Alignment::Center);
    frame.render_widget(header, chunks[1]);

    let label_text = channel.label.as_deref().unwrap_or("—");
    let label = Paragraph::new(label_text.to_string())
        .style(dim)
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
