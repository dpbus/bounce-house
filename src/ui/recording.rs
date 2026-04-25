use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::time::Instant;

use chrono::Local;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::audio_interface::AudioInterface;
use crate::capture::{self, CaptureHandle};
use crate::ui::Action;
use crate::ui::widgets::{key_hint, vertical_meter};

const FAST_DECAY: f32 = 0.976;
const SLOW_DECAY: f32 = 0.990;

const METER_WIDTH: usize = 2;
const METER_MAX_HEIGHT: u16 = 20;

pub struct RecordingState {
    pub interface: AudioInterface,
    pub channels: Vec<u8>,
    pub output_path: PathBuf,
    pub started_at: Instant,
    capture: CaptureHandle,
    display_levels: Vec<f32>,
    peak_holds: Vec<f32>,
    confirming_stop: bool,
}

impl Drop for RecordingState {
    fn drop(&mut self) {
        self.capture.running.store(false, Ordering::Relaxed);
        if let Some(handle) = self.capture.writer_handle.take() {
            let _ = handle.join();
        }
    }
}

impl RecordingState {
    pub fn new(interface: AudioInterface, channels: Vec<u8>) -> Self {
        let timestamp = Local::now().format("%Y-%m-%d-%H%M%S");
        let output_path = PathBuf::from(format!("recording-{}.wav", timestamp));

        let num_channels = channels.len();
        let capture = capture::start(&interface, &channels, &output_path);

        RecordingState {
            interface,
            channels,
            output_path,
            started_at: Instant::now(),
            capture,
            display_levels: vec![0.0; num_channels],
            peak_holds: vec![0.0; num_channels],
            confirming_stop: false,
        }
    }

    pub fn update_display(&mut self) {
        for (i, cl) in self.capture.levels.iter().enumerate() {
            let current = cl.current();
            self.display_levels[i] = current.max(self.display_levels[i] * FAST_DECAY);
            self.peak_holds[i] = current.max(self.peak_holds[i] * SLOW_DECAY);
        }
    }
}

pub fn draw(frame: &mut Frame, state: &RecordingState) {
    let block = recording_block(state);
    let inner = block.inner(frame.area());
    frame.render_widget(block, frame.area());

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(5),       // meter strips
            Constraint::Length(1),    // spacer
            Constraint::Length(1),    // path
            Constraint::Length(1),    // footer
        ])
        .split(inner);

    draw_meter_strips(frame, chunks[0], state);
    frame.render_widget(path_line(state), chunks[2]);
    frame.render_widget(footer_line(state), chunks[3]);
}

fn recording_block(state: &RecordingState) -> Block<'static> {
    let elapsed = state.started_at.elapsed().as_secs();
    let title = format!(
        " ● Recording — {} — {:02}:{:02} ",
        state.interface.name(),
        elapsed / 60,
        elapsed % 60,
    );
    Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Red))
}

fn path_line(state: &RecordingState) -> Paragraph<'static> {
    Paragraph::new(Line::from(vec![
        Span::styled("Saving to: ", Style::default().fg(Color::DarkGray)),
        Span::raw(state.output_path.display().to_string()),
    ]))
}

fn footer_line(state: &RecordingState) -> Paragraph<'static> {
    let mut spans = Vec::new();
    if state.confirming_stop {
        spans.push(Span::styled(
            "Stop recording?  ",
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        ));
        spans.extend(key_hint("Esc", "yes  ", Color::Cyan));
        spans.extend(key_hint("any other key", "no", Color::DarkGray));
    } else {
        spans.extend(key_hint("Esc", "stop and save  ", Color::Cyan));
        spans.extend(key_hint("Space", "mark take (coming soon)", Color::DarkGray));
    }
    Paragraph::new(Line::from(spans))
}

fn draw_meter_strips(frame: &mut Frame, area: Rect, state: &RecordingState) {
    let n = state.channels.len();
    if n == 0 {
        return;
    }

    let strips = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(vec![Constraint::Ratio(1, n as u32); n])
        .split(area);

    for (i, &ch) in state.channels.iter().enumerate() {
        channel_strip(
            frame,
            strips[i],
            ch,
            state.display_levels[i],
            state.peak_holds[i],
        );
    }
}

fn channel_strip(frame: &mut Frame, area: Rect, channel: u8, level: f32, peak_hold: f32) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Max(METER_MAX_HEIGHT)])
        .split(area);

    let header = Paragraph::new(format!("Ch {:>2}", channel)).alignment(Alignment::Center);
    frame.render_widget(header, chunks[0]);

    let lines = vertical_meter(level, Some(peak_hold), METER_WIDTH, chunks[1].height as usize);
    let meter = Paragraph::new(lines).alignment(Alignment::Center);
    frame.render_widget(meter, chunks[1]);
}

pub fn handle_input(state: &mut RecordingState, key: KeyEvent) -> Action {
    if state.confirming_stop {
        match key.code {
            KeyCode::Esc => Action::Quit,
            _ => {
                state.confirming_stop = false;
                Action::None
            }
        }
    } else {
        match key.code {
            KeyCode::Esc => {
                state.confirming_stop = true;
                Action::None
            }
            KeyCode::Char(' ') => {
                // TODO: mark take + open name modal
                Action::None
            }
            _ => Action::None,
        }
    }
}
