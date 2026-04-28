use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Padding, Paragraph};

use crate::channel::Channel;
use crate::ui::text_input::TextInput;

const MIN_DB: f32 = -45.0;
const MAX_DB: f32 = 6.0;

const CLIP_DB: f32 = 0.0;
const WARN_DB: f32 = -6.0;

/// Bright RGB colors for the green/yellow/red bands. Both meters and the
/// waveform reference these so they stay visually consistent.
pub const BAND_GREEN: Color = Color::Rgb(0, 255, 0);
pub const BAND_YELLOW: Color = Color::Rgb(255, 255, 0);
pub const BAND_RED: Color = Color::Rgb(255, 0, 0);

/// Dim variants for non-recorded audio in the waveform; recorded portions
/// use the bright variants.
pub const BAND_GREEN_DIM: Color = Color::Rgb(0, 80, 0);
pub const BAND_YELLOW_DIM: Color = Color::Rgb(80, 80, 0);
pub const BAND_RED_DIM: Color = Color::Rgb(80, 0, 0);

pub const TAKE_COLORS: &[Color] = &[
    Color::Rgb(60, 230, 80),
    Color::Rgb(255, 220, 30),
    Color::Rgb(255, 130, 30),
    Color::Rgb(255, 60, 70),
    Color::Rgb(195, 80, 220),
    Color::Rgb(60, 175, 255),
];

pub fn take_color(idx: usize) -> Color {
    TAKE_COLORS[idx % TAKE_COLORS.len()]
}

/// Braille spinner frame for the given tick. Advances every 6 ticks.
pub fn spinner_glyph(tick: u64) -> &'static str {
    const FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
    FRAMES[(tick / 6) as usize % FRAMES.len()]
}

const SILENCE_LEVEL: f32 = 0.0001;
const SILENCE_DB: f32 = -80.0;

/// Background tint for the meter "track" — every cell, full or empty,
/// gets this bg so the bar renders as bands inside a continuous track.
const METER_TRACK_BG: Color = Color::Rgb(40, 40, 40);

pub fn horizontal_meter(level: f32, peak_hold: Option<f32>, width: usize) -> Vec<Span<'static>> {
    const PARTIAL_GLYPHS: [&str; 7] = ["▏", "▎", "▍", "▌", "▋", "▊", "▉"];

    let (full_cells, partial) = db_to_fill(to_db(level), width);
    let (warn_cells, clip_cells) = band_positions(width);

    let mut spans = Vec::new();

    let green = full_cells.min(warn_cells);
    let yellow = full_cells.min(clip_cells).saturating_sub(green);
    let red = full_cells.saturating_sub(green + yellow);
    if green > 0 {
        spans.push(Span::styled(
            "█".repeat(green),
            Style::default().fg(BAND_GREEN).bg(METER_TRACK_BG),
        ));
    }
    if yellow > 0 {
        spans.push(Span::styled(
            "█".repeat(yellow),
            Style::default().fg(BAND_YELLOW).bg(METER_TRACK_BG),
        ));
    }
    if red > 0 {
        spans.push(Span::styled(
            "█".repeat(red),
            Style::default().fg(BAND_RED).bg(METER_TRACK_BG),
        ));
    }

    let mut cells_used = full_cells;
    if partial > 0 {
        let color = position_color(full_cells, warn_cells, clip_cells);
        spans.push(Span::styled(
            PARTIAL_GLYPHS[partial - 1],
            Style::default().fg(color).bg(METER_TRACK_BG),
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
            spans.push(Span::styled(
                " ".repeat(space_before),
                Style::default().bg(METER_TRACK_BG),
            ));
            spans.push(Span::styled(
                "▌",
                Style::default().fg(Color::White).bg(METER_TRACK_BG),
            ));
            spans.push(Span::styled(
                " ".repeat(space_after),
                Style::default().bg(METER_TRACK_BG),
            ));
        }
        None => {
            spans.push(Span::styled(
                " ".repeat(width.saturating_sub(cells_used)),
                Style::default().bg(METER_TRACK_BG),
            ));
        }
    }

    spans
}

pub fn vertical_meter(
    level: f32,
    peak_hold: Option<f32>,
    width: usize,
    height: usize,
) -> Vec<Line<'static>> {
    const PARTIAL_GLYPHS: [&str; 7] = ["▁", "▂", "▃", "▄", "▅", "▆", "▇"];

    let (full_rows, partial) = db_to_fill(to_db(level), height);
    let (warn_rows, clip_rows) = band_positions(height);

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
        let color = position_color(pos_from_bottom, warn_rows, clip_rows);

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

/// Color a single cell by its position on the dB scale, not by the bar's peak.
/// Mirrors Logic's behavior: bottom of bar stays green even when peaks clip.
fn position_color(pos: usize, warn_pos: usize, clip_pos: usize) -> Color {
    if pos >= clip_pos {
        BAND_RED
    } else if pos >= warn_pos {
        BAND_YELLOW
    } else {
        BAND_GREEN
    }
}

/// (warn_frac, clip_frac): the dB-fraction boundaries between green,
/// yellow, and red bands. Single source of truth for meters and waveform.
pub fn band_thresholds() -> (f32, f32) {
    (db_to_fraction(WARN_DB), db_to_fraction(CLIP_DB))
}

/// Same thresholds expressed in cell-position units, for meters that work
/// in discrete cells rather than fractions.
pub fn band_positions(length: usize) -> (usize, usize) {
    let (warn, clip) = band_thresholds();
    (
        (warn * length as f32).ceil().min(length as f32) as usize,
        (clip * length as f32).ceil().min(length as f32) as usize,
    )
}

/// Pro Tools-style linear-in-dB scale: the −45..0 range maps uniformly to the
/// bottom 92% of the bar; the over-0 clip zone gets the top 8%. Anything below
/// MIN_DB is silent (invisible).
fn db_to_fraction(db: f32) -> f32 {
    if db <= MIN_DB {
        0.0
    } else if db <= 0.0 {
        (db - MIN_DB) / -MIN_DB * 0.92
    } else if db <= MAX_DB {
        0.92 + db / MAX_DB * 0.08
    } else {
        1.0
    }
}

fn db_to_position(db: f32, length: usize) -> usize {
    (db_to_fraction(db) * length as f32)
        .ceil()
        .min(length as f32) as usize
}

/// Splits a dB level into (full_cells, partial) for a `total_cells`-wide
/// meter. Partial is the 0-7 sub-cell remainder for the next block glyph.
fn db_to_fill(db: f32, total_cells: usize) -> (usize, usize) {
    let total_subs = total_cells * 8;
    let fill_subs = (db_to_fraction(db) * total_subs as f32) as usize;
    let full = (fill_subs / 8).min(total_cells);
    let partial = if full == total_cells {
        0
    } else {
        fill_subs % 8
    };
    (full, partial)
}

/// Renders a key hint like `[Esc] stop and save` with a colored key.
pub fn key_hint(key: &str, action: &str, key_color: Color) -> Vec<Span<'static>> {
    vec![
        Span::styled(format!("[{}]", key), Style::default().fg(key_color)),
        Span::raw(format!(" {}", action)),
    ]
}

/// Color for hints whose action isn't currently available — clearly
/// dimmer than `DarkGray` so it doesn't read as just "secondary".
pub const DISABLED_HINT: Color = Color::Rgb(70, 70, 70);

/// Same as `key_hint` but renders the whole hint in `DISABLED_HINT`
/// when the action isn't currently available — preserves layout while
/// signaling unavailable.
pub fn key_hint_when(
    enabled: bool,
    key: &str,
    action: &str,
    key_color: Color,
) -> Vec<Span<'static>> {
    if enabled {
        key_hint(key, action, key_color)
    } else {
        vec![Span::styled(
            format!("[{}] {}", key, action),
            Style::default().fg(DISABLED_HINT),
        )]
    }
}

/// Bordered panel with a title at top-left, optional hints on the top-right
/// and bottom borders (alignment baked in by the caller), and consistent
/// inner padding. Renders the block and returns the inner drawing area.
pub fn panel(
    frame: &mut Frame,
    area: Rect,
    title: &'static str,
    top_right_hint: Option<Line<'static>>,
    bottom_hint: Option<Line<'static>>,
) -> Rect {
    let mut block = Block::default()
        .title(format!(" {} ", title))
        .borders(Borders::ALL)
        .padding(Padding::new(2, 2, 1, 1))
        .border_style(Style::default().fg(Color::DarkGray));
    if let Some(hint) = top_right_hint {
        block = block.title(hint);
    }
    if let Some(hint) = bottom_hint {
        block = block.title_bottom(hint);
    }
    let inner = block.inner(area);
    frame.render_widget(block, area);
    inner
}

/// `Label: Value` line — label dimmed, value default.
pub fn labeled(label: &'static str, value: String) -> Line<'static> {
    Line::from(vec![
        Span::styled(label, Style::default().fg(Color::DarkGray)),
        Span::raw(value),
    ])
}

/// Read-only row for displaying a channel in template previews. Armed
/// channels show a red filled circle; unarmed channels render dim with
/// an empty circle. (The interactive picker has its own row variant.)
pub fn channel_preview_row(channel: &Channel) -> Line<'static> {
    let (marker, marker_style, row_style) = if channel.armed {
        ("●", Style::default().fg(Color::Red), Style::default())
    } else {
        (
            "○",
            Style::default().fg(Color::DarkGray),
            Style::default().fg(Color::DarkGray),
        )
    };
    let label = channel.label.clone().unwrap_or_else(|| "—".to_string());
    Line::from(vec![
        Span::styled(format!(" {} ", marker), marker_style),
        Span::styled(format!("Ch {:>2}  ", channel.index), row_style),
        Span::styled(label, row_style),
    ])
}

/// Renders a `TextInput` as spans with a reverse-video block cursor on
/// the char under the cursor (or a trailing space at end-of-string).
/// `base` styles the surrounding text; the cursor cell layers REVERSED
/// over it.
pub fn input_with_cursor(input: &TextInput, base: Style) -> Vec<Span<'static>> {
    let (before, at, after) = input.split();
    let cursor_style = base.add_modifier(Modifier::REVERSED);
    let mut spans = Vec::with_capacity(3);
    if !before.is_empty() {
        spans.push(Span::styled(before.to_string(), base));
    }
    let cursor_glyph = if at.is_empty() { " " } else { at };
    spans.push(Span::styled(cursor_glyph.to_string(), cursor_style));
    if !after.is_empty() {
        spans.push(Span::styled(after.to_string(), base));
    }
    spans
}

/// Single dim line used as a placeholder when a panel has nothing active.
pub fn dim_status(text: &'static str) -> Vec<Line<'static>> {
    vec![Line::from(Span::styled(
        text,
        Style::default().fg(Color::DarkGray),
    ))]
}

/// Cells consumed by `Borders::ALL` along each axis (1 cell on each of
/// the two perpendicular sides). Exposed so callers sizing modals from
/// content dimensions can budget the available space against the frame.
pub const MODAL_BORDER_OVERHEAD: u16 = 2;

/// Renders a centered modal sized to hold `content_width` × `content_height`
/// cells of content, plus chrome. Draws Clear + a cyan-bordered Block with
/// `title` and returns the inner Rect for the caller to render content
/// into. Caller thinks in content terms; chrome is fully internalized.
pub fn modal(frame: &mut Frame, title: &str, content_width: u16, content_height: u16) -> Rect {
    let outer = center_rect(
        frame.area(),
        content_width.saturating_add(MODAL_BORDER_OVERHEAD),
        content_height.saturating_add(MODAL_BORDER_OVERHEAD),
    );
    frame.render_widget(Clear, outer);
    let block = Block::default()
        .title(format!(" {} ", title))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    let inner = block.inner(outer);
    frame.render_widget(block, outer);
    inner
}

fn center_rect(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    )
}

/// Renders `lines` newspaper-style across `n_cols` equal columns of
/// `area`: column 1 fills top-to-bottom, then column 2, and so on.
/// Overflow past the last column is dropped off the bottom.
pub fn flow_columns(frame: &mut Frame, area: Rect, lines: &[Line<'static>], n_cols: u32) {
    if n_cols == 0 || area.width == 0 || area.height == 0 {
        return;
    }
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .spacing(1)
        .constraints(vec![Constraint::Ratio(1, n_cols); n_cols as usize])
        .split(area);
    let per_col = area.height as usize;
    for (i, col_area) in cols.iter().enumerate() {
        let chunk: Vec<Line> = lines
            .iter()
            .skip(i * per_col)
            .take(per_col)
            .cloned()
            .collect();
        if !chunk.is_empty() {
            frame.render_widget(Paragraph::new(chunk), *col_area);
        }
    }
}

/// Converts a linear amplitude (0..1) to a dB-scaled fraction (0..1) suitable
/// for visualizing on a meter or waveform that should match perceptual loudness.
pub fn linear_to_db_fraction(level: f32) -> f32 {
    db_to_fraction(to_db(level))
}
