use crossterm::event::{KeyCode, KeyEvent};
use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

use crate::app::App;
use crate::template;
use crate::ui::Action;
use crate::ui::text_input::TextInput;
use crate::ui::view::View;
use crate::ui::widgets::{
    channel_preview_row, channels_summary, flow_columns, input_with_cursor, key_hint, labeled,
    modal,
};

const COL_WIDTH: u16 = 20;
const MAX_COLS: u16 = 4;
const HEADER_ROWS: u16 = 2;
const INPUT_ROW: u16 = 1;
const HINTS_ROW: u16 = 1;
const GAP_ROW: u16 = 1;
const NUM_GAPS: u16 = 3; // before grid, after grid, after input
const INNER_MARGIN: u16 = 2;
const MIN_CONTENT_WIDTH: u16 = 50;

const CONTENT_CHROME_ROWS: u16 =
    HEADER_ROWS + INPUT_ROW + HINTS_ROW + (GAP_ROW * NUM_GAPS) + INNER_MARGIN;

struct ContentLayout {
    cols: u16,
    width: u16,
    height: u16,
}

pub struct SaveTemplateModal {
    input: TextInput,
}

impl SaveTemplateModal {
    pub fn new() -> Self {
        Self {
            input: TextInput::new(),
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent, app: &mut App, view: &mut View) -> Action {
        match key.code {
            KeyCode::Esc => Action::Close,
            KeyCode::Enter => {
                let name = self.input.value().trim();
                if !template::is_valid_name(name) {
                    return Action::Stay;
                }
                if app.save_template(name).is_ok() {
                    view.flash_template_save(name.to_string());
                }
                Action::Close
            }
            _ => {
                self.input.handle_edit_key(key);
                Action::Stay
            }
        }
    }

    pub fn draw(&self, frame: &mut Frame, app: &App) {
        let n_channels = app.session.channels().len() as u16;
        let layout = pick_layout(n_channels);
        let inner = modal(frame, "Save Template", layout.width, layout.height);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([
                Constraint::Length(HEADER_ROWS),
                Constraint::Length(GAP_ROW),
                Constraint::Min(1),
                Constraint::Length(GAP_ROW),
                Constraint::Length(INPUT_ROW),
                Constraint::Length(GAP_ROW),
                Constraint::Length(HINTS_ROW),
            ])
            .split(inner);

        frame.render_widget(Paragraph::new(header_lines(app)), chunks[0]);
        let channels: Vec<Line<'static>> = app
            .session
            .channels()
            .iter()
            .map(channel_preview_row)
            .collect();
        flow_columns(frame, chunks[2], &channels, layout.cols as u32);
        frame.render_widget(Paragraph::new(save_as_line(&self.input)), chunks[4]);
        frame.render_widget(Paragraph::new(hints_line()).centered(), chunks[6]);
    }
}

/// Picks the layout that fits all channels: more columns for higher counts,
/// capped at MAX_COLS. Width clamped to MIN_CONTENT_WIDTH so the header
/// and hints lines don't get squeezed at low channel counts.
fn pick_layout(n_channels: u16) -> ContentLayout {
    let cols = match n_channels {
        0..=8 => 1,
        9..=20 => 2,
        21..=40 => 3,
        _ => MAX_COLS,
    };
    let rows = n_channels.div_ceil(cols).max(1);
    let raw_width = cols * COL_WIDTH + cols.saturating_sub(1) + INNER_MARGIN;
    ContentLayout {
        cols,
        width: raw_width.max(MIN_CONTENT_WIDTH),
        height: CONTENT_CHROME_ROWS + rows,
    }
}

fn header_lines(app: &App) -> Vec<Line<'static>> {
    let total = app.engine.channel_count() as usize;
    let armed = app.session.armed_channels().count();
    vec![
        labeled("Device:    ", app.engine.device_name().to_string()),
        labeled("Channels:  ", channels_summary(armed, total)),
    ]
}

fn save_as_line(input: &TextInput) -> Line<'static> {
    let mut spans = vec![Span::styled(
        "Save as:   ",
        Style::default().fg(Color::Yellow),
    )];
    spans.extend(input_with_cursor(input, Style::default()));
    Line::from(spans)
}

fn hints_line() -> Line<'static> {
    let mut spans = Vec::new();
    spans.extend(key_hint("Enter", "save  ", Color::Cyan));
    spans.extend(key_hint("Esc", "cancel", Color::Cyan));
    Line::from(spans)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pick_layout_uses_one_column_for_small_counts() {
        assert_eq!(pick_layout(0).cols, 1);
        assert_eq!(pick_layout(1).cols, 1);
        assert_eq!(pick_layout(8).cols, 1);
    }

    #[test]
    fn pick_layout_grows_columns_with_channel_count() {
        assert_eq!(pick_layout(9).cols, 2);
        assert_eq!(pick_layout(20).cols, 2);
        assert_eq!(pick_layout(21).cols, 3);
        assert_eq!(pick_layout(40).cols, 3);
    }

    #[test]
    fn pick_layout_caps_at_max_cols() {
        let layout = pick_layout(100);
        assert_eq!(layout.cols, MAX_COLS);
        let bigger = pick_layout(1000);
        assert_eq!(bigger.cols, MAX_COLS);
    }

    #[test]
    fn pick_layout_height_covers_all_channels() {
        // height = CONTENT_CHROME_ROWS + rows, where rows × cols ≥ n.
        for n in [1u16, 7, 8, 9, 20, 21, 40, 41, 64, 100] {
            let layout = pick_layout(n);
            let rows = layout.height - CONTENT_CHROME_ROWS;
            assert!(
                rows * layout.cols >= n,
                "n={n}: rows {rows} × cols {} < {n}",
                layout.cols
            );
        }
    }

    #[test]
    fn pick_layout_width_respects_minimum() {
        // Even at the smallest channel count, header/hints fit.
        let layout = pick_layout(1);
        assert!(layout.width >= MIN_CONTENT_WIDTH);
    }

    #[test]
    fn pick_layout_height_includes_chrome() {
        // Empty channels case still allocates room for header + input + hints.
        let layout = pick_layout(0);
        assert!(layout.height > CONTENT_CHROME_ROWS);
    }
}
