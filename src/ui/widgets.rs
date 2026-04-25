use ratatui::prelude::*;

const MIN_DB: f32 = -60.0;
const MAX_DB: f32 = 0.0;

const CLIP_DB: f32 = -3.0;
const WARN_DB: f32 = -18.0;

const SILENCE_LEVEL: f32 = 0.0001;
const SILENCE_DB: f32 = -80.0;

pub fn horizontal_meter(
    level: f32,
    peak_hold: Option<f32>,
    width: usize,
) -> Vec<Span<'static>> {
    const PARTIAL_GLYPHS: [&str; 7] = ["▏", "▎", "▍", "▌", "▋", "▊", "▉"];

    let db = to_db(level);
    let color = level_color(db);
    let (full_cells, partial) = db_to_fill(db, width);

    let mut spans = vec![Span::raw("│")];
    if full_cells > 0 {
        spans.push(Span::styled(
            "█".repeat(full_cells),
            Style::default().fg(color),
        ));
    }

    let mut cells_used = full_cells;
    if partial > 0 {
        spans.push(Span::styled(
            PARTIAL_GLYPHS[partial - 1],
            Style::default().fg(color),
        ));
        cells_used += 1;
    }

    let peak_pos = peak_hold
        .map(|p| db_to_position(to_db(p), width))
        .filter(|&pos| pos > cells_used && pos <= width);

    match peak_pos {
        Some(pos) => {
            let space_before = pos - cells_used - 1;
            let space_after = width - pos;
            spans.push(Span::raw(" ".repeat(space_before)));
            spans.push(Span::styled("▌", Style::default().fg(Color::White)));
            spans.push(Span::raw(" ".repeat(space_after)));
        }
        None => {
            spans.push(Span::raw(" ".repeat(width.saturating_sub(cells_used))));
        }
    }

    spans.push(Span::raw("│"));
    spans
}

pub fn vertical_meter(
    level: f32,
    peak_hold: Option<f32>,
    width: usize,
    height: usize,
) -> Vec<Line<'static>> {
    const PARTIAL_GLYPHS: [&str; 7] = ["▁", "▂", "▃", "▄", "▅", "▆", "▇"];

    let db = to_db(level);
    let color = level_color(db);
    let (full_rows, partial) = db_to_fill(db, height);

    let peak_row_from_bottom = peak_hold.and_then(|p| {
        let (peak_full, peak_partial) = db_to_fill(to_db(p), height);
        let peak_above_fill =
            peak_full > full_rows || (peak_full == full_rows && peak_partial > partial);
        if peak_above_fill && peak_partial > 0 {
            Some(peak_full)
        } else if peak_above_fill {
            Some(peak_full.saturating_sub(1))
        } else {
            None
        }
    });

    let mut lines = Vec::with_capacity(height);
    for row in 0..height {
        let pos_from_bottom = height - 1 - row;

        let span = if pos_from_bottom < full_rows {
            Span::styled("█".repeat(width), Style::default().fg(color))
        } else if pos_from_bottom == full_rows && partial > 0 {
            Span::styled(
                PARTIAL_GLYPHS[partial - 1].repeat(width),
                Style::default().fg(color),
            )
        } else if peak_row_from_bottom == Some(pos_from_bottom) {
            Span::styled("▔".repeat(width), Style::default().fg(Color::White))
        } else {
            Span::raw(" ".repeat(width))
        };

        lines.push(Line::from(span));
    }

    lines
}

fn to_db(level: f32) -> f32 {
    if level < SILENCE_LEVEL {
        SILENCE_DB
    } else {
        20.0 * level.log10()
    }
}

fn level_color(db: f32) -> Color {
    if db > CLIP_DB {
        Color::Red
    } else if db > WARN_DB {
        Color::Yellow
    } else if db > MIN_DB {
        Color::Green
    } else {
        Color::DarkGray
    }
}

fn db_to_fraction(db: f32) -> f32 {
    ((db - MIN_DB) / (MAX_DB - MIN_DB)).clamp(0.0, 1.0)
}

fn db_to_position(db: f32, length: usize) -> usize {
    (db_to_fraction(db) * length as f32)
        .ceil()
        .min(length as f32) as usize
}

/// Splits a dB level into (full_cells, partial) for a meter of `total_cells` cells.
/// 8 sub cells per cell
fn db_to_fill(db: f32, total_cells: usize) -> (usize, usize) {
    let total_subs = total_cells * 8;
    let fill_subs = (db_to_fraction(db) * total_subs as f32) as usize;
    let full = (fill_subs / 8).min(total_cells);
    let partial = if full == total_cells { 0 } else { fill_subs % 8 };
    (full, partial)
}

/// Renders a key hint like `[Esc] stop and save` with a colored key.
pub fn key_hint(key: &str, action: &str, key_color: Color) -> Vec<Span<'static>> {
    vec![
        Span::styled(format!("[{}]", key), Style::default().fg(key_color)),
        Span::raw(format!(" {}", action)),
    ]
}
