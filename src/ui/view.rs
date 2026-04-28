use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Padding};

use crate::app::App;
use crate::ui::footer;
use crate::ui::panels::{meters, recording, session, waveform};

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
            Constraint::Length(TOP_BAR_HEIGHT), // session + recording panels
            Constraint::Length(GAP),
            Constraint::Length(WAVEFORM_HEIGHT), // waveform panel
            Constraint::Length(GAP),
            Constraint::Fill(1), // meter strips fill remaining space
            Constraint::Length(GAP),
            Constraint::Length(1), // footer (key hints)
        ])
        .split(inner);

    let top_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .spacing(2)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(chunks[0]);

    session::draw(frame, top_chunks[0], app);
    recording::draw(frame, top_chunks[1], app);
    waveform::draw(frame, chunks[2], app);
    meters::draw(frame, chunks[4], app);
    footer::draw(frame, chunks[6], app);
}

fn outer_block(app: &App) -> Block<'static> {
    let (title, color) = if app.is_recording() {
        (
            format!(" ● Recording — {} ", app.engine.device_name()),
            Color::Red,
        )
    } else {
        (format!(" {} ", app.engine.device_name()), Color::Cyan)
    };
    Block::default()
        .title(title)
        .borders(Borders::ALL)
        .padding(Padding::new(2, 2, 1, 1))
        .border_style(Style::default().fg(color))
}
