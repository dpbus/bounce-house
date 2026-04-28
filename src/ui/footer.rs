use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

use crate::app::{App, AppState};
use crate::ui::widgets::{key_hint, key_hint_when};

pub fn draw(frame: &mut Frame, area: Rect, app: &App) {
    frame.render_widget(Paragraph::new(line(app)), area);
}

fn line(app: &App) -> Line<'static> {
    let mut spans = Vec::new();
    match &app.state {
        AppState::NamingTake { .. } => {
            return Line::from(Span::styled(
                "Naming take",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ));
        }
        AppState::ConfirmingStop => {
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
        AppState::Default if app.is_recording() => {
            let last_unbound = app
                .current_timeline()
                .is_some_and(|t| t.last_marker_unbound());
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
        }
        AppState::Default => {
            spans.extend(key_hint("R", "record  ", Color::Cyan));
            spans.extend(key_hint("C", "channels  ", Color::Cyan));
            if let Some(timeline) = app.current_timeline() {
                spans.extend(key_hint_when(
                    timeline.last_marker_unbound(),
                    "N",
                    "name take  ",
                    Color::Cyan,
                ));
            }
            spans.extend(key_hint("Q", "quit", Color::DarkGray));
        }
    }
    Line::from(spans)
}
