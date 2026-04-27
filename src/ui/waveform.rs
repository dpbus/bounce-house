use std::collections::VecDeque;

use ratatui::prelude::*;
use ratatui::symbols::Marker;
use ratatui::widgets::canvas::{Canvas, Line as CanvasLine};
use ratatui::widgets::{Block, Borders};

use crate::app::{App, LevelSample};
use crate::ui::widgets::{
    BAND_GREEN, BAND_GREEN_DIM, BAND_RED, BAND_RED_DIM, BAND_YELLOW, BAND_YELLOW_DIM,
    band_thresholds, linear_to_db_fraction, take_color,
};

pub fn draw(frame: &mut Frame, area: Rect, app: &App) {
    let label = match app.waveform_window_secs {
        s if s < 60 => format!("{}s", s),
        s if s < 3600 => format!("{} min", s / 60),
        s => format!("{} hr", s / 3600),
    };
    let hint = Line::from(vec![
        Span::styled("[W]", Style::default().fg(Color::Cyan)),
        Span::raw(" cycle "),
    ])
    .right_aligned();
    let block = Block::default()
        .title(format!(" Waveform — {} window ", label))
        .title(hint)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.width == 0 || inner.height < 4 {
        return;
    }

    // Top and bottom rows reserved for marker glyphs; canvas in between.
    let canvas_area = Rect::new(inner.x, inner.y + 1, inner.width, inner.height - 2);
    let top_marker_y = inner.y;
    let bottom_marker_y = inner.bottom() - 1;

    // Braille's 2 dot columns per cell give 2x horizontal resolution.
    let cols = canvas_area.width as usize * 2;
    let height = canvas_area.height as usize;

    let layout = WaveformLayout::new(
        app.waveform_window_secs,
        cols,
        app.engine.sample_rate().0 as u64,
        app.engine.sample_position(),
    );
    let amps = waveform_amps(&app.level_history, &layout);
    let marker_columns: Vec<(Option<u8>, usize)> = app
        .recording
        .as_ref()
        .map(|r| {
            r.timeline
                .markers()
                .iter()
                .filter_map(|m| {
                    let col = layout.sample_to_column(r.start_sample + m.sample)?;
                    Some((r.timeline.marker_color_index(m.sample), col))
                })
                .collect()
        })
        .unwrap_or_default();
    let (warn, clip) = band_thresholds();
    let (warn, clip) = (warn as f64, clip as f64);
    // 1 braille pixel of vertical extent — keeps the centerline visible
    // through silent recorded moments.
    let min_y = 1.0 / (height as f64 * 4.0);

    let canvas = Canvas::default()
        .marker(Marker::Braille)
        .x_bounds([0.0, cols as f64])
        .y_bounds([-1.0, 1.0])
        .paint(move |ctx| {
            for (col, opt) in amps.iter().enumerate() {
                let Some((amp, recorded)) = opt else { continue };
                let half = (linear_to_db_fraction(*amp) as f64).max(min_y);
                let (green, yellow, red) = if *recorded {
                    (BAND_GREEN, BAND_YELLOW, BAND_RED)
                } else {
                    (BAND_GREEN_DIM, BAND_YELLOW_DIM, BAND_RED_DIM)
                };
                let x = col as f64;

                let g_top = half.min(warn);
                ctx.draw(&CanvasLine {
                    x1: x, y1: -g_top, x2: x, y2: g_top, color: green,
                });
                if half > warn {
                    let y_top = half.min(clip);
                    ctx.draw(&CanvasLine {
                        x1: x, y1: warn, x2: x, y2: y_top, color: yellow,
                    });
                    ctx.draw(&CanvasLine {
                        x1: x, y1: -y_top, x2: x, y2: -warn, color: yellow,
                    });
                }
                if half > clip {
                    ctx.draw(&CanvasLine {
                        x1: x, y1: clip, x2: x, y2: half, color: red,
                    });
                    ctx.draw(&CanvasLine {
                        x1: x, y1: -half, x2: x, y2: -clip, color: red,
                    });
                }
            }
        });
    frame.render_widget(canvas, canvas_area);

    // For close marker clusters (within 1 canvas_col), prefer take-bound
    // markers over unbound ones; otherwise keep the latest. Snap-invariant
    // — relative canvas_col deltas are preserved across grid shifts.
    let mut kept: Vec<(Option<u8>, usize)> = Vec::new();
    let consider = |kept: &mut Vec<(Option<u8>, usize)>, m: (Option<u8>, usize)| {
        if !kept.iter().any(|k| k.1.abs_diff(m.1) <= 1) {
            kept.push(m);
        }
    };
    for &m in marker_columns.iter().rev() {
        if m.0.is_some() {
            consider(&mut kept, m);
        }
    }
    for &m in marker_columns.iter().rev() {
        if m.0.is_none() {
            consider(&mut kept, m);
        }
    }

    let buf = frame.buffer_mut();
    for &(color_index, canvas_col) in &kept {
        let term_col = inner.x + (canvas_col / 2) as u16;
        if term_col >= inner.right() {
            continue;
        }
        let glyph = if canvas_col % 2 == 0 { "▌" } else { "▐" };
        let color = color_index
            .map(|i| take_color(i as usize))
            .unwrap_or(Color::DarkGray);
        let style = Style::default().fg(color);
        buf.set_string(term_col, top_marker_y, glyph, style);
        buf.set_string(term_col, bottom_marker_y, glyph, style);
    }
}

/// Maps engine-absolute samples onto canvas columns. Leftmost is snapped
/// to a bucket grid so historical buckets stay aligned across frames; the
/// rightmost bucket contains `current_sample`, so live observations land
/// there immediately. The whole grid jump-shifts one column when
/// `current_sample` crosses to the next bucket boundary.
struct WaveformLayout {
    cols: usize,
    leftmost_sample: u64,
    samples_per_col: u64,
}

impl WaveformLayout {
    fn new(window_secs: u64, cols: usize, sample_rate: u64, current_sample: u64) -> Self {
        let visible_samples = window_secs.saturating_mul(sample_rate);
        let samples_per_col = (visible_samples / cols as u64).max(1);
        let snap_down = (current_sample / samples_per_col) * samples_per_col;
        let leftmost_sample = snap_down.saturating_sub(samples_per_col * (cols as u64 - 1));
        Self { cols, leftmost_sample, samples_per_col }
    }

    fn sample_to_column(&self, sample: u64) -> Option<usize> {
        if sample < self.leftmost_sample {
            return None;
        }
        let col = ((sample - self.leftmost_sample) / self.samples_per_col) as usize;
        // The right edge sample (= current_sample) computes to col == cols
        // due to integer division; clamp so the latest tick lands in the
        // rightmost column rather than getting skipped.
        Some(col.min(self.cols - 1))
    }
}

/// `(amp, was_recording)` per pixel column. Empty buckets between filled
/// ones forward-fill from the previous value — each entry represents the
/// span from its capture moment to the next entry's, so missing buckets
/// inherit continuity rather than render as gaps.
fn waveform_amps(
    history: &VecDeque<LevelSample>,
    layout: &WaveformLayout,
) -> Vec<Option<(f32, bool)>> {
    let mut buckets: Vec<Option<(f32, bool)>> = vec![None; layout.cols];
    let mut last_off_left: Option<(f32, bool)> = None;

    for entry in history {
        match layout.sample_to_column(entry.sample) {
            Some(col) => {
                let (amp, recorded) = buckets[col].unwrap_or((0.0, false));
                buckets[col] = Some((amp.max(entry.peak), recorded || entry.recorded));
            }
            None if entry.sample < layout.leftmost_sample => {
                last_off_left = Some((entry.peak, entry.recorded));
            }
            None => {}
        }
    }

    // Stop the fill at the rightmost in-window entry so we don't leak
    // the last observation across columns that represent the "future"
    // beyond what's been captured yet.
    let Some(last_filled) = buckets.iter().rposition(|b| b.is_some()) else {
        return buckets;
    };
    let mut prev = last_off_left;
    for bucket in &mut buckets[..=last_filled] {
        match bucket {
            Some(v) => prev = Some(*v),
            None => *bucket = prev,
        }
    }
    buckets
}
