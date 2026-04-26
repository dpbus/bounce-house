use std::collections::VecDeque;

use chrono::Local;
use ratatui::prelude::*;
use ratatui::symbols::Marker;
use ratatui::widgets::canvas::{Canvas, Line as CanvasLine};
use ratatui::widgets::{Block, Borders, Padding, Paragraph};

use crate::app::{App, AppState, TICK_FPS};
use crate::channel::Channel;
use crate::ui::widgets::{
    BAND_GREEN, BAND_GREEN_DIM, BAND_RED, BAND_RED_DIM, BAND_YELLOW, BAND_YELLOW_DIM,
    band_thresholds, key_hint, linear_to_db_fraction, vertical_meter,
};

const TOP_BAR_HEIGHT: u16 = 12;
const WAVEFORM_HEIGHT: u16 = 16;
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
    draw_waveform(frame, chunks[2], app);
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
    let block = Block::default()
        .title(" Session ")
        .borders(Borders::ALL)
        .padding(Padding::new(2, 2, 1, 1))
        .border_style(Style::default().fg(Color::DarkGray));
    let inner = block.inner(area);
    frame.render_widget(block, area);

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
    let block = Block::default()
        .title(" Recording ")
        .borders(Borders::ALL)
        .padding(Padding::new(2, 2, 1, 1))
        .border_style(Style::default().fg(Color::DarkGray));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let lines = match &app.state {
        AppState::Idle => vec![
            Line::from(Span::styled(
                "Idle",
                Style::default().fg(Color::DarkGray),
            )),
        ],
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
            vec![
                Line::from(Span::styled(
                    format!("● {:02}:{:02}", elapsed / 60, elapsed % 60),
                    Style::default()
                        .fg(Color::Red)
                        .add_modifier(Modifier::BOLD),
                )),
                Line::from(""),
                labeled("Folder: ", dirname),
            ]
        }
        AppState::PickingChannel { .. } => vec![
            Line::from(Span::styled(
                "Channel picker open",
                Style::default().fg(Color::DarkGray),
            )),
        ],
    };
    frame.render_widget(Paragraph::new(lines), inner);
}

fn draw_waveform(frame: &mut Frame, area: Rect, app: &App) {
    let label = match app.waveform_window_secs {
        s if s < 60 => format!("{}s", s),
        s if s < 3600 => format!("{} min", s / 60),
        s => format!("{} hr", s / 3600),
    };
    let block = Block::default()
        .title(format!(" Waveform — {} window ", label))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let width = inner.width as usize;
    let height = inner.height as usize;
    if width == 0 || height < 2 {
        return;
    }

    // Braille markers pack 2x4 dots per cell — run at 2x horizontal resolution.
    let pixel_width = width * 2;
    let amps = waveform_amps(&app.level_history, app.waveform_window_secs, pixel_width);
    let (warn, clip) = band_thresholds();
    let (warn, clip) = (warn as f64, clip as f64);
    // 1 braille pixel of vertical extent — keeps the centerline visible
    // through silent recorded moments.
    let min_y = 1.0 / (height as f64 * 4.0);

    let canvas = Canvas::default()
        .marker(Marker::Braille)
        .x_bounds([0.0, pixel_width as f64])
        .y_bounds([-1.0, 1.0])
        .paint(move |ctx| {
            for (col, opt) in amps.iter().enumerate() {
                let Some((amp, recorded)) = opt else { continue };
                let half = (linear_to_db_fraction(*amp) as f64).max(min_y);
                let x = col as f64;
                let (green, yellow, red) = if *recorded {
                    (BAND_GREEN, BAND_YELLOW, BAND_RED)
                } else {
                    (BAND_GREEN_DIM, BAND_YELLOW_DIM, BAND_RED_DIM)
                };

                let g_top = half.min(warn);
                ctx.draw(&CanvasLine {
                    x1: x, y1: -g_top, x2: x, y2: g_top, color: green,
                });
                if half > warn {
                    let y_top = half.min(clip);
                    ctx.draw(&CanvasLine {
                        x1: x, y1: warn, x2: x, y2: y_top, color: yellow,
                    });
                    ctx.draw(&CanvasLine {
                        x1: x, y1: -y_top, x2: x, y2: -warn, color: yellow,
                    });
                }
                if half > clip {
                    ctx.draw(&CanvasLine {
                        x1: x, y1: clip, x2: x, y2: half, color: red,
                    });
                    ctx.draw(&CanvasLine {
                        x1: x, y1: -half, x2: x, y2: -clip, color: red,
                    });
                }
            }
        });
    frame.render_widget(canvas, inner);
}

/// `(amp, was_recording)` per pixel column. Buckets are anchored to absolute
/// history indices so each one is sealed once its time has passed. The
/// `was_recording` flag is true if any sample in the bucket was captured to
/// disk — used to render recorded portions at full brightness vs dim live
/// audio. `None` columns are pre-recording or empty future buckets.
fn waveform_amps(
    history: &VecDeque<(f32, bool)>,
    window_secs: u64,
    pixel_width: usize,
) -> Vec<Option<(f32, bool)>> {
    let visible_ticks = window_secs as usize * TICK_FPS;
    let bucket_size = (visible_ticks / pixel_width).max(1);
    let history_len = history.len();
    let latest_bucket = (history_len / bucket_size) as i64;
    let leftmost = latest_bucket - (pixel_width as i64 - 1);

    (0..pixel_width)
        .map(|col| {
            let bucket_idx = leftmost + col as i64;
            if bucket_idx < 0 {
                return None;
            }
            let start = bucket_idx as usize * bucket_size;
            let end = (start + bucket_size).min(history_len);
            if start >= end {
                return None;
            }
            let (amp, recorded) = history.range(start..end).fold(
                (0.0f32, false),
                |(amx, rec), &(a, r)| (amx.max(a), rec || r),
            );
            Some((amp, recorded))
        })
        .collect()
}

fn labeled(label: &'static str, value: String) -> Line<'static> {
    Line::from(vec![
        Span::styled(label, Style::default().fg(Color::DarkGray)),
        Span::raw(value),
    ])
}

fn draw_meter_strips(frame: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .title(" Meters ")
        .borders(Borders::ALL)
        .padding(Padding::new(2, 2, 1, 1))
        .border_style(Style::default().fg(Color::DarkGray));
    let inner = block.inner(area);
    frame.render_widget(block, area);

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
            spans.extend(key_hint("Esc", "stop  ", Color::Cyan));
            spans.extend(key_hint("W", "waveform window  ", Color::Cyan));
            spans.extend(key_hint("Space", "mark take (coming soon)", Color::DarkGray));
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
