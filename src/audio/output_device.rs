use cpal::traits::{DeviceTrait, StreamTrait};

use crate::audio::DeviceInfo;

pub struct OutputDevice {
    _stream: cpal::Stream,
}

impl OutputDevice {
    /// `None` when the device doesn't expose an output config (e.g.,
    /// debug fake devices).
    pub fn start(info: &DeviceInfo) -> Option<Self> {
        let cpal_device = info.cpal_device.clone();
        let output_config: cpal::StreamConfig = cpal_device.default_output_config().ok()?.into();

        let stream = cpal_device
            .build_output_stream(
                &output_config,
                |data: &mut [f32], _| {
                    data.fill(0.0);
                },
                |err| eprintln!("Output stream error: {}", err),
                None,
            )
            .expect("Failed to build output stream");

        stream.play().expect("Failed to start output stream");

        Some(OutputDevice { _stream: stream })
    }
}
