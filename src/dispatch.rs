use std::io;

use crate::app::{App, AppError};
use crate::playback::Playback;
use crate::template::Template;

pub enum Action {
    StartRecording,
    StopRecording,
    TogglePause,
    TogglePlayback,
    DropMarker,
    DeleteLastMarker,
    CreateTake(String),
    SaveTemplate(String),
    LoadTemplate(Template),
    ToggleArmed(u16),
    SetLabel(u16, Option<String>),
    CycleWaveformWindow,
}

#[derive(Debug)]
pub enum DispatchError {
    App,
    Io,
}

impl From<AppError> for DispatchError {
    fn from(_: AppError) -> Self {
        DispatchError::App
    }
}

impl From<io::Error> for DispatchError {
    fn from(_: io::Error) -> Self {
        DispatchError::Io
    }
}

pub fn dispatch(action: Action, app: &mut App) -> Result<(), DispatchError> {
    match action {
        Action::StartRecording => app.start_recording()?,
        Action::StopRecording => app.stop_recording(),
        Action::TogglePause => app.toggle_pause(),
        Action::TogglePlayback => toggle_playback(app),
        Action::DropMarker => app.drop_marker(),
        Action::DeleteLastMarker => app.delete_last_marker(),
        Action::CreateTake(name) => app.create_take(&name),
        Action::SaveTemplate(name) => app.save_template(&name)?,
        Action::LoadTemplate(template) => app.load_template(&template),
        Action::ToggleArmed(channel) => app.toggle_armed(channel),
        Action::SetLabel(channel, label) => app.set_label(channel, label),
        Action::CycleWaveformWindow => app.cycle_waveform_window(),
    }
    Ok(())
}

/// Fire-and-forget dispatch — drops the result. Use when the caller
/// doesn't need to know whether the action succeeded.
pub fn fire(action: Action, app: &mut App) {
    let _ = dispatch(action, app);
}

fn toggle_playback(app: &mut App) {
    if app.playing.is_some() {
        app.playing = None;
        return;
    }
    if !app.session.has_recording() {
        return;
    }
    let Some(output_device) = &app.mixer.output_device else {
        return;
    };
    app.playing = Some(Playback::start(output_device, &app.session));
}
