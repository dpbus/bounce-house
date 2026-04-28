use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::{App, AppState};
use crate::ui::modals::{ActiveModal, ChannelPickerModal, LoadTemplateModal, SaveTemplateModal};
use crate::ui::view::View;

pub enum Outcome {
    Continue,
    Quit,
}

pub fn handle(key: KeyEvent, app: &mut App, view: &mut View) -> Outcome {
    let action = decide(app, key);
    if matches!(action, KeyAction::Quit) {
        return Outcome::Quit;
    }
    apply(app, view, action);
    Outcome::Continue
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
    OpenChannelPicker,
    CycleWaveformWindow,
    DropMarker,
    MarkAndName,
    NameTake,
    DeleteLastMarker,
    CancelTakeNaming,
    CommitTakeNaming,
    TakeNameAppendChar(char),
    TakeNameBackspace,
    OpenSaveTemplate,
    OpenLoadTemplate,
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
        AppState::ConfirmingStop => match key.code {
            Esc => KeyAction::StopRecording,
            _ => KeyAction::CancelConfirmStop,
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
                KeyAction::OpenSaveTemplate
            }
            Char('o') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                KeyAction::OpenLoadTemplate
            }
            Char('q') | Char('Q') | Esc => KeyAction::Quit,
            Char('r') | Char('R') => KeyAction::StartRecording,
            Char('c') | Char('C') => KeyAction::OpenChannelPicker,
            Char('w') | Char('W') => KeyAction::CycleWaveformWindow,
            Char('n') | Char('N') => KeyAction::NameTake,
            _ => KeyAction::None,
        },
    }
}

fn apply(app: &mut App, view: &mut View, action: KeyAction) {
    match action {
        KeyAction::None | KeyAction::Quit => {}
        KeyAction::StartRecording => {
            let _ = app.start_recording();
        }
        KeyAction::BeginConfirmStop => app.begin_confirm_stop(),
        KeyAction::CancelConfirmStop => app.cancel_confirm_stop(),
        KeyAction::StopRecording => app.stop_recording(),
        KeyAction::CycleWaveformWindow => app.cycle_waveform_window(),
        KeyAction::DropMarker => app.drop_marker(),
        KeyAction::MarkAndName => app.mark_and_name(),
        KeyAction::NameTake => app.name_take(),
        KeyAction::DeleteLastMarker => app.delete_last_marker(),
        KeyAction::CancelTakeNaming => app.cancel_take_naming(),
        KeyAction::CommitTakeNaming => app.commit_take_naming(),
        KeyAction::TakeNameAppendChar(c) => app.take_name_append_char(c),
        KeyAction::TakeNameBackspace => app.take_name_backspace(),
        KeyAction::OpenChannelPicker => {
            if !app.is_recording() {
                view.open_modal(ActiveModal::ChannelPicker(ChannelPickerModal::new()));
            }
        }
        KeyAction::OpenSaveTemplate => {
            view.open_modal(ActiveModal::SaveTemplate(SaveTemplateModal::new()));
        }
        KeyAction::OpenLoadTemplate => {
            view.open_modal(ActiveModal::LoadTemplate(LoadTemplateModal::new(
                app.list_templates(),
            )));
        }
    }
}
