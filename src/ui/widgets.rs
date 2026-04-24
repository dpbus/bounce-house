use ratatui::prelude::*;

fn to_db(level: f32) -> f32 {
    if level < 0.0001 {
        -80.0
    } else {
        20.0 * level.log10()
    }
}

pub fn meter_spans(level: f32, width: usize) -> Vec<Span<'static>> {
    let db = to_db(level);
    let normalized = ((db + 60.0) / 60.0).clamp(0.0, 1.0);
    let filled = (normalized * width as f32).ceil().min(width as f32) as usize;

    let bar_color = if db > -1.0 {
        Color::Red
    } else if db > -6.0 {
        Color::Yellow
    } else if db > -60.0 {
        Color::Green
    } else {
        Color::DarkGray
    };

    let filled_str: String = "█".repeat(filled);
    let empty_str: String = " ".repeat(width.saturating_sub(filled));

    vec![
        Span::raw("│"),
        Span::styled(filled_str, Style::default().fg(bar_color)),
        Span::raw(empty_str),
        Span::raw("│"),
    ]
}
