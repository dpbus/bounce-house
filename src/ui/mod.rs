mod channel_picker;
mod device_picker;
mod main_view;
mod meter_panel;
mod recording_panel;
mod session_panel;
mod template_save;
mod waveform;
mod widgets;

use std::io::{self, stdout};
use std::time::Duration;

use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    execute,
    terminal::{self, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::prelude::*;

use crate::app::{App, AppState};
use crate::config::Config;

pub fn run() -> io::Result<()> {
    terminal::enable_raw_mode()?;
    execute!(stdout(), EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;

    let result = bootstrap(&mut terminal);

    terminal::disable_raw_mode()?;
    execute!(stdout(), LeaveAlternateScreen)?;

    result
}

fn bootstrap(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> io::Result<()> {
    let device = match device_picker::pick(terminal) {
        Ok(d) => d,
        Err(e) if e.kind() == io::ErrorKind::Interrupted => return Ok(()),
        Err(e) => return Err(e),
    };

    let config = Config::load_or_create()?;

    let mut app = App::new(device, config);
    main_loop(terminal, &mut app)
}

fn main_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
) -> io::Result<()> {
    loop {
        app.tick_display();

        terminal.draw(|frame| {
            main_view::draw(frame, app);
            if matches!(app.state, AppState::PickingChannel { .. }) {
                channel_picker::draw(frame, app);
            }
            if matches!(app.state, AppState::SavingTemplate { .. }) {
                template_save::draw(frame, app);
            }
        })?;

        if event::poll(Duration::from_millis(16))? {
            if let Event::Key(key) = event::read()? {
                match decide(app, key) {
                    KeyAction::Quit => break,
                    KeyAction::None => {}
                    other => apply(app, other),
                }
            }
        }
    }
    Ok(())
}

/// Decisions made by inspecting key + current state. Kept separate from
/// mutation so the borrow against `&app.state` doesn't conflict with the
/// `&mut app` we need to act.
enum KeyAction {
    None,
    Quit,
    StartRecording,
    BeginConfirmStop,
    CancelConfirmStop,
    StopRecording,
    OpenPicker,
    ClosePicker,
    CycleWaveformWindow,
    DropMarker,
    MarkAndName,
    NameTake,
    DeleteLastMarker,
    CancelTakeNaming,
    CommitTakeNaming,
    TakeNameAppendChar(char),
    TakeNameBackspace,
    PickerCursorUp,
    PickerCursorDown,
    PickerToggleArmed,
    PickerStartRename,
    PickerCancelRename,
    PickerCommitRename,
    PickerAppendChar(char),
    PickerBackspace,
    BeginSaveTemplate,
    CancelSaveTemplate,
    CommitSaveTemplate,
    SaveTemplateAppendChar(char),
    SaveTemplateBackspace,
}

fn decide(app: &App, key: KeyEvent) -> KeyAction {
    use KeyCode::*;
    match &app.state {
        AppState::NamingTake { .. } => match key.code {
            Esc => KeyAction::CancelTakeNaming,
            Enter => KeyAction::CommitTakeNaming,
            Backspace => KeyAction::TakeNameBackspace,
            Char(c) => KeyAction::TakeNameAppendChar(c),
            _ => KeyAction::None,
        },
        AppState::SavingTemplate { .. } => match key.code {
            Esc => KeyAction::CancelSaveTemplate,
            Enter => KeyAction::CommitSaveTemplate,
            Backspace => KeyAction::SaveTemplateBackspace,
            Char(c) => KeyAction::SaveTemplateAppendChar(c),
            _ => KeyAction::None,
        },
        AppState::ConfirmingStop => match key.code {
            Esc => KeyAction::StopRecording,
            _ => KeyAction::CancelConfirmStop,
        },
        AppState::PickingChannel { renaming: None, .. } => match key.code {
            Esc => KeyAction::ClosePicker,
            Up | Char('k') => KeyAction::PickerCursorUp,
            Down | Char('j') => KeyAction::PickerCursorDown,
            Char(' ') => KeyAction::PickerToggleArmed,
            Tab => KeyAction::PickerStartRename,
            _ => KeyAction::None,
        },
        AppState::PickingChannel {
            renaming: Some(_), ..
        } => match key.code {
            Esc => KeyAction::PickerCancelRename,
            Enter => KeyAction::PickerCommitRename,
            Backspace => KeyAction::PickerBackspace,
            Char(c) => KeyAction::PickerAppendChar(c),
            _ => KeyAction::None,
        },
        AppState::Default if app.is_recording() => match key.code {
            Esc => KeyAction::BeginConfirmStop,
            Char('w') | Char('W') => KeyAction::CycleWaveformWindow,
            Char(' ') => KeyAction::DropMarker,
            Char('t') | Char('T') => KeyAction::MarkAndName,
            Char('n') | Char('N') => KeyAction::NameTake,
            Backspace => KeyAction::DeleteLastMarker,
            _ => KeyAction::None,
        },
        AppState::Default => match key.code {
            Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                KeyAction::BeginSaveTemplate
            }
            Char('q') | Char('Q') | Esc => KeyAction::Quit,
            Char('r') | Char('R') => KeyAction::StartRecording,
            Char('c') | Char('C') => KeyAction::OpenPicker,
            Char('w') | Char('W') => KeyAction::CycleWaveformWindow,
            Char('n') | Char('N') => KeyAction::NameTake,
            _ => KeyAction::None,
        },
    }
}

fn apply(app: &mut App, action: KeyAction) {
    match action {
        KeyAction::None | KeyAction::Quit => {}
        KeyAction::StartRecording => {
            let _ = app.start_recording();
        }
        KeyAction::BeginConfirmStop => app.begin_confirm_stop(),
        KeyAction::CancelConfirmStop => app.cancel_confirm_stop(),
        KeyAction::StopRecording => app.stop_recording(),
        KeyAction::OpenPicker => app.open_picker(),
        KeyAction::ClosePicker => app.close_picker(),
        KeyAction::CycleWaveformWindow => app.cycle_waveform_window(),
        KeyAction::DropMarker => app.drop_marker(),
        KeyAction::MarkAndName => app.mark_and_name(),
        KeyAction::NameTake => app.name_take(),
        KeyAction::DeleteLastMarker => app.delete_last_marker(),
        KeyAction::CancelTakeNaming => app.cancel_take_naming(),
        KeyAction::CommitTakeNaming => app.commit_take_naming(),
        KeyAction::TakeNameAppendChar(c) => app.take_name_append_char(c),
        KeyAction::TakeNameBackspace => app.take_name_backspace(),
        KeyAction::PickerCursorUp => {
            if let AppState::PickingChannel { cursor, .. } = &mut app.state {
                if *cursor > 0 {
                    *cursor -= 1;
                }
            }
        }
        KeyAction::PickerCursorDown => {
            let max = app.session.channels.len().saturating_sub(1);
            if let AppState::PickingChannel { cursor, .. } = &mut app.state {
                if *cursor < max {
                    *cursor += 1;
                }
            }
        }
        KeyAction::PickerToggleArmed => {
            if let Some(channel_index) = focused_channel_index(app) {
                app.toggle_armed(channel_index);
            }
        }
        KeyAction::PickerStartRename => {
            let current = picker_cursor_label(app).unwrap_or_default();
            if let AppState::PickingChannel { renaming, .. } = &mut app.state {
                *renaming = Some(current);
            }
        }
        KeyAction::PickerCancelRename => {
            if let AppState::PickingChannel { renaming, .. } = &mut app.state {
                *renaming = None;
            }
        }
        KeyAction::PickerCommitRename => {
            let (channel_index, label) = match (&app.state, focused_channel_index(app)) {
                (
                    AppState::PickingChannel {
                        renaming: Some(buf),
                        ..
                    },
                    Some(ci),
                ) => {
                    let label = if buf.trim().is_empty() {
                        None
                    } else {
                        Some(buf.trim().to_string())
                    };
                    (Some(ci), label)
                }
                _ => (None, None),
            };
            if let Some(channel_index) = channel_index {
                app.set_label(channel_index, label);
            }
            if let AppState::PickingChannel { renaming, .. } = &mut app.state {
                *renaming = None;
            }
        }
        KeyAction::PickerAppendChar(c) => {
            if let AppState::PickingChannel {
                renaming: Some(buf),
                ..
            } = &mut app.state
            {
                buf.push(c);
            }
        }
        KeyAction::PickerBackspace => {
            if let AppState::PickingChannel {
                renaming: Some(buf),
                ..
            } = &mut app.state
            {
                buf.pop();
            }
        }
        KeyAction::BeginSaveTemplate => app.begin_save_template(),
        KeyAction::CancelSaveTemplate => app.cancel_save_template(),
        KeyAction::CommitSaveTemplate => app.commit_save_template(),
        KeyAction::SaveTemplateAppendChar(c) => app.save_template_append_char(c),
        KeyAction::SaveTemplateBackspace => app.save_template_backspace(),
    }
}

fn focused_channel_index(app: &App) -> Option<u16> {
    if let AppState::PickingChannel { cursor, .. } = &app.state {
        app.session.channels.get(*cursor).map(|c| c.index)
    } else {
        None
    }
}

fn picker_cursor_label(app: &App) -> Option<String> {
    if let AppState::PickingChannel { cursor, .. } = &app.state {
        app.session
            .channels
            .get(*cursor)
            .and_then(|c| c.label.clone())
            .or(Some(String::new()))
    } else {
        None
    }
}
