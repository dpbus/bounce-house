use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

use crate::app::App;
use crate::recording::Recording;
use crate::timeline::BounceStatus;
use crate::ui::view::View;
use crate::ui::widgets::{
    dim_status, flow_columns, input_with_cursor, key_hint, panel, spinner_glyph, take_color,
};

pub fn draw(frame: &mut Frame, area: Rect, app: &App, view: &View) {
    let inner = panel(frame, area, "Recording", None, naming_hint(view));

    let Some(recording) = &app.recording else {
        frame.render_widget(Paragraph::new(dim_status("Idle")), inner);
        return;
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // header
            Constraint::Length(1), // blank
            Constraint::Length(1), // "Takes"
            Constraint::Fill(1),   // entries
        ])
        .split(inner);

    let header_row = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(17), Constraint::Fill(1)])
        .split(chunks[0]);
    frame.render_widget(Paragraph::new(timer_line(app, recording)), header_row[0]);
    frame.render_widget(Paragraph::new(folder_line(recording)), header_row[1]);

    let entries = take_entries(app, view);
    if entries.is_empty() {
        return;
    }
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "Takes",
            Style::default().fg(Color::DarkGray),
        ))),
        chunks[2],
    );
    let n_cols = if chunks[3].width >= 50 {
        3
    } else if chunks[3].width >= 32 {
        2
    } else {
        1
    };
    flow_columns(frame, chunks[3], &entries, n_cols);
}

fn naming_hint(view: &View) -> Option<Line<'static>> {
    view.take_naming().is_some().then(|| {
        let mut spans = vec![Span::raw(" ")];
        spans.extend(key_hint("Enter", "save  ", Color::Cyan));
        spans.extend(key_hint("Esc", "cancel", Color::DarkGray));
        spans.push(Span::raw(" "));
        Line::from(spans)
    })
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
    .right_aligned()
}

/// Lines for the Takes section: in-progress naming buffer first, then
/// named takes newest-first.
fn take_entries(app: &App, view: &View) -> Vec<Line<'static>> {
    let takes = app.current_timeline().map(|t| t.takes()).unwrap_or(&[]);
    let mut entries = Vec::new();

    if let Some(overlay) = view.take_naming() {
        let next_color = takes.last().map(|t| t.color_index + 1).unwrap_or(0);
        let color = take_color(next_color as usize);
        let mut spans = vec![Span::styled("▌ ", Style::default().fg(color))];
        spans.extend(input_with_cursor(overlay.input(), Style::default()));
        entries.push(Line::from(spans));
    }

    let sample_rate = app.engine.sample_rate().0 as u64;
    for take in takes.iter().rev() {
        let color = take_color(take.color_index as usize);
        let secs = take.end_sample.saturating_sub(take.start_sample) / sample_rate;
        let mut spans = vec![
            Span::styled("▌ ", Style::default().fg(color)),
            Span::raw(take.name.clone()),
            Span::styled(
                format!(" ({}:{:02})", secs / 60, secs % 60),
                Style::default().fg(Color::DarkGray),
            ),
        ];
        spans.push(Span::raw(" "));
        spans.push(bounce_status_span(&take.bounce_status, app.total_ticks));
        entries.push(Line::from(spans));
    }

    entries
}

fn bounce_status_span(status: &BounceStatus, total_ticks: u64) -> Span<'static> {
    match status {
        BounceStatus::Pending => Span::styled("◌", Style::default().fg(Color::DarkGray)),
        BounceStatus::Bouncing => Span::styled(
            spinner_glyph(total_ticks),
            Style::default().fg(Color::White),
        ),
        BounceStatus::Done(_) => Span::styled("✓", Style::default().fg(Color::Green)),
        BounceStatus::Failed(_) => Span::styled("✗", Style::default().fg(Color::Red)),
    }
}
