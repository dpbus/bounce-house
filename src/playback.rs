use crate::audio::{OutputDevice, ProducerControl, TrackReader};
use crate::session::Session;

pub struct Playback {
    _reader: TrackReader,
    control: Option<ProducerControl>,
}

impl Playback {
    pub fn start(output_device: &OutputDevice, session: &Session) -> Self {
        let (producer, control) = output_device.attach_producer();
        let reader = TrackReader::start(
            producer,
            output_device.channel_count(),
            session.recording_track_paths(),
        );
        Playback {
            _reader: reader,
            control: Some(control),
        }
    }
}

impl Drop for Playback {
    fn drop(&mut self) {
        if let Some(control) = self.control.take() {
            control.detach();
        }
    }
}
