mod channel_picker;
mod device_picker;
mod main_view;
mod waveform;
mod widgets;

use std::io::{self, stdout};
use std::path::PathBuf;
use std::time::Duration;

use crossterm::{
    event::{self, Event, KeyCode, KeyEvent},
    execute,
    terminal::{self, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::prelude::*;

use crate::app::{App, AppState};

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

    let raw_dir = PathBuf::from("./recordings");
    std::fs::create_dir_all(&raw_dir)?;

    let mut app = App::new(device, raw_dir);
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
    NameLastUnbound,
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
}

fn decide(app: &App, key: KeyEvent) -> KeyAction {
    use KeyCode::*;
    if matches!(app.state, AppState::Recording { .. }) && app.timeline.is_naming_take() {
        return match key.code {
            Esc => KeyAction::CancelTakeNaming,
            Enter => KeyAction::CommitTakeNaming,
            Backspace => KeyAction::TakeNameBackspace,
            Char(c) => KeyAction::TakeNameAppendChar(c),
            _ => KeyAction::None,
        };
    }
    match &app.state {
        AppState::Idle => match key.code {
            Char('q') | Char('Q') | Esc => KeyAction::Quit,
            Char('r') | Char('R') => KeyAction::StartRecording,
            Char('c') | Char('C') => KeyAction::OpenPicker,
            Char('w') | Char('W') => KeyAction::CycleWaveformWindow,
            _ => KeyAction::None,
        },
        AppState::Recording { confirming_stop: false, .. } => match key.code {
            Esc => KeyAction::BeginConfirmStop,
            Char('w') | Char('W') => KeyAction::CycleWaveformWindow,
            Char(' ') => KeyAction::DropMarker,
            Char('t') | Char('T') => KeyAction::MarkAndName,
            Char('n') | Char('N') => KeyAction::NameLastUnbound,
            _ => KeyAction::None,
        },
        AppState::Recording { confirming_stop: true, .. } => match key.code {
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
        AppState::PickingChannel { renaming: Some(_), .. } => match key.code {
            Esc => KeyAction::PickerCancelRename,
            Enter => KeyAction::PickerCommitRename,
            Backspace => KeyAction::PickerBackspace,
            Char(c) => KeyAction::PickerAppendChar(c),
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
        KeyAction::BeginConfirmStop => {
            if let AppState::Recording { confirming_stop, .. } = &mut app.state {
                *confirming_stop = true;
            }
        }
        KeyAction::CancelConfirmStop => {
            if let AppState::Recording { confirming_stop, .. } = &mut app.state {
                *confirming_stop = false;
            }
        }
        KeyAction::StopRecording => app.stop_recording(),
        KeyAction::OpenPicker => {
            app.state = AppState::PickingChannel {
                cursor: 0,
                renaming: None,
            };
        }
        KeyAction::ClosePicker => {
            app.state = AppState::Idle;
        }
        KeyAction::CycleWaveformWindow => app.cycle_waveform_window(),
        KeyAction::DropMarker => app.drop_marker(),
        KeyAction::MarkAndName => app.mark_and_name(),
        KeyAction::NameLastUnbound => app.name_last_unbound(),
        KeyAction::CancelTakeNaming => app.timeline.cancel(),
        KeyAction::CommitTakeNaming => app.timeline.commit(),
        KeyAction::TakeNameAppendChar(c) => app.timeline.append_char(c),
        KeyAction::TakeNameBackspace => app.timeline.backspace(),
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
            let idx = picker_cursor_index(app);
            if let Some(idx) = idx {
                app.toggle_armed(idx);
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
            let (idx, label) = match (&app.state, picker_cursor_index(app)) {
                (AppState::PickingChannel { renaming: Some(buf), .. }, Some(idx)) => {
                    let label = if buf.trim().is_empty() {
                        None
                    } else {
                        Some(buf.trim().to_string())
                    };
                    (Some(idx), label)
                }
                _ => (None, None),
            };
            if let Some(idx) = idx {
                app.set_label(idx, label);
            }
            if let AppState::PickingChannel { renaming, .. } = &mut app.state {
                *renaming = None;
            }
        }
        KeyAction::PickerAppendChar(c) => {
            if let AppState::PickingChannel { renaming: Some(buf), .. } = &mut app.state {
                buf.push(c);
            }
        }
        KeyAction::PickerBackspace => {
            if let AppState::PickingChannel { renaming: Some(buf), .. } = &mut app.state {
                buf.pop();
            }
        }
    }
}

fn picker_cursor_index(app: &App) -> Option<crate::units::ChannelIndex> {
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
