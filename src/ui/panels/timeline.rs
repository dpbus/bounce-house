use std::collections::HashMap;
use std::ops::Range;

use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

use crate::app::App;
use crate::timeline::{BounceStatus, Take, Timeline};
use crate::transport::TransportMode;
use crate::ui::view::View;
use crate::ui::widgets::{
    dim_status, input_with_cursor, key_hint, mmss, panel, spinner_glyph, take_color,
    truncate_with_ellipsis,
};

/// Until the recording grows past this duration, the panel maps this
/// many seconds across its full height. After, the scale stretches so
/// the entire recording always fits on one screen.
const MIN_TIMELINE_SECS: u64 = 30;
/// Fixed character column for take names so durations align.
const NAME_WIDTH: usize = 14;

#[derive(Clone, Copy)]
enum TimelineEvent {
    Take { index: usize },
    MarkerCluster { marker_count: usize },
}

#[derive(Clone, Copy)]
struct PlacedEvent {
    row: usize,
    anchor_sample: u64,
    event: TimelineEvent,
}

/// Maps recording-relative seconds to row indices. "Now" is pinned at
/// the bottom row; older content stacks upward. Until the recording
/// grows past `MIN_TIMELINE_SECS` the rows above stay empty; after, the
/// scale stretches so the full recording fits in `panel_rows`.
struct TimelineLayout {
    now_sec: u64,
    max_secs: u64,
    panel_rows: usize,
    now_row: usize,
}

impl TimelineLayout {
    fn new(now_sec: u64, panel_rows: usize) -> Self {
        let max_secs = now_sec.max(MIN_TIMELINE_SECS);
        let now_row = panel_rows.saturating_sub(1);
        Self {
            now_sec,
            max_secs,
            panel_rows,
            now_row,
        }
    }

    fn row_for_sec(&self, secs: u64) -> usize {
        let secs_ago = self.now_sec.saturating_sub(secs);
        let rows_back =
            (secs_ago.saturating_mul(self.panel_rows as u64) / self.max_secs.max(1)) as usize;
        self.now_row.saturating_sub(rows_back)
    }
}

pub fn draw(frame: &mut Frame, area: Rect, app: &App, view: &View) {
    let inner = panel(frame, area, "Timeline", None, naming_hint(view));

    let session = &app.session;
    if !session.has_recording() {
        frame.render_widget(Paragraph::new(dim_status("No recording")), inner);
        return;
    }

    let panel_rows = inner.height as usize;
    if panel_rows == 0 {
        return;
    }

    let timeline = session.timeline();
    let is_recording = app.transport.is_recording();
    let layout = TimelineLayout::new(app.recording_duration_secs().unwrap_or(0), panel_rows);
    let naming_row = naming_row(view, timeline, &layout);
    let bottom_row = naming_row.unwrap_or(layout.now_row);

    let events = collect_events(timeline, &layout, is_recording);
    let (placed_events, overflow_boundary) = bump_into_rows(events, bottom_row, timeline, &layout);

    let mut grid: Vec<Line<'static>> = (0..panel_rows).map(|_| empty_row()).collect();
    render_events(&mut grid, &placed_events, timeline, view.total_ticks);
    fill_take_continuations(&mut grid, &placed_events, timeline, &layout);
    render_in_progress_segment(
        &mut grid,
        view,
        &placed_events,
        timeline,
        &layout,
        naming_row,
    );

    // Recording-start indicator travels up with the proportional scale
    // and anchors at row 0 past MIN_TIMELINE_SECS. Painted after the
    // continuations so it survives any take block crossing its row.
    let start_row = layout.row_for_sec(0);
    if naming_row != Some(start_row) {
        grid[start_row] = recording_start_line(recording_start_color(timeline, view));
    }

    // Overflow boundary takes row 0 — its presence means the recording
    // start has scrolled off into the hidden range above.
    if let Some(boundary) = overflow_boundary {
        grid[0] = view_top_boundary_line(timeline.secs_at(boundary));
    }

    let since_secs = app
        .transport
        .rel_sample_position()
        .map(|rel| timeline.since_last_marker_secs(rel))
        .unwrap_or(0);
    grid[layout.now_row] = now_line(layout.now_sec, since_secs, app.transport.mode());

    frame.render_widget(Paragraph::new(grid), inner);
}

/// Row where the take-name input lives — the trailing marker's row,
/// capped one above the now line so a freshly-dropped marker can't
/// collide with the clock.
fn naming_row(view: &View, timeline: &Timeline, layout: &TimelineLayout) -> Option<usize> {
    view.take_naming().map(|_| {
        let trailing_sample = timeline.markers().last().map(|m| m.sample).unwrap_or(0);
        let trailing_row = layout.row_for_sec(timeline.secs_at(trailing_sample));
        trailing_row.min(layout.now_row.saturating_sub(1))
    })
}

/// Build the chronological list of events: takes anchored at their
/// end_sample, plus unbound markers grouped by row so rapid-fire
/// markers within one row's time bucket render as one counted line.
fn collect_events(
    timeline: &Timeline,
    layout: &TimelineLayout,
    is_recording: bool,
) -> Vec<(u64, TimelineEvent)> {
    let mut events: Vec<(u64, TimelineEvent)> = Vec::new();

    for (i, take) in timeline.takes().iter().enumerate() {
        events.push((take.end_sample, TimelineEvent::Take { index: i }));
    }

    // After stop, hide the auto-mark added by Recording::stop — it
    // doesn't represent anything the user dropped, and the now line
    // already shows the recording's end time.
    let suppressed = if is_recording {
        None
    } else {
        timeline.markers().last().map(|m| m.sample)
    };

    let mut clusters_by_row: HashMap<usize, (u64, usize)> = HashMap::new();
    for marker in timeline.markers() {
        // Sample 0 is the recording-start auto-mark — represented by
        // the pinned 0:00 line at row 0, not as a regular marker.
        if marker.sample == 0 {
            continue;
        }
        if timeline.is_marker_bound(marker.sample) {
            continue;
        }
        if Some(marker.sample) == suppressed {
            continue;
        }
        let row = layout.row_for_sec(timeline.secs_at(marker.sample));
        clusters_by_row
            .entry(row)
            .and_modify(|(_, count)| *count += 1)
            .or_insert((marker.sample, 1));
    }
    for (anchor_sample, marker_count) in clusters_by_row.into_values() {
        events.push((anchor_sample, TimelineEvent::MarkerCluster { marker_count }));
    }

    events.sort_by_key(|(s, _)| *s);
    events
}

/// Walk events newest-first, placing each at its proportional row
/// unless that row is already taken — in which case the event bumps to
/// the next free row above. Returns the placements plus, when events
/// overflow the panel, the sample at the top boundary so the row-0
/// "view top" indicator can show its time.
fn bump_into_rows(
    events: Vec<(u64, TimelineEvent)>,
    bottom_row: usize,
    timeline: &Timeline,
    layout: &TimelineLayout,
) -> (Vec<PlacedEvent>, Option<u64>) {
    let total_events = events.len();
    let mut placed: Vec<PlacedEvent> = Vec::new();
    let mut next_free_row = bottom_row;
    for (sample, event) in events.iter().rev() {
        if next_free_row == 0 {
            break;
        }
        let proportional_row = layout.row_for_sec(timeline.secs_at(*sample));
        let row = proportional_row.min(next_free_row - 1);
        placed.push(PlacedEvent {
            row,
            anchor_sample: *sample,
            event: *event,
        });
        next_free_row = row;
    }

    let overflow_boundary = if placed.len() < total_events {
        placed.last().map(|p| p.anchor_sample)
    } else {
        None
    };

    placed.reverse();
    (placed, overflow_boundary)
}

fn render_events(
    grid: &mut [Line<'static>],
    placed_events: &[PlacedEvent],
    timeline: &Timeline,
    total_ticks: u64,
) {
    let takes = timeline.takes();
    for placed in placed_events {
        match placed.event {
            TimelineEvent::Take { index } => {
                let take = &takes[index];
                let dur_secs = timeline.duration_secs(take.start_sample, take.end_sample);
                grid[placed.row] = take_info_line(take, dur_secs, total_ticks);
            }
            TimelineEvent::MarkerCluster { marker_count } => {
                let secs = timeline.secs_at(placed.anchor_sample);
                grid[placed.row] = marker_line(secs, marker_count);
            }
        }
    }
}

/// Each take's colored block extends upward from its info row to its
/// proportional start, capped at the previous event's row.
fn fill_take_continuations(
    grid: &mut [Line<'static>],
    placed_events: &[PlacedEvent],
    timeline: &Timeline,
    layout: &TimelineLayout,
) {
    let takes = timeline.takes();
    for (i, placed) in placed_events.iter().enumerate() {
        let TimelineEvent::Take { index } = placed.event else {
            continue;
        };
        let take = &takes[index];
        let start_row = layout.row_for_sec(timeline.secs_at(take.start_sample));
        let prev_floor = if i == 0 {
            0
        } else {
            placed_events[i - 1].row + 1
        };
        let first = start_row.max(prev_floor);
        let color = take_color(take.color_index as usize);
        #[allow(clippy::needless_range_loop)]
        for r in first..placed.row {
            grid[r] = continuation_line(color);
        }
    }
}

/// While the user is naming a take, fill the in-progress segment with
/// the next take's color and render the input on the naming row.
fn render_in_progress_segment(
    grid: &mut [Line<'static>],
    view: &View,
    placed_events: &[PlacedEvent],
    timeline: &Timeline,
    layout: &TimelineLayout,
    naming_row: Option<usize>,
) {
    let Some(naming_row) = naming_row else {
        return;
    };
    let color = take_color(timeline.next_take_color() as usize);
    for r in in_progress_fill_range(placed_events, timeline, layout, naming_row) {
        grid[r] = continuation_line(color);
    }
    if let Some(overlay) = view.take_naming() {
        let mut spans = vec![Span::styled("▌ ", Style::default().fg(color))];
        spans.extend(input_with_cursor(overlay.input(), Style::default()));
        grid[naming_row] = Line::from(spans);
    }
}

/// Rows the in-progress block should fill, ending just above the naming
/// row. Top edge is the previous marker's placed row, but never above
/// the most recently committed take — markers between the prev marker
/// and the naming row get overwritten because the in-progress take is
/// about to absorb that span.
fn in_progress_fill_range(
    placed_events: &[PlacedEvent],
    timeline: &Timeline,
    layout: &TimelineLayout,
    naming_row: usize,
) -> Range<usize> {
    let prev_sample = second_to_last_marker_sample(timeline);
    let prev_natural_row = layout.row_for_sec(timeline.secs_at(prev_sample));
    let prev_placed_row = placed_row_for(placed_events, prev_sample, prev_natural_row);
    let committed_take_floor = placed_events
        .iter()
        .rev()
        .find_map(|p| match p.event {
            TimelineEvent::Take { .. } => Some(p.row + 1),
            _ => None,
        })
        .unwrap_or(0);
    prev_placed_row.max(committed_take_floor)..naming_row
}

/// Returns the row where `sample` was actually placed (which may have
/// been bumped from its proportional row), falling back to the natural
/// row when the sample isn't in the placement list.
fn placed_row_for(
    placed_events: &[PlacedEvent],
    sample: u64,
    fallback_natural_row: usize,
) -> usize {
    placed_events
        .iter()
        .find_map(|p| (p.anchor_sample == sample).then_some(p.row))
        .unwrap_or(fallback_natural_row)
}

fn empty_row() -> Line<'static> {
    Line::from(Span::styled("│", Style::default().fg(Color::DarkGray)))
}

fn continuation_line(color: Color) -> Line<'static> {
    Line::from(Span::styled("▌", Style::default().fg(color)))
}

fn take_info_line(take: &Take, dur_secs: u64, total_ticks: u64) -> Line<'static> {
    let color = take_color(take.color_index as usize);
    Line::from(vec![
        Span::styled("▌ ", Style::default().fg(color)),
        Span::raw(format!(
            "{:<width$}",
            truncate_with_ellipsis(&take.name, NAME_WIDTH),
            width = NAME_WIDTH
        )),
        Span::styled(
            format!("  {}  ", mmss(dur_secs)),
            Style::default().fg(Color::DarkGray),
        ),
        bounce_status_span(take.bounce_status, total_ticks),
    ])
}

fn marker_line(secs: u64, count: usize) -> Line<'static> {
    let text = if count > 1 {
        format!("· {} ({})", mmss(secs), count)
    } else {
        format!("· {}", mmss(secs))
    };
    Line::from(Span::styled(text, Style::default().fg(Color::DarkGray)))
}

/// Color for the `0:00` glyph: matches the take that starts at sample
/// 0 if any, the in-progress take's color while naming one that begins
/// at recording start, else `None` for a neutral dot.
fn recording_start_color(timeline: &Timeline, view: &View) -> Option<Color> {
    if view.take_naming().is_some() && second_to_last_marker_sample(timeline) == 0 {
        return Some(take_color(timeline.next_take_color() as usize));
    }
    timeline
        .marker_color_index(0)
        .map(|i| take_color(i as usize))
}

/// Sample of the marker before the trailing one — i.e., the start of
/// the span the user is naming or about to name. Defaults to 0 when
/// fewer than two markers exist.
fn second_to_last_marker_sample(timeline: &Timeline) -> u64 {
    timeline
        .markers()
        .iter()
        .rev()
        .nth(1)
        .map(|m| m.sample)
        .unwrap_or(0)
}

fn recording_start_line(color: Option<Color>) -> Line<'static> {
    let glyph = match color {
        Some(c) => Span::styled("▌", Style::default().fg(c)),
        None => Span::styled("·", Style::default().fg(Color::DarkGray)),
    };
    Line::from(vec![
        glyph,
        Span::styled(" 0:00", Style::default().fg(Color::DarkGray)),
    ])
}

fn view_top_boundary_line(secs: u64) -> Line<'static> {
    Line::from(Span::styled(
        format!("▲ {}", mmss(secs)),
        Style::default().fg(Color::DarkGray),
    ))
}

fn now_line(now_sec: u64, since_secs: u64, mode: TransportMode) -> Line<'static> {
    let (glyph, clock_style) = match mode {
        TransportMode::Recording => (
            "●",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ),
        TransportMode::Paused => (
            "⏸",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        TransportMode::Playing => (
            "▶",
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ),
        TransportMode::Idle => ("■", Style::default().fg(Color::DarkGray)),
    };
    let clock_text = format!("{} {}", glyph, mmss(now_sec));
    let since_text = format!("+{}", mmss(since_secs));
    // Align the digit of "+MM:SS" with the digit of the take duration
    // column in take_info_line: 2 ("▌ ") + NAME_WIDTH + 2 spaces. The
    // "+" sits one column earlier so the digits line up.
    let digit_col = 2 + NAME_WIDTH + 2;
    let pad = (digit_col - 1).saturating_sub(clock_text.chars().count());
    Line::from(vec![
        Span::styled(clock_text, clock_style),
        Span::raw(" ".repeat(pad)),
        Span::styled(since_text, Style::default().fg(Color::DarkGray)),
    ])
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

fn bounce_status_span(status: BounceStatus, total_ticks: u64) -> Span<'static> {
    match status {
        BounceStatus::Pending => Span::styled("◌", Style::default().fg(Color::DarkGray)),
        BounceStatus::Bouncing => Span::styled(
            spinner_glyph(total_ticks),
            Style::default().fg(Color::White),
        ),
        BounceStatus::Done => Span::styled("✓", Style::default().fg(Color::Green)),
        BounceStatus::Failed => Span::styled("✗", Style::default().fg(Color::Red)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn now_at_bottom_row() {
        let layout = TimelineLayout::new(60, 24);
        // The "now" sample (== now_sec) sits at the bottom row.
        assert_eq!(layout.row_for_sec(60), 23);
    }

    #[test]
    fn early_seconds_stay_near_bottom_until_window_grows() {
        // Panel covers MIN_TIMELINE_SECS (30) when recording is shorter.
        // At 5 seconds elapsed, "0 sec" maps to ~5/30 of the height up.
        let layout = TimelineLayout::new(5, 24);
        let row = layout.row_for_sec(0);
        // ~5/30 of 24 = 4 rows back from now_row (23) → row 19.
        assert!((18..=20).contains(&row), "expected ~19, got {row}");
    }

    #[test]
    fn old_seconds_clamp_to_top_row() {
        // After window grows past min, the recording start (sec 0) anchors at row 0.
        let layout = TimelineLayout::new(60, 24);
        assert_eq!(layout.row_for_sec(0), 0);
    }

    #[test]
    fn future_seconds_clamp_to_now_row() {
        // Querying past `now_sec` (e.g. by sample races) saturates rather than wrapping.
        let layout = TimelineLayout::new(60, 24);
        assert_eq!(layout.row_for_sec(120), 23);
    }

    #[test]
    fn min_timeline_secs_holds_until_recording_outgrows_it() {
        // When elapsed < MIN_TIMELINE_SECS, the scale is fixed at MIN.
        // 15 seconds elapsed → max_secs = 30, row 0 corresponds to "30 sec ago".
        let layout = TimelineLayout::new(15, 24);
        assert_eq!(layout.max_secs, MIN_TIMELINE_SECS);
    }

    #[test]
    fn long_recording_stretches_to_fit_full_duration() {
        let layout = TimelineLayout::new(600, 24);
        assert_eq!(layout.max_secs, 600);
    }

    #[test]
    fn now_row_handles_zero_panel_rows() {
        let layout = TimelineLayout::new(60, 0);
        assert_eq!(layout.now_row, 0);
    }

    #[test]
    fn row_for_sec_stable_across_proportional_scale() {
        // Halfway through the visible window should be ~halfway up the panel.
        let layout = TimelineLayout::new(60, 24);
        let mid = layout.row_for_sec(30);
        assert!((11..=13).contains(&mid), "expected ~12, got {mid}");
    }
}
