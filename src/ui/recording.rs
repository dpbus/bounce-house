use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::time::Instant;

use chrono::Local;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, List, ListItem};

use crate::audio_interface::AudioInterface;
use crate::capture::{self, CaptureHandle};
use crate::ui::Action;
use crate::ui::widgets::meter_spans;

const FAST_DECAY: f32 = 0.93;
const SLOW_DECAY: f32 = 0.97;

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
    let elapsed = state.started_at.elapsed().as_secs();
    let mins = elapsed / 60;
    let secs = elapsed % 60;

    let title = format!(
        " ● Recording — {} — {:02}:{:02} ",
        state.interface.name(),
        mins,
        secs,
    );

    let mut items: Vec<ListItem> = state
        .channels
        .iter()
        .enumerate()
        .map(|(i, &ch)| {
            let level = state.display_levels[i];
            let peak = state.peak_holds[i];
            let mut spans = vec![Span::raw(format!("Ch {:>2}  ", ch))];
            spans.extend(meter_spans(level, Some(peak), 30));
            ListItem::new(Line::from(spans))
        })
        .collect();

    items.push(ListItem::new(""));
    items.push(ListItem::new(Line::from(vec![
        Span::styled("Saving to: ", Style::default().fg(Color::DarkGray)),
        Span::raw(state.output_path.display().to_string()),
    ])));
    items.push(ListItem::new(""));

    if state.confirming_stop {
        items.push(ListItem::new(Line::from(vec![
            Span::styled(
                "Stop recording?  ",
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
            ),
            Span::styled("[Esc]", Style::default().fg(Color::Cyan)),
            Span::raw(" yes  "),
            Span::styled("[any other key]", Style::default().fg(Color::DarkGray)),
            Span::raw(" no"),
        ])));
    } else {
        items.push(ListItem::new(Line::from(vec![
            Span::styled("[Esc]", Style::default().fg(Color::Cyan)),
            Span::raw(" stop and save  "),
            Span::styled("[Space]", Style::default().fg(Color::DarkGray)),
            Span::raw(" mark take (coming soon)"),
        ])));
    }

    let list = List::new(items).block(
        Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Red)),
    );
    frame.render_widget(list, frame.area());
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
