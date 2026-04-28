use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

use crate::app::App;
use crate::ui::view::View;
use crate::ui::widgets::{key_hint, key_hint_when};

/// `[,] settings` — width reserved on the right of the footer.
const SETTINGS_HINT_WIDTH: u16 = 12;

pub fn draw(frame: &mut Frame, area: Rect, app: &App, view: &View) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Fill(1),
            Constraint::Length(SETTINGS_HINT_WIDTH),
        ])
        .split(area);
    frame.render_widget(Paragraph::new(left(app, view)), chunks[0]);
    let settings_spans = key_hint_when(
        settings_available(app, view),
        ",",
        "settings",
        Color::Cyan,
    );
    frame.render_widget(
        Paragraph::new(Line::from(settings_spans)).right_aligned(),
        chunks[1],
    );
}

fn settings_available(app: &App, view: &View) -> bool {
    !app.is_recording()
        && view.take_naming().is_none()
        && !view.confirm_stop_active()
        && !view.confirm_quit_active()
}

fn left(app: &App, view: &View) -> Line<'static> {
    if view.take_naming().is_some() {
        return Line::from(Span::styled(
            "Naming take",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ));
    }
    if view.confirm_stop_active() {
        let mut spans = vec![Span::styled(
            "Stop recording?  ",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )];
        spans.extend(key_hint("Enter", "yes  ", Color::Cyan));
        spans.extend(key_hint("any other key", "no", Color::DarkGray));
        return Line::from(spans);
    }
    if view.confirm_quit_active() {
        let mut spans = vec![Span::styled(
            "Quit?  ",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )];
        spans.extend(key_hint("Enter", "yes  ", Color::Cyan));
        spans.extend(key_hint("any other key", "no", Color::DarkGray));
        return Line::from(spans);
    }
    let mut spans = Vec::new();
    if app.is_recording() {
        let last_unbound = app.has_unbound_marker();
        spans.extend(key_hint("T", "take  ", Color::Cyan));
        spans.extend(key_hint("Space", "mark  ", Color::Cyan));
        spans.extend(key_hint_when(
            last_unbound,
            "Backspace",
            "unmark  ",
            Color::Cyan,
        ));
        spans.extend(key_hint_when(last_unbound, "N", "name take  ", Color::Cyan));
        spans.extend(key_hint("Esc", "stop", Color::Cyan));
    } else {
        spans.extend(key_hint("R", "record  ", Color::Cyan));
        spans.extend(key_hint("C", "channels  ", Color::Cyan));
        spans.extend(key_hint_when(
            app.has_unbound_marker(),
            "N",
            "name take  ",
            Color::Cyan,
        ));
        spans.extend(key_hint("Q", "quit", Color::Cyan));
    }
    Line::from(spans)
}
