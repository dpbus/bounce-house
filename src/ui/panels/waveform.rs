use std::collections::VecDeque;

use ratatui::prelude::*;
use ratatui::symbols::Marker;
use ratatui::widgets::canvas::{Canvas, Line as CanvasLine};
use ratatui::widgets::{Block, Borders};

use crate::app::App;
use crate::level_history::LevelSample;
use crate::ui::widgets::{
    BAND_GREEN, BAND_GREEN_DIM, BAND_RED, BAND_RED_DIM, BAND_YELLOW, BAND_YELLOW_DIM,
    band_thresholds, linear_to_db_fraction, take_color,
};

pub fn draw(frame: &mut Frame, area: Rect, app: &App) {
    let label = match app.mixer.level_history.window_secs() {
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
        app.mixer.level_history.window_secs(),
        cols,
        app.mixer.audio_input.sample_rate().0 as u64,
        app.mixer.audio_input.sample_position(),
    );
    let amps = waveform_amps(app.mixer.level_history.samples(), &layout);
    let marker_columns: Vec<(Option<u8>, usize)> = app
        .session
        .timeline()
        .markers()
        .iter()
        .filter_map(|m| {
            let abs = app.transport.relative_to_absolute(m.sample)?;
            let col = layout.sample_to_column(abs)?;
            Some((app.session.timeline().marker_color_index(m.sample), col))
        })
        .collect();
    let (warn, clip) = band_thresholds();
    let (warn, clip) = (warn as f64, clip as f64);
    // 1 braille pixel of vertical extent so the centerline stays visible
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
                    x1: x,
                    y1: -g_top,
                    x2: x,
                    y2: g_top,
                    color: green,
                });
                if half > warn {
                    let y_top = half.min(clip);
                    ctx.draw(&CanvasLine {
                        x1: x,
                        y1: warn,
                        x2: x,
                        y2: y_top,
                        color: yellow,
                    });
                    ctx.draw(&CanvasLine {
                        x1: x,
                        y1: -y_top,
                        x2: x,
                        y2: -warn,
                        color: yellow,
                    });
                }
                if half > clip {
                    ctx.draw(&CanvasLine {
                        x1: x,
                        y1: clip,
                        x2: x,
                        y2: half,
                        color: red,
                    });
                    ctx.draw(&CanvasLine {
                        x1: x,
                        y1: -half,
                        x2: x,
                        y2: -clip,
                        color: red,
                    });
                }
            }
        });
    frame.render_widget(canvas, canvas_area);

    // For close marker clusters (within 1 canvas_col), prefer take-bound
    // markers over unbound ones; otherwise keep the latest. Snap-invariant:
    // relative canvas_col deltas are preserved across grid shifts.
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
    /// Signed because in the early phase the visible window's left edge
    /// is "before recording started" — a region with no audio data.
    leftmost_sample: i64,
    samples_per_col: u64,
}

impl WaveformLayout {
    fn new(window_secs: u64, cols: usize, sample_rate: u64, current_sample: u64) -> Self {
        let visible_samples = window_secs.saturating_mul(sample_rate);
        let samples_per_col = (visible_samples / cols as u64).max(1);
        let current_bucket_start = (current_sample / samples_per_col) * samples_per_col;
        let leftmost_sample =
            current_bucket_start as i64 - (samples_per_col * (cols as u64 - 1)) as i64;
        Self {
            cols,
            leftmost_sample,
            samples_per_col,
        }
    }

    fn sample_to_column(&self, sample: u64) -> Option<usize> {
        let sample = sample as i64;
        if sample < self.leftmost_sample {
            return None;
        }
        let col = ((sample - self.leftmost_sample) as u64 / self.samples_per_col) as usize;
        // The right edge sample (= current_sample) computes to col == cols
        // due to integer division; clamp so the latest tick lands in the
        // rightmost column rather than getting skipped.
        Some(col.min(self.cols - 1))
    }
}

/// `(amp, was_recording)` per pixel column. Empty buckets between filled
/// ones forward-fill from the previous value: each entry represents the
/// span from its capture moment to the next entry's, so missing buckets
/// inherit continuity rather than rendering as gaps.
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
            None if (entry.sample as i64) < layout.leftmost_sample => {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn layout_at(window_secs: u64, cols: usize, current_sample: u64) -> WaveformLayout {
        WaveformLayout::new(window_secs, cols, 48_000, current_sample)
    }

    #[test]
    fn current_sample_lands_in_rightmost_column() {
        let layout = layout_at(10, 100, 480_000); // exactly 10s in
        assert_eq!(layout.sample_to_column(480_000), Some(99));
    }

    #[test]
    fn samples_before_window_return_none() {
        let layout = layout_at(10, 100, 1_000_000);
        // Anything before the leftmost is off-screen.
        assert_eq!(layout.sample_to_column(0), None);
    }

    #[test]
    fn leftmost_sample_is_one_window_back_from_current_bucket() {
        let layout = layout_at(10, 100, 480_000);
        // 10s × 48000 = 480_000 visible samples; samples_per_col = 4_800.
        // Current bucket starts at 480_000 (already aligned), leftmost at
        // 480_000 - 4_800 * 99 = 480_000 - 475_200 = 4_800.
        assert_eq!(layout.leftmost_sample, 4_800);
        assert_eq!(layout.samples_per_col, 4_800);
    }

    #[test]
    fn early_recording_has_negative_leftmost_sample() {
        // Only 5 samples in but 10s window → most of the window is "before
        // recording started." leftmost_sample should be negative.
        let layout = layout_at(10, 100, 5);
        assert!(layout.leftmost_sample < 0);
    }

    #[test]
    fn sample_to_column_distributes_across_columns() {
        let layout = layout_at(10, 100, 480_000);
        // Sample 240_000 (5s in, midpoint of window) → roughly col 50.
        let col = layout.sample_to_column(240_000).unwrap();
        assert!((49..=51).contains(&col), "expected ~50, got {col}");
    }

    #[test]
    fn sample_at_leftmost_lands_in_column_zero() {
        let layout = layout_at(10, 100, 480_000);
        let leftmost = layout.leftmost_sample as u64;
        assert_eq!(layout.sample_to_column(leftmost), Some(0));
    }

    #[test]
    fn samples_per_col_floors_at_one() {
        // Tiny window relative to many cols — samples_per_col would be 0
        // without the .max(1) guard. The math should still produce a
        // valid layout instead of dividing by zero.
        let layout = WaveformLayout::new(1, 100_000, 48_000, 0);
        assert_eq!(layout.samples_per_col, 1);
    }
}
