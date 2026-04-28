use chrono::{DateTime, Duration, Local};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Padding};

use crate::app::App;
use crate::ui::Action;
use crate::ui::footer;
use crate::ui::input;
use crate::ui::modals::ActiveModal;
use crate::ui::panels::{meters, recording, session, timeline, waveform};
use crate::ui::take_naming::TakeNaming;

const TOP_BAR_HEIGHT: u16 = 12;
const WAVEFORM_HEIGHT: u16 = 18;
const GAP: u16 = 1;
const TIMELINE_WIDTH: u16 = 32;
const TEMPLATE_FEEDBACK_SECS: i64 = 3;

/// The view layer's persistent state. App owns domain state; View owns
/// what the user sees and the modal lifecycle. Together they're passed
/// to draw and key-handling functions.
pub struct View {
    active_modal: Option<ActiveModal>,
    take_naming: Option<TakeNaming>,
    confirm_stop: bool,
    confirm_quit: bool,
    last_template_save: Option<Flash<String>>,
    last_template_load: Option<Flash<String>>,
}

/// Rails-style "flash": pairs a value with the moment it was set, so
/// the UI can render time-windowed status messages (e.g. "saved 'foo'"
/// for a few seconds after a save).
#[derive(Clone)]
pub struct Flash<T> {
    value: T,
    at: DateTime<Local>,
}

impl<T> Flash<T> {
    pub fn now(value: T) -> Self {
        Self {
            value,
            at: Local::now(),
        }
    }

    pub fn fresh_within(&self, secs: i64) -> Option<&T> {
        (Local::now() - self.at < Duration::seconds(secs)).then_some(&self.value)
    }
}

impl View {
    pub fn new() -> Self {
        Self {
            active_modal: None,
            take_naming: None,
            confirm_stop: false,
            confirm_quit: false,
            last_template_save: None,
            last_template_load: None,
        }
    }

    pub fn open_modal(&mut self, modal: ActiveModal) {
        self.active_modal = Some(modal);
    }

    pub fn open_take_naming(&mut self, overlay: TakeNaming) {
        self.take_naming = Some(overlay);
    }

    pub fn take_naming(&self) -> Option<&TakeNaming> {
        self.take_naming.as_ref()
    }

    pub fn open_confirm_stop(&mut self) {
        self.confirm_stop = true;
    }

    pub fn confirm_stop_active(&self) -> bool {
        self.confirm_stop
    }

    pub fn open_confirm_quit(&mut self) {
        self.confirm_quit = true;
    }

    pub fn confirm_quit_active(&self) -> bool {
        self.confirm_quit
    }

    pub fn flash_template_save(&mut self, name: String) {
        self.last_template_save = Some(Flash::now(name));
    }

    pub fn flash_template_load(&mut self, name: String) {
        self.last_template_load = Some(Flash::now(name));
    }

    pub fn recent_template_save(&self) -> Option<&str> {
        self.last_template_save
            .as_ref()
            .and_then(|t| t.fresh_within(TEMPLATE_FEEDBACK_SECS))
            .map(String::as_str)
    }

    pub fn recent_template_load(&self) -> Option<&str> {
        self.last_template_load
            .as_ref()
            .and_then(|t| t.fresh_within(TEMPLATE_FEEDBACK_SECS))
            .map(String::as_str)
    }

    pub fn draw(&self, frame: &mut Frame, app: &App) {
        let block = outer_block(app);
        let inner = block.inner(frame.area());
        frame.render_widget(block, frame.area());

        // Split inner vertically: body fills, footer is the bottom
        // line spanning the entire width (incl. under the timeline).
        let body_and_footer = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Fill(1),
                Constraint::Length(GAP),
                Constraint::Length(1),
            ])
            .split(inner);
        let body = body_and_footer[0];
        let footer_area = body_and_footer[2];

        // Body splits horizontally: main area on the left, timeline
        // panel pinned to the right edge.
        let h_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Fill(1), Constraint::Length(TIMELINE_WIDTH)])
            .split(body);
        let main = h_chunks[0];

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(TOP_BAR_HEIGHT),
                Constraint::Length(GAP),
                Constraint::Length(WAVEFORM_HEIGHT),
                Constraint::Length(GAP),
                Constraint::Fill(1),
            ])
            .split(main);

        let top_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .spacing(2)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(chunks[0]);

        session::draw(frame, top_chunks[0], app, self);
        recording::draw(frame, top_chunks[1], app);
        waveform::draw(frame, chunks[2], app);
        meters::draw(frame, chunks[4], app);
        timeline::draw(frame, h_chunks[1], app, self);
        footer::draw(frame, footer_area, app, self);

        if let Some(modal) = &self.active_modal {
            // Dim the back layer for visual hierarchy. Each modal's
            // `Clear` resets cell modifiers in its own rect, so the
            // modal itself stays bright after drawing.
            dim_buffer(frame);
            modal.draw(frame, app);
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent, app: &mut App) -> input::Outcome {
        // Inline overlays take priority over modals and over normal input.
        if let Some(mut overlay) = self.take_naming.take() {
            if matches!(overlay.handle_key(key, app), Action::Stay) {
                self.take_naming = Some(overlay);
            }
            return input::Outcome::Continue;
        }
        if self.confirm_stop {
            self.confirm_stop = false;
            if matches!(
                key.code,
                KeyCode::Enter | KeyCode::Char('y') | KeyCode::Char('Y')
            ) {
                app.stop_recording();
            }
            return input::Outcome::Continue;
        }
        if self.confirm_quit {
            self.confirm_quit = false;
            if matches!(
                key.code,
                KeyCode::Enter | KeyCode::Char('y') | KeyCode::Char('Y')
            ) {
                return input::Outcome::Quit;
            }
            return input::Outcome::Continue;
        }
        // Take the modal out of `self.active_modal` so we can pass `&mut self`
        // (for view-level state like flashes) alongside the modal call. Without
        // the take, the modal's borrow against `self.active_modal` would
        // alias the `self` we need to forward.
        if let Some(mut modal) = self.active_modal.take() {
            if matches!(modal.handle_key(key, app, self), Action::Stay) {
                self.active_modal = Some(modal);
            }
            return input::Outcome::Continue;
        }
        input::handle(key, app, self)
    }
}

fn dim_buffer(frame: &mut Frame) {
    let buffer = frame.buffer_mut();
    let area = buffer.area;
    for y in area.y..area.bottom() {
        for x in area.x..area.right() {
            buffer[(x, y)].modifier.insert(Modifier::DIM);
        }
    }
}

fn outer_block(app: &App) -> Block<'static> {
    let (title, color) = if app.is_recording() {
        (
            format!(" ● Recording — {} ", app.engine.device_name()),
            Color::Red,
        )
    } else {
        (format!(" {} ", app.engine.device_name()), Color::Cyan)
    };
    Block::default()
        .title(title)
        .borders(Borders::ALL)
        .padding(Padding::new(2, 2, 1, 1))
        .border_style(Style::default().fg(color))
}
