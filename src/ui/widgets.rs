use ratatui::prelude::*;

fn to_db(level: f32) -> f32 {
    if level < 0.0001 {
        -80.0
    } else {
        20.0 * level.log10()
    }
}

fn db_to_position(db: f32, width: usize) -> usize {
    let normalized = ((db + 60.0) / 60.0).clamp(0.0, 1.0);
    (normalized * width as f32).ceil().min(width as f32) as usize
}

pub fn meter_spans(level: f32, peak_hold: Option<f32>, width: usize) -> Vec<Span<'static>> {
    let db = to_db(level);
    let filled = db_to_position(db, width);

    let bar_color = if db > -3.0 {
        Color::Red
    } else if db > -18.0 {
        Color::Yellow
    } else if db > -60.0 {
        Color::Green
    } else {
        Color::DarkGray
    };

    let mut spans = vec![
        Span::raw("│"),
        Span::styled("█".repeat(filled), Style::default().fg(bar_color)),
    ];

    let peak_pos = peak_hold
        .map(|p| db_to_position(to_db(p), width))
        .filter(|&pos| pos > filled && pos <= width);

    match peak_pos {
        Some(pos) => {
            let space_before = pos - filled - 1;
            let space_after = width - pos;
            spans.push(Span::raw(" ".repeat(space_before)));
            spans.push(Span::styled("▌", Style::default().fg(Color::White)));
            spans.push(Span::raw(" ".repeat(space_after)));
        }
        None => {
            spans.push(Span::raw(" ".repeat(width.saturating_sub(filled))));
        }
    }

    spans.push(Span::raw("│"));
    spans
}
