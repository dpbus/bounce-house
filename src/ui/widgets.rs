use std::sync::LazyLock;

use palette::{FromColor, Oklch, Srgb};
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

/// Lightness and chroma in OkLCh — a perceptually uniform color space.
/// At a fixed L, every hue reads at the same apparent brightness, so
/// yellow won't pop louder than blue. Chroma past ~0.2 risks falling
/// outside sRGB for some hues; 0.13 stays in gamut everywhere.
const TAKE_COLOR_LIGHTNESS: f32 = 0.65;
const TAKE_COLOR_CHROMA: f32 = 0.15;

/// Golden angle: 360° × (1 − 1/φ²). Irrational, so stepping by it
/// around the hue wheel never lands on a commensurable cycle — the
/// sequence keeps subdividing forever and consecutive hues are always
/// ~137.5° apart, well past any rainbow-progression feel.
const TAKE_HUE_STRIDE_DEG: f32 = 137.50776;

/// Per-session starting hue in degrees. Stable for the life of the
/// process so take colors don't shift mid-session, but different each
/// launch.
static TAKE_HUE_START_DEG: LazyLock<f32> = LazyLock::new(|| rand::random::<f32>() * 360.0);

/// Take colors are generated on the fly by stepping around the OkLCh
/// hue wheel by `TAKE_HUE_STRIDE_DEG` per take. Constant adjacent
/// contrast, perceptually uniform brightness, no curated palette to
/// maintain.
pub fn take_color(idx: usize) -> Color {
    let hue_deg = (*TAKE_HUE_START_DEG + idx as f32 * TAKE_HUE_STRIDE_DEG).rem_euclid(360.0);
    let oklch = Oklch::new(TAKE_COLOR_LIGHTNESS, TAKE_COLOR_CHROMA, hue_deg);
    let rgb: Srgb<u8> = Srgb::from_color(oklch).into_format();
    Color::Rgb(rgb.red, rgb.green, rgb.blue)
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
        let color = position_color(full_cells, warn_cells, clip_cells, false);
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
    dim: bool,
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

    let peak_color = if dim { Color::DarkGray } else { Color::White };

    let mut lines = Vec::with_capacity(height);
    for row in 0..height {
        let pos_from_bottom = height - 1 - row;
        let color = position_color(pos_from_bottom, warn_rows, clip_rows, dim);

        let span = if pos_from_bottom < full_rows {
            Span::styled("█".repeat(width), Style::default().fg(color))
        } else if pos_from_bottom == full_rows && partial > 0 {
            Span::styled(
                PARTIAL_GLYPHS[partial - 1].repeat(width),
                Style::default().fg(color),
            )
        } else if peak_row_from_bottom == Some(pos_from_bottom) {
            Span::styled("▔".repeat(width), Style::default().fg(peak_color))
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
/// When `dim`, returns the deep-dim variant for inactive/unarmed contexts.
fn position_color(pos: usize, warn_pos: usize, clip_pos: usize, dim: bool) -> Color {
    match (pos >= clip_pos, pos >= warn_pos, dim) {
        (true, _, false) => BAND_RED,
        (true, _, true) => BAND_RED_DIM,
        (false, true, false) => BAND_YELLOW,
        (false, true, true) => BAND_YELLOW_DIM,
        (false, false, false) => BAND_GREEN,
        (false, false, true) => BAND_GREEN_DIM,
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

pub fn mmss(secs: u64) -> String {
    format!("{}:{:02}", secs / 60, secs % 60)
}

pub fn truncate_with_ellipsis(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let cut: String = s.chars().take(max.saturating_sub(1)).collect();
    format!("{}…", cut)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mmss_formats_minutes_and_seconds() {
        assert_eq!(mmss(0), "0:00");
        assert_eq!(mmss(5), "0:05");
        assert_eq!(mmss(60), "1:00");
        assert_eq!(mmss(125), "2:05");
        assert_eq!(mmss(3599), "59:59");
        assert_eq!(mmss(3600), "60:00"); // no hours rollover by design
    }

    #[test]
    fn truncate_with_ellipsis_passes_through_short_strings() {
        assert_eq!(truncate_with_ellipsis("foo", 5), "foo");
        assert_eq!(truncate_with_ellipsis("foo", 3), "foo");
    }

    #[test]
    fn truncate_with_ellipsis_appends_ellipsis_for_overflow() {
        assert_eq!(truncate_with_ellipsis("hello", 4), "hel…");
        assert_eq!(truncate_with_ellipsis("abcdef", 3), "ab…");
    }

    #[test]
    fn truncate_with_ellipsis_counts_chars_not_bytes() {
        // "café" has 4 chars but 5 bytes in UTF-8.
        assert_eq!(truncate_with_ellipsis("café", 4), "café");
        assert_eq!(truncate_with_ellipsis("café", 3), "ca…");
    }

    #[test]
    fn truncate_with_ellipsis_handles_max_zero() {
        // saturating_sub(1) yields 0; just an ellipsis with no body.
        assert_eq!(truncate_with_ellipsis("anything", 0), "…");
    }

    #[test]
    fn to_db_clamps_silence_to_floor() {
        assert_eq!(to_db(0.0), SILENCE_DB);
        assert_eq!(to_db(SILENCE_LEVEL / 2.0), SILENCE_DB);
    }

    #[test]
    fn to_db_at_unity_is_zero() {
        let db = to_db(1.0);
        assert!(db.abs() < 0.001, "expected ~0 dB, got {db}");
    }

    #[test]
    fn to_db_half_is_minus_six() {
        let db = to_db(0.5);
        assert!((db - (-6.02)).abs() < 0.05, "expected ~-6 dB, got {db}");
    }

    #[test]
    fn db_to_fraction_at_floor_is_zero() {
        assert_eq!(db_to_fraction(MIN_DB), 0.0);
        assert_eq!(db_to_fraction(MIN_DB - 10.0), 0.0);
    }

    #[test]
    fn db_to_fraction_at_zero_is_band_threshold() {
        // -45..0 maps uniformly to the bottom 92%.
        assert!((db_to_fraction(0.0) - 0.92).abs() < 0.001);
    }

    #[test]
    fn db_to_fraction_at_max_db_caps_at_one() {
        assert!((db_to_fraction(MAX_DB) - 1.0).abs() < 0.001);
        assert_eq!(db_to_fraction(MAX_DB + 100.0), 1.0);
    }

    #[test]
    fn db_to_fraction_clip_zone_is_top_eight_percent() {
        // -3 dB sits inside the green band; +3 dB sits in the clip band.
        let mid = db_to_fraction(-3.0);
        let clip = db_to_fraction(3.0);
        assert!(mid < 0.92);
        assert!(clip > 0.92 && clip < 1.0);
    }

    #[test]
    fn band_thresholds_are_warn_then_clip() {
        let (warn, clip) = band_thresholds();
        assert!(warn < clip);
        assert!(warn > 0.0 && clip <= 1.0);
    }

    #[test]
    fn band_positions_map_thresholds_to_cell_indices() {
        // For a 50-cell meter the warn/clip positions must be in [0, 50].
        let (warn, clip) = band_positions(50);
        assert!(warn <= clip);
        assert!(clip <= 50);
    }

    #[test]
    fn db_to_fill_splits_into_full_cells_and_partial() {
        // At unity (0 dB) we expect ~92% of cells full.
        let (full, partial) = db_to_fill(0.0, 100);
        assert_eq!(full, 92);
        assert_eq!(partial, 0);
    }

    #[test]
    fn db_to_fill_caps_partial_to_zero_when_full() {
        // Above max — fully filled, no partial.
        let (full, partial) = db_to_fill(MAX_DB + 10.0, 50);
        assert_eq!(full, 50);
        assert_eq!(partial, 0);
    }

    #[test]
    fn position_color_promotes_through_bands() {
        assert_eq!(position_color(0, 5, 8, false), BAND_GREEN);
        assert_eq!(position_color(4, 5, 8, false), BAND_GREEN);
        assert_eq!(position_color(5, 5, 8, false), BAND_YELLOW);
        assert_eq!(position_color(7, 5, 8, false), BAND_YELLOW);
        assert_eq!(position_color(8, 5, 8, false), BAND_RED);
        assert_eq!(position_color(99, 5, 8, false), BAND_RED);
    }

    #[test]
    fn position_color_dim_returns_dim_band_variants() {
        assert_eq!(position_color(0, 5, 8, true), BAND_GREEN_DIM);
        assert_eq!(position_color(5, 5, 8, true), BAND_YELLOW_DIM);
        assert_eq!(position_color(8, 5, 8, true), BAND_RED_DIM);
    }

    #[test]
    fn spinner_glyph_advances_every_six_ticks() {
        let frame_a = spinner_glyph(0);
        let frame_b = spinner_glyph(5);
        let frame_c = spinner_glyph(6);
        assert_eq!(frame_a, frame_b);
        assert_ne!(frame_a, frame_c);
    }

    #[test]
    fn spinner_glyph_cycles_after_full_period() {
        // 10 frames × 6 ticks = 60 ticks per full cycle.
        assert_eq!(spinner_glyph(0), spinner_glyph(60));
        assert_eq!(spinner_glyph(0), spinner_glyph(120));
    }

    #[test]
    fn take_color_is_deterministic_across_calls() {
        // The first random offset is set on first call; all subsequent
        // calls with the same idx return the same color.
        let a = take_color(0);
        let b = take_color(0);
        assert_eq!(a, b);
    }

    #[test]
    fn take_color_differs_across_consecutive_indices() {
        // The golden-angle stride should produce visually different
        // colors for adjacent indices.
        let c0 = take_color(0);
        let c1 = take_color(1);
        let c2 = take_color(2);
        assert_ne!(c0, c1);
        assert_ne!(c1, c2);
    }

    #[test]
    fn linear_to_db_fraction_silence_is_zero() {
        assert_eq!(linear_to_db_fraction(0.0), 0.0);
    }

    #[test]
    fn linear_to_db_fraction_unity_is_band_threshold() {
        let frac = linear_to_db_fraction(1.0);
        assert!((frac - 0.92).abs() < 0.001);
    }
}
