use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::App;
use crate::ui::modals::{
    ActiveModal, ChannelPickerModal, HelpModal, LoadTemplateModal, SaveTemplateModal,
    SettingsModal,
};
use crate::ui::take_naming::TakeNaming;
use crate::ui::view::View;

pub enum Outcome {
    Continue,
    Quit,
}

pub fn handle(key: KeyEvent, app: &mut App, view: &mut View) -> Outcome {
    apply(app, view, decide(app, key));
    Outcome::Continue
}

/// Decisions made by inspecting key + current state. Kept separate
/// from `apply` so `decide` is testable against `&App` without needing
/// the mutable plumbing.
enum KeyAction {
    None,
    StartRecording,
    OpenConfirmStop,
    OpenConfirmQuit,
    OpenChannelPicker,
    CycleWaveformWindow,
    DropMarker,
    MarkAndOpenTakeNaming,
    OpenRetroactiveTakeNaming,
    DeleteLastMarker,
    OpenSaveTemplate,
    OpenLoadTemplate,
    OpenSettings,
    OpenHelp,
}

fn decide(app: &App, key: KeyEvent) -> KeyAction {
    use KeyCode::*;
    if app.is_recording() {
        return match key.code {
            Esc | Char('r') | Char('R') => KeyAction::OpenConfirmStop,
            Char('w') | Char('W') => KeyAction::CycleWaveformWindow,
            Char(' ') => KeyAction::DropMarker,
            Char('t') | Char('T') => KeyAction::MarkAndOpenTakeNaming,
            Char('n') | Char('N') => KeyAction::OpenRetroactiveTakeNaming,
            Backspace => KeyAction::DeleteLastMarker,
            Char('?') => KeyAction::OpenHelp,
            _ => KeyAction::None,
        };
    }
    match key.code {
        Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => KeyAction::OpenSaveTemplate,
        Char('l') if key.modifiers.contains(KeyModifiers::CONTROL) => KeyAction::OpenLoadTemplate,
        Char(',') => KeyAction::OpenSettings,
        Char('q') | Char('Q') => KeyAction::OpenConfirmQuit,
        Char('r') | Char('R') => KeyAction::StartRecording,
        Char('c') | Char('C') => KeyAction::OpenChannelPicker,
        Char('w') | Char('W') => KeyAction::CycleWaveformWindow,
        Char('n') | Char('N') => KeyAction::OpenRetroactiveTakeNaming,
        Char('?') => KeyAction::OpenHelp,
        _ => KeyAction::None,
    }
}

fn apply(app: &mut App, view: &mut View, action: KeyAction) {
    match action {
        KeyAction::None => {}
        KeyAction::StartRecording => {
            let _ = app.start_recording();
        }
        KeyAction::OpenConfirmStop => view.open_confirm_stop(),
        KeyAction::OpenConfirmQuit => view.open_confirm_quit(),
        KeyAction::CycleWaveformWindow => app.cycle_waveform_window(),
        KeyAction::DropMarker => app.drop_marker(),
        KeyAction::MarkAndOpenTakeNaming => {
            app.drop_marker();
            view.open_take_naming(TakeNaming::fresh());
        }
        KeyAction::OpenRetroactiveTakeNaming => {
            if app.has_unbound_marker() {
                view.open_take_naming(TakeNaming::retroactive());
            }
        }
        KeyAction::DeleteLastMarker => app.delete_last_marker(),
        KeyAction::OpenChannelPicker => {
            view.open_modal(ActiveModal::ChannelPicker(ChannelPickerModal::new()));
        }
        KeyAction::OpenSaveTemplate => {
            view.open_modal(ActiveModal::SaveTemplate(SaveTemplateModal::new()));
        }
        KeyAction::OpenLoadTemplate => {
            view.open_modal(ActiveModal::LoadTemplate(LoadTemplateModal::new(
                app.list_templates(),
            )));
        }
        KeyAction::OpenSettings => {
            view.open_modal(ActiveModal::Settings(SettingsModal::new(&app.settings)));
        }
        KeyAction::OpenHelp => {
            view.open_modal(ActiveModal::Help(HelpModal::new()));
        }
    }
}
