use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

use crate::app::App;
use crate::ui::view::View;
use crate::ui::widgets::{key_hint, key_hint_when};

const RIGHT_HINTS_WIDTH: u16 = 20;

pub fn draw(frame: &mut Frame, area: Rect, app: &App, view: &View) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Fill(1), Constraint::Length(RIGHT_HINTS_WIDTH)])
        .split(area);
    frame.render_widget(Paragraph::new(left(app, view)), chunks[0]);
    frame.render_widget(
        Paragraph::new(right(app, view)).right_aligned(),
        chunks[1],
    );
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
        return confirm_overlay_line("Stop recording?");
    }
    if view.confirm_quit_active() {
        return confirm_overlay_line("Quit?");
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
        spans.extend(key_hint_when(
            app.session.armed().next().is_some(),
            "R",
            "record  ",
            Color::Cyan,
        ));
        spans.extend(key_hint("C", "channels  ", Color::Cyan));
        spans.extend(key_hint_when(
            app.has_unbound_marker(),
            "N",
            "name take",
            Color::Cyan,
        ));
    }
    Line::from(spans)
}

fn right(app: &App, view: &View) -> Line<'static> {
    let quit_actionable = !app.is_recording()
        && view.take_naming().is_none()
        && !view.confirm_stop_active()
        && !view.confirm_quit_active();
    let mut spans = Vec::new();
    spans.extend(key_hint_when(quit_actionable, "Q", "quit  ", Color::Cyan));
    spans.extend(key_hint("?", "help", Color::Cyan));
    Line::from(spans)
}

fn confirm_overlay_line(prompt: &str) -> Line<'static> {
    let mut spans = vec![Span::styled(
        format!("{}  ", prompt),
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    )];
    spans.extend(key_hint("Enter", "yes  ", Color::Cyan));
    spans.extend(key_hint("any other key", "no", Color::DarkGray));
    Line::from(spans)
}
