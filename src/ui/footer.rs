use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

use crate::app::App;
use crate::ui::view::View;
use crate::ui::widgets::{key_hint, key_hint_when};

pub fn draw(frame: &mut Frame, area: Rect, app: &App, view: &View) {
    frame.render_widget(Paragraph::new(line(app, view)), area);
}

fn line(app: &App, view: &View) -> Line<'static> {
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
        spans.extend(key_hint("Esc", "yes  ", Color::Cyan));
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
        spans.extend(key_hint("Esc", "stop", Color::DarkGray));
    } else {
        spans.extend(key_hint("R", "record  ", Color::Cyan));
        spans.extend(key_hint("C", "channels  ", Color::Cyan));
        spans.extend(key_hint_when(
            app.has_unbound_marker(),
            "N",
            "name take  ",
            Color::Cyan,
        ));
        spans.extend(key_hint("Q", "quit", Color::DarkGray));
    }
    Line::from(spans)
}
