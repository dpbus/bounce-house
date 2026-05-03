use chrono::Local;
use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

use crate::app::App;
use crate::ui::view::View;
use crate::ui::widgets::truncate_with_ellipsis;

/// Width reserved for the right column where flash messages render.
const RIGHT_COL_WIDTH: u16 = 36;

pub fn draw(frame: &mut Frame, area: Rect, app: &App, view: &View) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Fill(1), Constraint::Length(RIGHT_COL_WIDTH)])
        .split(area);

    frame.render_widget(Paragraph::new(status_line(app, view)), chunks[0]);
    if let Some(line) = flash_line(view) {
        frame.render_widget(Paragraph::new(line).right_aligned(), chunks[1]);
    }
}

fn status_line(app: &App, view: &View) -> Line<'static> {
    let mut spans = session_duration_spans(view);
    if let Some(folder) = recording_folder(app) {
        spans.extend(separator());
        spans.push(Span::raw(folder));
    }
    Line::from(spans)
}

fn separator() -> Vec<Span<'static>> {
    vec![Span::styled("  •  ", Style::default().fg(Color::DarkGray))]
}

fn session_duration_spans(view: &View) -> Vec<Span<'static>> {
    let secs = (Local::now() - view.started_at).num_seconds().max(0) as u64;
    vec![
        Span::styled("Session ", Style::default().fg(Color::DarkGray)),
        Span::raw(format!(
            "{:02}:{:02}:{:02}",
            secs / 3600,
            (secs % 3600) / 60,
            secs % 60
        )),
    ]
}

fn recording_folder(app: &App) -> Option<String> {
    app.session
        .has_recording()
        .then(|| app.session.name.clone())
}

fn flash_line(view: &View) -> Option<Line<'static>> {
    if let Some(name) = view.recent_template_save() {
        return Some(flash_text("saved", name));
    }
    if let Some(name) = view.recent_template_load() {
        return Some(flash_text("loaded", name));
    }
    None
}

fn flash_text(verb: &str, name: &str) -> Line<'static> {
    const NAME_MAX: usize = 22;
    Line::from(vec![
        Span::styled(" ✓ ", Style::default().fg(Color::Green)),
        Span::styled(
            format!("{} '{}' ", verb, truncate_with_ellipsis(name, NAME_MAX)),
            Style::default().fg(Color::Green),
        ),
    ])
}
