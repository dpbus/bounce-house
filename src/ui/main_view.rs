use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::app::{App, AppState};
use crate::ui::widgets::{key_hint, vertical_meter};

const METER_WIDTH: usize = 2;
const METER_MAX_HEIGHT: u16 = 20;

pub fn draw(frame: &mut Frame, app: &App) {
    let block = outer_block(app);
    let inner = block.inner(frame.area());
    frame.render_widget(block, frame.area());

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(5),       // meter strips
            Constraint::Length(1),    // spacer
            Constraint::Length(1),    // status line
            Constraint::Length(1),    // footer (keys)
        ])
        .split(inner);

    draw_meter_strips(frame, chunks[0], app);
    frame.render_widget(Paragraph::new(status_line(app)), chunks[2]);
    frame.render_widget(Paragraph::new(footer_line(app)), chunks[3]);
}

fn outer_block(app: &App) -> Block<'static> {
    let (title, color) = match &app.state {
        AppState::Recording { .. } => (
            format!(" ● Recording — {} ", app.engine.name()),
            Color::Red,
        ),
        _ => (
            format!(" {} ", app.engine.name()),
            Color::Cyan,
        ),
    };
    Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(color))
}

fn draw_meter_strips(frame: &mut Frame, area: Rect, app: &App) {
    let armed: Vec<&crate::channel::Channel> = app.session.armed().collect();
    if armed.is_empty() {
        let msg = Paragraph::new(Line::from(vec![
            Span::styled("No channels armed.  ", Style::default().fg(Color::DarkGray)),
            Span::styled("[c]", Style::default().fg(Color::Cyan)),
            Span::raw(" open channel picker to arm channels"),
        ]))
        .alignment(Alignment::Center);
        frame.render_widget(msg, area);
        return;
    }

    let n = armed.len();
    let strips = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(vec![Constraint::Ratio(1, n as u32); n])
        .split(area);

    for (i, channel) in armed.iter().enumerate() {
        channel_strip(frame, strips[i], channel, app);
    }
}

fn channel_strip(frame: &mut Frame, area: Rect, channel: &crate::channel::Channel, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),                 // channel number
            Constraint::Length(1),                 // label
            Constraint::Max(METER_MAX_HEIGHT),     // meter
        ])
        .split(area);

    let header = Paragraph::new(format!("Ch {:>2}", channel.index.0))
        .alignment(Alignment::Center);
    frame.render_widget(header, chunks[0]);

    let label_text = channel.label.as_deref().unwrap_or("—");
    let label = Paragraph::new(label_text.to_string())
        .style(Style::default().fg(Color::DarkGray))
        .alignment(Alignment::Center);
    frame.render_widget(label, chunks[1]);

    let i = channel.index.as_usize();
    let level = app.display_levels[i];
    let peak = app.peak_holds[i];
    let lines = vertical_meter(level, Some(peak), METER_WIDTH, chunks[2].height as usize);
    let meter = Paragraph::new(lines).alignment(Alignment::Center);
    frame.render_widget(meter, chunks[2]);
}

fn status_line(app: &App) -> Line<'static> {
    match &app.state {
        AppState::Idle => Line::from(vec![
            Span::styled("Idle ", Style::default().fg(Color::DarkGray)),
            Span::raw("— "),
            Span::raw(format!("{} channels armed", app.session.armed().count())),
        ]),
        AppState::Recording { started_at, recording, .. } => {
            let elapsed = started_at.elapsed().as_secs();
            Line::from(vec![
                Span::styled(
                    format!("● {:02}:{:02} ", elapsed / 60, elapsed % 60),
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                ),
                Span::raw("— "),
                Span::styled(
                    recording.output_path().display().to_string(),
                    Style::default().fg(Color::DarkGray),
                ),
            ])
        }
        AppState::PickingChannel { .. } => {
            Line::from(Span::styled(
                "Channel picker open",
                Style::default().fg(Color::DarkGray),
            ))
        }
    }
}

fn footer_line(app: &App) -> Line<'static> {
    let mut spans = Vec::new();
    match &app.state {
        AppState::Idle => {
            spans.extend(key_hint("R", "record  ", Color::Cyan));
            spans.extend(key_hint("C", "channels  ", Color::Cyan));
            spans.extend(key_hint("Q", "quit", Color::DarkGray));
        }
        AppState::Recording { confirming_stop: false, .. } => {
            spans.extend(key_hint("Esc", "stop  ", Color::Cyan));
            spans.extend(key_hint("Space", "mark take (coming soon)", Color::DarkGray));
        }
        AppState::Recording { confirming_stop: true, .. } => {
            spans.push(Span::styled(
                "Stop recording?  ",
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
            ));
            spans.extend(key_hint("Esc", "yes  ", Color::Cyan));
            spans.extend(key_hint("any other key", "no", Color::DarkGray));
        }
        AppState::PickingChannel { .. } => {
            spans.extend(key_hint("Esc", "close picker", Color::Cyan));
        }
    }
    Line::from(spans)
}
