#![cfg(debug_assertions)]

use crate::app::App;
use crate::channel::Channel;

const TEST_LABELS: &[&str] = &[
    "Kick",
    "Snare",
    "Hi-Hat",
    "OH-L",
    "OH-R",
    "Tom-1",
    "Tom-2",
    "Floor-Tom",
    "Bass",
    "Gtr-L",
    "Gtr-R",
    "Vox",
    "BV-1",
    "BV-2",
    "Keys-L",
    "Keys-R",
];

/// Pads the session up to the count specified by `DEBUG_CHANNELS` for
/// visual UI testing. Padded channels won't receive audio; do not
/// record while padded.
pub fn pad_channels_from_env(app: &mut App) {
    let Some(target) = std::env::var("DEBUG_CHANNELS")
        .ok()
        .and_then(|s| s.parse::<u16>().ok())
    else {
        return;
    };

    let current = app.session.channels().len() as u16;
    if target <= current {
        return;
    }
    for i in current..target {
        let armed = i % 4 == 0;
        let label = if armed {
            Some(TEST_LABELS[(i as usize / 4) % TEST_LABELS.len()].to_string())
        } else {
            None
        };
        app.session.debug_push_channel(Channel {
            index: i,
            label,
            armed,
        });
    }
    let total = target as usize;
    app.display_levels.resize(total, 0.0);
    app.peak_holds.resize(total, 0.0);
}
