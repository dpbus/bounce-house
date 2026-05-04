use crate::audio::{OutputDevice, TrackReader};
use crate::session::Session;

pub struct Playback {
    _reader: TrackReader,
}

impl Playback {
    pub fn start(output_device: &OutputDevice, session: &Session) -> Self {
        let producer = output_device.attach_producer();
        let reader = TrackReader::start(
            producer,
            output_device.channel_count(),
            session.recording_track_paths(),
        );
        Playback { _reader: reader }
    }
}
