use chrono::Local;
use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

use crate::app::App;
use crate::ui::view::View;
use crate::ui::widgets::key_hint_when;

/// Width reserved for the right column. Sized for the hints line; flash
/// messages sit in the same column when active.
const RIGHT_COL_WIDTH: u16 = 50;

pub fn draw(frame: &mut Frame, area: Rect, app: &App, view: &View) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Fill(1), Constraint::Length(RIGHT_COL_WIDTH)])
        .split(area);

    frame.render_widget(Paragraph::new(status_line(app)), chunks[0]);
    let right = flash_line(view).unwrap_or_else(|| hints_line(app, view));
    frame.render_widget(Paragraph::new(right).right_aligned(), chunks[1]);
}

fn hints_line(app: &App, view: &View) -> Line<'static> {
    let actionable = !app.is_recording()
        && view.take_naming().is_none()
        && !view.confirm_stop_active()
        && !view.confirm_quit_active();
    let mut spans = Vec::new();
    spans.extend(key_hint_when(
        actionable,
        "Ctrl+S/L",
        "save/load template  ",
        Color::Cyan,
    ));
    spans.extend(key_hint_when(actionable, ",", "settings", Color::Cyan));
    Line::from(spans)
}

fn status_line(app: &App) -> Line<'static> {
    let mut spans = state_spans(app);
    spans.extend(separator());
    spans.extend(session_duration_spans(app));
    if let Some(folder) = recording_folder(app) {
        spans.extend(separator());
        spans.push(Span::raw(folder));
    }
    Line::from(spans)
}

fn state_spans(app: &App) -> Vec<Span<'static>> {
    let Some(rec) = &app.recording else {
        return vec![Span::styled(
            "○ Idle",
            Style::default().fg(Color::DarkGray),
        )];
    };
    let elapsed = rec.elapsed_secs();
    if app.is_recording() {
        let since = rec.since_last_marker_secs(app.engine.sample_position(), app.engine.sample_rate());
        vec![
            Span::styled(
                format!("● {:02}:{:02}", elapsed / 60, elapsed % 60),
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("  ▌ {:02}:{:02}", since / 60, since % 60),
                Style::default().fg(Color::DarkGray),
            ),
        ]
    } else {
        vec![Span::styled(
            format!("■ {:02}:{:02}", elapsed / 60, elapsed % 60),
            Style::default().fg(Color::DarkGray),
        )]
    }
}

fn separator() -> Vec<Span<'static>> {
    vec![Span::styled("  •  ", Style::default().fg(Color::DarkGray))]
}

fn session_duration_spans(app: &App) -> Vec<Span<'static>> {
    let secs = (Local::now() - app.session.started_at)
        .num_seconds()
        .max(0) as u64;
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
    app.recording
        .as_ref()
        .and_then(|r| r.output_dir.file_name())
        .map(|n| n.to_string_lossy().into_owned())
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
    Line::from(vec![
        Span::styled(" ✓ ", Style::default().fg(Color::Green)),
        Span::styled(
            format!("{} '{}' ", verb, name),
            Style::default().fg(Color::Green),
        ),
    ])
}
