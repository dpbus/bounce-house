use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

use crate::app::App;
use crate::recording::Recording;
use crate::ui::widgets::{dim_status, panel};

pub fn draw(frame: &mut Frame, area: Rect, app: &App) {
    let inner = panel(frame, area, "Recording", None, None);

    let Some(recording) = &app.recording else {
        frame.render_widget(Paragraph::new(dim_status("Idle")), inner);
        return;
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(1)])
        .split(inner);

    frame.render_widget(Paragraph::new(timer_line(app, recording)), chunks[0]);
    frame.render_widget(Paragraph::new(folder_line(recording)), chunks[1]);
}

fn timer_line(app: &App, recording: &Recording) -> Line<'static> {
    let elapsed = recording.elapsed_secs();
    let (glyph, style) = if app.is_recording() {
        (
            "●",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        )
    } else {
        ("■", Style::default().fg(Color::DarkGray))
    };
    let mut spans = vec![Span::styled(
        format!("{} {:02}:{:02}", glyph, elapsed / 60, elapsed % 60),
        style,
    )];
    if app.is_recording() {
        let since = recording
            .since_last_marker_secs(app.engine.sample_position(), app.engine.sample_rate());
        spans.push(Span::styled(
            format!("  ▌ {:02}:{:02}", since / 60, since % 60),
            Style::default().fg(Color::DarkGray),
        ));
    }
    Line::from(spans)
}

fn folder_line(recording: &Recording) -> Line<'static> {
    let dirname = recording
        .output_dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    Line::from(vec![
        Span::styled("Folder: ", Style::default().fg(Color::DarkGray)),
        Span::raw(dirname),
    ])
}
