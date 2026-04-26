use chrono::Local;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Padding, Paragraph};

use crate::app::{App, AppState, TICK_FPS};
use crate::channel::Channel;
use crate::ui::waveform;
use crate::ui::widgets::{key_hint, take_color, vertical_meter};

const TOP_BAR_HEIGHT: u16 = 12;
const WAVEFORM_HEIGHT: u16 = 18;
const GAP: u16 = 1; // standard breathing room between sections

pub fn draw(frame: &mut Frame, app: &App) {
    let block = outer_block(app);
    let inner = block.inner(frame.area());
    frame.render_widget(block, frame.area());

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(TOP_BAR_HEIGHT),  // session + recording panels
            Constraint::Length(GAP),
            Constraint::Length(WAVEFORM_HEIGHT), // waveform panel
            Constraint::Length(GAP),
            Constraint::Fill(1),                  // meter strips fill remaining space
            Constraint::Length(GAP),
            Constraint::Length(1),                // footer (key hints)
        ])
        .split(inner);

    let top_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .spacing(2)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(chunks[0]);

    draw_session_panel(frame, top_chunks[0], app);
    draw_recording_panel(frame, top_chunks[1], app);
    waveform::draw(frame, chunks[2], app);
    draw_meter_strips(frame, chunks[4], app);
    frame.render_widget(Paragraph::new(footer_line(app)), chunks[6]);
}

fn outer_block(app: &App) -> Block<'static> {
    let (title, color) = match &app.state {
        AppState::Recording { .. } => (
            format!(" ● Recording — {} ", app.engine.name()),
            Color::Red,
        ),
        _ => (format!(" {} ", app.engine.name()), Color::Cyan),
    };
    Block::default()
        .title(title)
        .borders(Borders::ALL)
        .padding(Padding::new(2, 2, 1, 1))
        .border_style(Style::default().fg(color))
}

fn draw_session_panel(frame: &mut Frame, area: Rect, app: &App) {
    let inner = panel(frame, area, "Session");

    let duration = Local::now() - app.session.started_at;
    let secs = duration.num_seconds().max(0);
    let duration_text = format!(
        "{:02}:{:02}:{:02}",
        secs / 3600,
        (secs % 3600) / 60,
        secs % 60
    );

    let lines = vec![
        labeled("Device:   ", app.engine.name().to_string()),
        labeled(
            "Started:  ",
            app.session.started_at.format("%H:%M:%S").to_string(),
        ),
        labeled("Duration: ", duration_text),
        labeled(
            "Channels: ",
            format!(
                "{} armed / {}",
                app.session.armed().count(),
                app.engine.channel_count(),
            ),
        ),
        labeled("Output:   ", app.session.raw_dir.display().to_string()),
    ];
    frame.render_widget(Paragraph::new(lines), inner);
}

fn draw_recording_panel(frame: &mut Frame, area: Rect, app: &App) {
    let inner = panel(frame, area, "Recording");

    let lines = match &app.state {
        AppState::Idle => dim_status("Idle"),
        AppState::PickingChannel { .. } => dim_status("Channel picker open"),
        AppState::Recording {
            started_at,
            recording,
            ..
        } => {
            let elapsed = started_at.elapsed().as_secs();
            let dirname = recording
                .output_dir()
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let mut lines = vec![
                Line::from(Span::styled(
                    format!("● {:02}:{:02}", elapsed / 60, elapsed % 60),
                    Style::default()
                        .fg(Color::Red)
                        .add_modifier(Modifier::BOLD),
                )),
                Line::from(""),
                labeled("Folder: ", dirname),
            ];

            let take_name_buf = app.timeline.take_name_buf();
            let takes = app.timeline.takes();
            if !takes.is_empty() || take_name_buf.is_some() {
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    "Takes",
                    Style::default().fg(Color::DarkGray),
                )));
                // Naming buffer first, then named takes newest-first, so
                // overflow drops the oldest off the bottom.
                if let Some(buf) = take_name_buf {
                    let next_color = takes.last().map(|t| t.color_index + 1).unwrap_or(0);
                    let color = take_color(next_color as usize);
                    lines.push(Line::from(vec![
                        Span::styled("  ▌ ", Style::default().fg(color)),
                        Span::raw(buf.to_string()),
                        Span::styled(
                            "_",
                            Style::default()
                                .fg(Color::Yellow)
                                .add_modifier(Modifier::SLOW_BLINK),
                        ),
                    ]));
                }
                for take in takes.iter().rev() {
                    let color = take_color(take.color_index as usize);
                    let secs = take.end_tick.saturating_sub(take.start_tick) / TICK_FPS as u64;
                    lines.push(Line::from(vec![
                        Span::styled("  ▌ ", Style::default().fg(color)),
                        Span::raw(take.name.clone()),
                        Span::styled(
                            format!(" ({}:{:02})", secs / 60, secs % 60),
                            Style::default().fg(Color::DarkGray),
                        ),
                    ]));
                }
            }
            lines
        }
    };
    frame.render_widget(Paragraph::new(lines), inner);
}

fn panel(frame: &mut Frame, area: Rect, title: &'static str) -> Rect {
    let block = Block::default()
        .title(format!(" {} ", title))
        .borders(Borders::ALL)
        .padding(Padding::new(2, 2, 1, 1))
        .border_style(Style::default().fg(Color::DarkGray));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    inner
}

fn labeled(label: &'static str, value: String) -> Line<'static> {
    Line::from(vec![
        Span::styled(label, Style::default().fg(Color::DarkGray)),
        Span::raw(value),
    ])
}

fn dim_status(text: &'static str) -> Vec<Line<'static>> {
    vec![Line::from(Span::styled(
        text,
        Style::default().fg(Color::DarkGray),
    ))]
}

fn draw_meter_strips(frame: &mut Frame, area: Rect, app: &App) {
    let inner = panel(frame, area, "Meters");

    let armed: Vec<&Channel> = app.session.armed().collect();
    if armed.is_empty() {
        let msg = Paragraph::new(Line::from(vec![
            Span::styled("No channels armed.  ", Style::default().fg(Color::DarkGray)),
            Span::styled("[c]", Style::default().fg(Color::Cyan)),
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

fn footer_line(app: &App) -> Line<'static> {
    let mut spans = Vec::new();
    let naming_take = matches!(app.state, AppState::Recording { .. }) && app.timeline.is_naming_take();
    if naming_take {
        spans.push(Span::styled(
            "Naming take  ",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ));
        spans.extend(key_hint("Enter", "save  ", Color::Cyan));
        spans.extend(key_hint("Esc", "cancel", Color::DarkGray));
        return Line::from(spans);
    }
    match &app.state {
        AppState::Idle => {
            spans.extend(key_hint("R", "record  ", Color::Cyan));
            spans.extend(key_hint("C", "channels  ", Color::Cyan));
            spans.extend(key_hint("W", "waveform window  ", Color::Cyan));
            spans.extend(key_hint("Q", "quit", Color::DarkGray));
        }
        AppState::Recording {
            confirming_stop: false,
            ..
        } => {
            spans.extend(key_hint("T", "take  ", Color::Cyan));
            spans.extend(key_hint("Space", "mark  ", Color::Cyan));
            if app.timeline.can_delete_last_marker() {
                spans.extend(key_hint("Backspace", "unmark  ", Color::Cyan));
            } else {
                spans.push(Span::styled(
                    "[Backspace] unmark  ",
                    Style::default().fg(Color::DarkGray),
                ));
            }
            spans.extend(key_hint("N", "name last  ", Color::Cyan));
            spans.extend(key_hint("W", "window  ", Color::Cyan));
            spans.extend(key_hint("Esc", "stop", Color::DarkGray));
        }
        AppState::Recording {
            confirming_stop: true,
            ..
        } => {
            spans.push(Span::styled(
                "Stop recording?  ",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
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
