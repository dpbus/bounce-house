use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::{App, RecordingState};
use crate::ui::modals::{
    ActiveModal, ChannelPickerModal, HelpModal, LoadTemplateModal, SaveTemplateModal, SettingsModal,
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
#[derive(Debug, PartialEq, Eq)]
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
    TogglePause,
    OpenSaveTemplate,
    OpenLoadTemplate,
    OpenSettings,
    OpenHelp,
    ScrollStripsLeft,
    ScrollStripsRight,
}

fn decide(app: &App, key: KeyEvent) -> KeyAction {
    decide_with_state(app.recording_state(), key)
}

fn decide_with_state(state: RecordingState, key: KeyEvent) -> KeyAction {
    use KeyCode::*;
    match state {
        RecordingState::Recording => match key.code {
            Esc | Char('r') | Char('R') => KeyAction::OpenConfirmStop,
            Char('w') | Char('W') => KeyAction::CycleWaveformWindow,
            Char(' ') => KeyAction::DropMarker,
            Char('t') | Char('T') => KeyAction::MarkAndOpenTakeNaming,
            Char('n') | Char('N') => KeyAction::OpenRetroactiveTakeNaming,
            Char('p') | Char('P') => KeyAction::TogglePause,
            Char('[') => KeyAction::ScrollStripsLeft,
            Char(']') => KeyAction::ScrollStripsRight,
            Backspace => KeyAction::DeleteLastMarker,
            Char('?') => KeyAction::OpenHelp,
            _ => KeyAction::None,
        },
        RecordingState::Paused => match key.code {
            Esc | Char('r') | Char('R') => KeyAction::OpenConfirmStop,
            Char('w') | Char('W') => KeyAction::CycleWaveformWindow,
            Char('n') | Char('N') => KeyAction::OpenRetroactiveTakeNaming,
            Char('p') | Char('P') => KeyAction::TogglePause,
            Char('[') => KeyAction::ScrollStripsLeft,
            Char(']') => KeyAction::ScrollStripsRight,
            Backspace => KeyAction::DeleteLastMarker,
            Char('?') => KeyAction::OpenHelp,
            _ => KeyAction::None,
        },
        RecordingState::Idle => match key.code {
            Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                KeyAction::OpenSaveTemplate
            }
            Char('l') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                KeyAction::OpenLoadTemplate
            }
            Char(',') => KeyAction::OpenSettings,
            Char('q') | Char('Q') | Esc => KeyAction::OpenConfirmQuit,
            Char('r') | Char('R') => KeyAction::StartRecording,
            Char('c') | Char('C') => KeyAction::OpenChannelPicker,
            Char('w') | Char('W') => KeyAction::CycleWaveformWindow,
            Char('n') | Char('N') => KeyAction::OpenRetroactiveTakeNaming,
            Char('[') => KeyAction::ScrollStripsLeft,
            Char(']') => KeyAction::ScrollStripsRight,
            Char('?') => KeyAction::OpenHelp,
            _ => KeyAction::None,
        },
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
        KeyAction::TogglePause => app.toggle_pause(),
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
        KeyAction::ScrollStripsLeft => app.scroll_strips_left(1),
        KeyAction::ScrollStripsRight => app.scroll_strips_right(1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn ctrl(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::CONTROL)
    }

    #[test]
    fn idle_r_starts_recording() {
        assert_eq!(
            decide_with_state(RecordingState::Idle, key(KeyCode::Char('r'))),
            KeyAction::StartRecording
        );
        assert_eq!(
            decide_with_state(RecordingState::Idle, key(KeyCode::Char('R'))),
            KeyAction::StartRecording
        );
    }

    #[test]
    fn idle_q_or_esc_opens_quit_confirm() {
        for code in [KeyCode::Char('q'), KeyCode::Char('Q'), KeyCode::Esc] {
            assert_eq!(
                decide_with_state(RecordingState::Idle, key(code)),
                KeyAction::OpenConfirmQuit,
                "unexpected for {code:?}"
            );
        }
    }

    #[test]
    fn idle_c_opens_channel_picker() {
        assert_eq!(
            decide_with_state(RecordingState::Idle, key(KeyCode::Char('c'))),
            KeyAction::OpenChannelPicker
        );
    }

    #[test]
    fn idle_n_opens_retroactive_take_naming() {
        assert_eq!(
            decide_with_state(RecordingState::Idle, key(KeyCode::Char('n'))),
            KeyAction::OpenRetroactiveTakeNaming
        );
    }

    #[test]
    fn idle_ctrl_s_opens_save_template() {
        assert_eq!(
            decide_with_state(RecordingState::Idle, ctrl(KeyCode::Char('s'))),
            KeyAction::OpenSaveTemplate
        );
        // Bare 's' should not.
        assert_eq!(
            decide_with_state(RecordingState::Idle, key(KeyCode::Char('s'))),
            KeyAction::None
        );
    }

    #[test]
    fn idle_ctrl_l_opens_load_template() {
        assert_eq!(
            decide_with_state(RecordingState::Idle, ctrl(KeyCode::Char('l'))),
            KeyAction::OpenLoadTemplate
        );
    }

    #[test]
    fn idle_comma_opens_settings() {
        assert_eq!(
            decide_with_state(RecordingState::Idle, key(KeyCode::Char(','))),
            KeyAction::OpenSettings
        );
    }

    #[test]
    fn idle_question_opens_help() {
        assert_eq!(
            decide_with_state(RecordingState::Idle, key(KeyCode::Char('?'))),
            KeyAction::OpenHelp
        );
    }

    #[test]
    fn idle_w_cycles_waveform_window() {
        assert_eq!(
            decide_with_state(RecordingState::Idle, key(KeyCode::Char('w'))),
            KeyAction::CycleWaveformWindow
        );
    }

    #[test]
    fn idle_p_does_nothing() {
        // Pause only meaningful while recording.
        assert_eq!(
            decide_with_state(RecordingState::Idle, key(KeyCode::Char('p'))),
            KeyAction::None
        );
    }

    #[test]
    fn idle_unmapped_keys_yield_none() {
        for code in [KeyCode::Char('x'), KeyCode::Tab, KeyCode::F(1), KeyCode::Up] {
            assert_eq!(
                decide_with_state(RecordingState::Idle, key(code)),
                KeyAction::None,
                "unexpected for {code:?}"
            );
        }
    }

    #[test]
    fn recording_r_or_esc_opens_stop_confirm() {
        for code in [KeyCode::Char('r'), KeyCode::Char('R'), KeyCode::Esc] {
            assert_eq!(
                decide_with_state(RecordingState::Recording, key(code)),
                KeyAction::OpenConfirmStop,
                "unexpected for {code:?}"
            );
        }
    }

    #[test]
    fn recording_space_drops_marker() {
        assert_eq!(
            decide_with_state(RecordingState::Recording, key(KeyCode::Char(' '))),
            KeyAction::DropMarker
        );
    }

    #[test]
    fn recording_t_marks_and_names_take() {
        assert_eq!(
            decide_with_state(RecordingState::Recording, key(KeyCode::Char('t'))),
            KeyAction::MarkAndOpenTakeNaming
        );
    }

    #[test]
    fn recording_n_opens_retroactive_naming() {
        assert_eq!(
            decide_with_state(RecordingState::Recording, key(KeyCode::Char('n'))),
            KeyAction::OpenRetroactiveTakeNaming
        );
    }

    #[test]
    fn recording_p_toggles_pause() {
        assert_eq!(
            decide_with_state(RecordingState::Recording, key(KeyCode::Char('p'))),
            KeyAction::TogglePause
        );
    }

    #[test]
    fn recording_backspace_deletes_last_marker() {
        assert_eq!(
            decide_with_state(RecordingState::Recording, key(KeyCode::Backspace)),
            KeyAction::DeleteLastMarker
        );
    }

    #[test]
    fn recording_q_does_nothing() {
        // While recording, q is intentionally not bound — user must stop first.
        assert_eq!(
            decide_with_state(RecordingState::Recording, key(KeyCode::Char('q'))),
            KeyAction::None
        );
    }

    #[test]
    fn recording_settings_and_templates_unbound() {
        // No mid-recording template/settings access.
        assert_eq!(
            decide_with_state(RecordingState::Recording, key(KeyCode::Char(','))),
            KeyAction::None
        );
        assert_eq!(
            decide_with_state(RecordingState::Recording, ctrl(KeyCode::Char('s'))),
            KeyAction::None
        );
        assert_eq!(
            decide_with_state(RecordingState::Recording, ctrl(KeyCode::Char('l'))),
            KeyAction::None
        );
    }

    #[test]
    fn recording_w_still_cycles_waveform() {
        assert_eq!(
            decide_with_state(RecordingState::Recording, key(KeyCode::Char('w'))),
            KeyAction::CycleWaveformWindow
        );
    }

    #[test]
    fn recording_question_still_opens_help() {
        assert_eq!(
            decide_with_state(RecordingState::Recording, key(KeyCode::Char('?'))),
            KeyAction::OpenHelp
        );
    }

    #[test]
    fn paused_p_toggles_pause() {
        assert_eq!(
            decide_with_state(RecordingState::Paused, key(KeyCode::Char('p'))),
            KeyAction::TogglePause
        );
    }

    #[test]
    fn paused_space_does_nothing() {
        // Can't drop a marker on a frozen timeline.
        assert_eq!(
            decide_with_state(RecordingState::Paused, key(KeyCode::Char(' '))),
            KeyAction::None
        );
    }

    #[test]
    fn paused_t_does_nothing() {
        // T drops a marker, same disable rationale as Space.
        assert_eq!(
            decide_with_state(RecordingState::Paused, key(KeyCode::Char('t'))),
            KeyAction::None
        );
    }

    #[test]
    fn paused_n_still_opens_retroactive_naming() {
        // Naming an existing take is fine while paused.
        assert_eq!(
            decide_with_state(RecordingState::Paused, key(KeyCode::Char('n'))),
            KeyAction::OpenRetroactiveTakeNaming
        );
    }

    #[test]
    fn paused_r_or_esc_still_opens_stop_confirm() {
        for code in [KeyCode::Char('r'), KeyCode::Char('R'), KeyCode::Esc] {
            assert_eq!(
                decide_with_state(RecordingState::Paused, key(code)),
                KeyAction::OpenConfirmStop,
                "unexpected for {code:?}"
            );
        }
    }

    #[test]
    fn paused_backspace_still_deletes_last_marker() {
        assert_eq!(
            decide_with_state(RecordingState::Paused, key(KeyCode::Backspace)),
            KeyAction::DeleteLastMarker
        );
    }

    #[test]
    fn paused_w_still_cycles_waveform() {
        assert_eq!(
            decide_with_state(RecordingState::Paused, key(KeyCode::Char('w'))),
            KeyAction::CycleWaveformWindow
        );
    }
}
