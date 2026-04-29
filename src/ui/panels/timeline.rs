use std::collections::HashMap;
use std::ops::Range;

use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

use crate::app::App;
use crate::recording::Recording;
use crate::timeline::{BounceStatus, Take};
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
    /// One or more older events rolled up into a single line because
    /// they overflowed the panel height.
    OlderRollup { hidden_count: usize },
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

    fn row_for_sec(&self, s: u64) -> usize {
        let secs_ago = self.now_sec.saturating_sub(s);
        let rows_back =
            (secs_ago.saturating_mul(self.panel_rows as u64) / self.max_secs.max(1)) as usize;
        self.now_row.saturating_sub(rows_back)
    }
}

pub fn draw(frame: &mut Frame, area: Rect, app: &App, view: &View) {
    let inner = panel(frame, area, "Timeline", None, naming_hint(view));

    let Some(recording) = &app.recording else {
        frame.render_widget(Paragraph::new(dim_status("No recording")), inner);
        return;
    };

    let panel_rows = inner.height as usize;
    if panel_rows == 0 {
        return;
    }

    let layout = TimelineLayout::new(recording.elapsed_secs(), panel_rows);
    let naming_row = naming_row(view, recording, &layout);
    let bottom_row = naming_row.unwrap_or(layout.now_row);

    let events = collect_events(recording, &layout, app.is_recording());
    let placed_events = bump_into_rows(events, bottom_row, recording, &layout);

    let mut grid: Vec<Line<'static>> = (0..panel_rows).map(|_| empty_row()).collect();
    render_events(&mut grid, &placed_events, recording, app.total_ticks);
    fill_take_continuations(&mut grid, &placed_events, recording, &layout);
    render_in_progress_segment(&mut grid, view, &placed_events, recording, &layout, naming_row);

    let since_secs = if app.is_recording() {
        recording.since_last_marker_secs(app.engine.sample_position())
    } else {
        recording.elapsed_secs()
    };
    grid[layout.now_row] = now_line(inner.width, layout.now_sec, since_secs, app.is_recording());


    frame.render_widget(Paragraph::new(grid), inner);
}

/// Row where the in-progress take's name input lives during naming —
/// the trailing marker's row, capped one above the clock so a freshly-
/// dropped marker still sits just above the now line.
fn naming_row(view: &View, recording: &Recording, layout: &TimelineLayout) -> Option<usize> {
    view.take_naming().map(|_| {
        let trailing_sample = recording
            .timeline
            .markers()
            .last()
            .map(|m| m.sample)
            .unwrap_or(0);
        let trailing_row = layout.row_for_sec(recording.secs_at(trailing_sample));
        trailing_row.min(layout.now_row.saturating_sub(1))
    })
}

/// Build the chronological list of events: takes anchored at their
/// end_sample, plus unbound markers grouped by row so rapid-fire
/// markers within one row's time bucket render as one counted line.
fn collect_events(
    recording: &Recording,
    layout: &TimelineLayout,
    is_recording: bool,
) -> Vec<(u64, TimelineEvent)> {
    let timeline = &recording.timeline;
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
        if timeline.is_marker_bound(marker.sample) {
            continue;
        }
        if Some(marker.sample) == suppressed {
            continue;
        }
        let row = layout.row_for_sec(recording.secs_at(marker.sample));
        clusters_by_row
            .entry(row)
            .and_modify(|(_, count)| *count += 1)
            .or_insert((marker.sample, 1));
    }
    for (_, (anchor_sample, marker_count)) in clusters_by_row {
        events.push((
            anchor_sample,
            TimelineEvent::MarkerCluster { marker_count },
        ));
    }

    events.sort_by_key(|(s, _)| *s);
    events
}

/// Walk events newest-first, placing each at its proportional row
/// unless that row is already taken — in which case the event bumps to
/// the next free row above. Older events that don't fit roll up into a
/// single `OlderRollup` at the topmost row.
fn bump_into_rows(
    events: Vec<(u64, TimelineEvent)>,
    bottom_row: usize,
    recording: &Recording,
    layout: &TimelineLayout,
) -> Vec<PlacedEvent> {
    let total_events = events.len();
    let mut placed: Vec<PlacedEvent> = Vec::new();
    let mut next_free_row = bottom_row;
    for (sample, event) in events.iter().rev() {
        if next_free_row == 0 {
            break;
        }
        let proportional = layout.row_for_sec(recording.secs_at(*sample));
        let row = proportional.min(next_free_row - 1);
        placed.push(PlacedEvent {
            row,
            anchor_sample: *sample,
            event: *event,
        });
        next_free_row = row;
    }

    if placed.len() < total_events {
        // Out of rows. Repurpose the topmost placed slot as the rollup
        // glyph, so its event is hidden too — hence the +1.
        let hidden = total_events - placed.len() + 1;
        if let Some(top) = placed.last_mut() {
            top.event = TimelineEvent::OlderRollup {
                hidden_count: hidden,
            };
        }
    }

    placed.reverse();
    placed
}

fn render_events(
    grid: &mut [Line<'static>],
    placed_events: &[PlacedEvent],
    recording: &Recording,
    total_ticks: u64,
) {
    let takes = recording.timeline.takes();
    for placed in placed_events {
        match placed.event {
            TimelineEvent::Take { index } => {
                let take = &takes[index];
                let dur_secs = recording.duration_secs(take.start_sample, take.end_sample);
                grid[placed.row] = take_info_line(take, dur_secs, total_ticks);
            }
            TimelineEvent::MarkerCluster { marker_count } => {
                let secs = recording.secs_at(placed.anchor_sample);
                grid[placed.row] = marker_line(secs, marker_count);
            }
            TimelineEvent::OlderRollup { hidden_count } => {
                grid[placed.row] = older_rollup_line(hidden_count);
            }
        }
    }
}

/// Each take's colored block extends upward from its info row to its
/// proportional start, capped at the previous event's row.
fn fill_take_continuations(
    grid: &mut [Line<'static>],
    placed_events: &[PlacedEvent],
    recording: &Recording,
    layout: &TimelineLayout,
) {
    let takes = recording.timeline.takes();
    for (i, placed) in placed_events.iter().enumerate() {
        let TimelineEvent::Take { index } = placed.event else {
            continue;
        };
        let take = &takes[index];
        let start_row = layout.row_for_sec(recording.secs_at(take.start_sample));
        let prev_floor = if i == 0 {
            0
        } else {
            placed_events[i - 1].row + 1
        };
        let first = start_row.max(prev_floor);
        let color = take_color(take.color_index as usize);
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
    recording: &Recording,
    layout: &TimelineLayout,
    naming_row: Option<usize>,
) {
    let Some(naming_row) = naming_row else {
        return;
    };
    let color = take_color(recording.timeline.next_take_color() as usize);
    for r in in_progress_fill_range(placed_events, recording, layout, naming_row) {
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
    recording: &Recording,
    layout: &TimelineLayout,
    naming_row: usize,
) -> Range<usize> {
    let markers = recording.timeline.markers();
    let prev_sample = if markers.len() >= 2 {
        markers[markers.len() - 2].sample
    } else {
        0
    };
    let prev_natural_row = layout.row_for_sec(recording.secs_at(prev_sample));
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
        bounce_status_span(&take.bounce_status, total_ticks),
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

fn older_rollup_line(hidden_count: usize) -> Line<'static> {
    Line::from(Span::styled(
        format!("▴ {} older", hidden_count),
        Style::default().fg(Color::DarkGray),
    ))
}

fn now_line(width: u16, now_sec: u64, since_secs: u64, is_recording: bool) -> Line<'static> {
    let (glyph, clock_style) = if is_recording {
        (
            "●",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        )
    } else {
        ("■", Style::default().fg(Color::DarkGray))
    };
    let since_text = format!("+ {}", mmss(since_secs));
    let clock_text = format!("{} {}", glyph, mmss(now_sec));
    let used = since_text.chars().count() + clock_text.chars().count();
    let pad = (width as usize).saturating_sub(used);
    Line::from(vec![
        Span::styled(since_text, Style::default().fg(Color::DarkGray)),
        Span::raw(" ".repeat(pad)),
        Span::styled(clock_text, clock_style),
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
