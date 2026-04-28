use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

use crate::app::App;
use crate::timeline::BounceStatus;
use crate::ui::view::View;
use crate::ui::widgets::{
    dim_status, input_with_cursor, key_hint, panel, spinner_glyph, take_color,
    truncate_with_ellipsis,
};

pub fn draw(frame: &mut Frame, area: Rect, app: &App, view: &View) {
    let inner = panel(frame, area, "Timeline", None, naming_hint(view));

    let entries = take_entries(app, view);
    if entries.is_empty() {
        frame.render_widget(Paragraph::new(dim_status("No takes yet")), inner);
        return;
    }
    frame.render_widget(Paragraph::new(entries), inner);
}

fn naming_hint(view: &View) -> Option<Line<'static>> {
    view.take_naming().is_some().then(|| {
        let mut spans = vec![Span::raw(" ")];
        spans.extend(key_hint("Enter", "save  ", Color::Cyan));
        spans.extend(key_hint("Esc", "cancel", Color::Cyan));
        spans.push(Span::raw(" "));
        Line::from(spans)
    })
}

const NAME_WIDTH: usize = 14;

/// Named takes oldest-first, then the in-progress naming buffer at the
/// end (the next take being formed).
fn take_entries(app: &App, view: &View) -> Vec<Line<'static>> {
    let takes = app.current_timeline().map(|t| t.takes()).unwrap_or(&[]);
    let mut entries = Vec::new();

    let sample_rate = app.engine.sample_rate().0 as u64;
    for take in takes.iter() {
        let color = take_color(take.color_index as usize);
        let secs = take.end_sample.saturating_sub(take.start_sample) / sample_rate;
        let spans = vec![
            Span::styled("▌ ", Style::default().fg(color)),
            Span::raw(format!(
                "{:<width$}",
                truncate_with_ellipsis(&take.name, NAME_WIDTH),
                width = NAME_WIDTH
            )),
            Span::styled(
                format!("  {}:{:02}  ", secs / 60, secs % 60),
                Style::default().fg(Color::DarkGray),
            ),
            bounce_status_span(&take.bounce_status, app.total_ticks),
        ];
        entries.push(Line::from(spans));
    }

    if let Some(overlay) = view.take_naming() {
        let next_color = takes.last().map(|t| t.color_index + 1).unwrap_or(0);
        let color = take_color(next_color as usize);
        let mut spans = vec![Span::styled("▌ ", Style::default().fg(color))];
        spans.extend(input_with_cursor(overlay.input(), Style::default()));
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
