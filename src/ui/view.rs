use chrono::{DateTime, Duration, Local};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Padding};

use crate::app::App;
use crate::dispatch::{Action, fire};
use crate::transport::TransportMode;
use crate::ui::ChannelStrips;
use crate::ui::ModalOutcome;
use crate::ui::footer;
use crate::ui::header;
use crate::ui::input;
use crate::ui::modals::ActiveModal;
use crate::ui::panels::{timeline, waveform};
use crate::ui::take_naming::TakeNaming;

const HEADER_HEIGHT: u16 = 1;
const WAVEFORM_HEIGHT: u16 = 22;
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
    pub channel_strips: ChannelStrips,
    pub total_ticks: u64,
    /// App-launch time, for the "Session HH:MM:SS" header timer.
    pub started_at: DateTime<Local>,
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
            channel_strips: ChannelStrips::new(),
            total_ticks: 0,
            started_at: Local::now(),
        }
    }

    pub fn tick(&mut self) {
        self.total_ticks += 1;
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

        // Header and footer span full inner width; the body in between
        // splits horizontally so the timeline panel pins to the right.
        let outer_v = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(HEADER_HEIGHT),
                Constraint::Length(GAP),
                Constraint::Fill(1),
                Constraint::Length(GAP),
                Constraint::Length(1),
            ])
            .split(inner);
        let header_area = outer_v[0];
        let body = outer_v[2];
        let footer_area = outer_v[4];

        let body_h = Layout::default()
            .direction(Direction::Horizontal)
            .spacing(1)
            .constraints([Constraint::Fill(1), Constraint::Length(TIMELINE_WIDTH)])
            .split(body);
        let main = body_h[0];

        let main_v = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(WAVEFORM_HEIGHT),
                Constraint::Length(GAP),
                Constraint::Fill(1),
            ])
            .split(main);

        header::draw(frame, header_area, app, self);
        waveform::draw(frame, main_v[0], app);
        self.channel_strips.draw(frame, main_v[2], app);
        timeline::draw(frame, body_h[1], app, self);
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
            if matches!(overlay.handle_key(key, app), ModalOutcome::Stay) {
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
                fire(Action::StopRecording, app);
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
            if matches!(modal.handle_key(key, app, self), ModalOutcome::Stay) {
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
    let (title, color) = match app.transport.mode() {
        TransportMode::Recording => (
            format!(" ● Recording — {} ", app.mixer.input_device.name()),
            Color::Red,
        ),
        TransportMode::Paused => (
            format!(" ⏸ Paused — {} ", app.mixer.input_device.name()),
            Color::Yellow,
        ),
        TransportMode::Playing => {
            let name = app
                .mixer
                .output_device
                .as_ref()
                .expect("Playing requires output_device")
                .name();
            (format!(" ▶ Playing — {name} "), Color::Green)
        }
        TransportMode::Idle => (format!(" {} ", app.mixer.input_device.name()), Color::Cyan),
    };
    Block::default()
        .title(title)
        .borders(Borders::ALL)
        .padding(Padding::new(2, 2, 1, 1))
        .border_style(Style::default().fg(color))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::take_naming::TakeNaming;

    #[test]
    fn new_view_has_no_active_overlays() {
        let view = View::new();
        assert!(view.active_modal.is_none());
        assert!(view.take_naming().is_none());
        assert!(!view.confirm_stop_active());
        assert!(!view.confirm_quit_active());
        assert!(view.recent_template_save().is_none());
        assert!(view.recent_template_load().is_none());
    }

    #[test]
    fn open_take_naming_sets_overlay() {
        let mut view = View::new();
        view.open_take_naming(TakeNaming::fresh());
        assert!(view.take_naming().is_some());
    }

    #[test]
    fn open_confirm_stop_and_quit_track_state() {
        let mut view = View::new();
        view.open_confirm_stop();
        assert!(view.confirm_stop_active());
        assert!(!view.confirm_quit_active());

        let mut view = View::new();
        view.open_confirm_quit();
        assert!(view.confirm_quit_active());
        assert!(!view.confirm_stop_active());
    }

    #[test]
    fn flash_template_save_surfaces_through_recent() {
        let mut view = View::new();
        view.flash_template_save("drums".into());
        assert_eq!(view.recent_template_save(), Some("drums"));
    }

    #[test]
    fn flash_template_load_surfaces_through_recent() {
        let mut view = View::new();
        view.flash_template_load("vocals".into());
        assert_eq!(view.recent_template_load(), Some("vocals"));
    }

    #[test]
    fn flash_save_and_load_are_independent() {
        let mut view = View::new();
        view.flash_template_save("a".into());
        view.flash_template_load("b".into());
        assert_eq!(view.recent_template_save(), Some("a"));
        assert_eq!(view.recent_template_load(), Some("b"));
    }

    #[test]
    fn flash_expires_after_window() {
        // Construct a Flash with an `at` time in the past.
        let stale = Flash {
            value: "old".to_string(),
            at: Local::now() - Duration::seconds(TEMPLATE_FEEDBACK_SECS + 1),
        };
        assert!(stale.fresh_within(TEMPLATE_FEEDBACK_SECS).is_none());

        let fresh = Flash::now("new".to_string());
        assert_eq!(
            fresh.fresh_within(TEMPLATE_FEEDBACK_SECS),
            Some(&"new".to_string())
        );
    }

    #[test]
    fn recent_template_returns_none_after_window_passes() {
        let mut view = View::new();
        view.last_template_save = Some(Flash {
            value: "old".to_string(),
            at: Local::now() - Duration::seconds(TEMPLATE_FEEDBACK_SECS + 1),
        });
        assert!(view.recent_template_save().is_none());
    }
}
