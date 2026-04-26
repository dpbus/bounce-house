use std::collections::VecDeque;

use ratatui::prelude::*;
use ratatui::symbols::Marker;
use ratatui::widgets::canvas::{Canvas, Line as CanvasLine};
use ratatui::widgets::{Block, Borders};

use crate::app::{App, TICK_FPS};
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
        app.level_history.len(),
        app.waveform_window_secs,
        cols,
    );
    let amps = waveform_amps(&app.level_history, &layout);
    let marker_columns: Vec<(Color, usize)> = app
        .timeline
        .markers()
        .iter()
        .filter_map(|m| {
            let col = layout.tick_to_column(m.tick, app.total_ticks)?;
            let color = app
                .timeline
                .marker_color_index(m.tick)
                .map(|i| take_color(i as usize))
                .unwrap_or(Color::DarkGray);
            Some((color, col))
        })
        .collect();
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

    let buf = frame.buffer_mut();
    for &(color, canvas_col) in &marker_columns {
        let term_col = inner.x + (canvas_col / 2) as u16;
        if term_col >= inner.right() {
            continue;
        }
        let glyph = if canvas_col % 2 == 0 { "▌" } else { "▐" };
        let style = Style::default().fg(color);
        buf.set_string(term_col, top_marker_y, glyph, style);
        buf.set_string(term_col, bottom_marker_y, glyph, style);
    }
}

/// Maps the rotating level history onto canvas columns. Buckets anchor to
/// absolute history indices, so each is sealed once its time has passed.
struct WaveformLayout {
    cols: usize,
    history_len: usize,
    bucket_size: usize,
    leftmost: i64,
}

impl WaveformLayout {
    fn new(history_len: usize, window_secs: u64, cols: usize) -> Self {
        let visible_ticks = window_secs as usize * TICK_FPS;
        let bucket_size = (visible_ticks / cols).max(1);
        let latest_bucket = (history_len / bucket_size) as i64;
        let leftmost = latest_bucket - (cols as i64 - 1);
        Self { cols, history_len, bucket_size, leftmost }
    }

    /// History range for column `col`, or `None` if outside the visible
    /// data (pre-recording past the left edge, or unfilled at the right).
    fn column_range(&self, col: usize) -> Option<(usize, usize)> {
        let bucket_idx = self.leftmost + col as i64;
        if bucket_idx < 0 {
            return None;
        }
        let start = bucket_idx as usize * self.bucket_size;
        let end = (start + self.bucket_size).min(self.history_len);
        if start >= end { None } else { Some((start, end)) }
    }

    /// Column for an absolute `tick`, or `None` if the tick has rotated
    /// out of the visible window.
    fn tick_to_column(&self, tick: u64, total_ticks: u64) -> Option<usize> {
        let oldest_visible = total_ticks.saturating_sub(self.history_len as u64);
        if tick < oldest_visible || tick >= total_ticks {
            return None;
        }
        let history_index = (tick - oldest_visible) as usize;
        let bucket_idx = (history_index / self.bucket_size) as i64;
        let col = bucket_idx - self.leftmost;
        (col >= 0 && (col as usize) < self.cols).then_some(col as usize)
    }
}

/// `(amp, was_recording)` per pixel column. The `was_recording` flag is
/// true if any sample in the bucket was captured to disk.
fn waveform_amps(
    history: &VecDeque<(f32, bool)>,
    layout: &WaveformLayout,
) -> Vec<Option<(f32, bool)>> {
    (0..layout.cols)
        .map(|col| {
            let (start, end) = layout.column_range(col)?;
            let (amp, recorded) = history.range(start..end).fold(
                (0.0f32, false),
                |(amx, rec), &(a, r)| (amx.max(a), rec || r),
            );
            Some((amp, recorded))
        })
        .collect()
}
