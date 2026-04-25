use cpal::traits::{DeviceTrait, HostTrait};

use crate::units::SampleRate;

pub struct Device {
    cpal_device: cpal::Device,
    cpal_config: cpal::StreamConfig,
    name: String,
    channel_count: u16,
    sample_rate: SampleRate,
}

impl Device {
    pub fn list() -> Vec<Device> {
        let host = cpal::default_host();
        let devices = host
            .input_devices()
            .expect("Failed to enumerate input devices");
        devices.map(Device::from_cpal).collect()
    }

    fn from_cpal(cpal_device: cpal::Device) -> Self {
        let cpal_config: cpal::StreamConfig = cpal_device
            .default_input_config()
            .expect("No default input config")
            .into();
        let name = cpal_device.name().unwrap_or_else(|_| "Unknown".to_string());
        let channel_count = cpal_config.channels;
        let sample_rate = SampleRate(cpal_config.sample_rate.0);

        Device {
            cpal_device,
            cpal_config,
            name,
            channel_count,
            sample_rate,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn channel_count(&self) -> u16 {
        self.channel_count
    }

    pub fn sample_rate(&self) -> SampleRate {
        self.sample_rate
    }

    pub(super) fn build_input_stream<F>(&self, mut callback: F) -> cpal::Stream
    where
        F: FnMut(&[f32]) + Send + 'static,
    {
        self.cpal_device
            .build_input_stream(
                &self.cpal_config,
                move |data, _| callback(data),
                |err| eprintln!("Stream error: {}", err),
                None,
            )
            .expect("Failed to build input stream")
    }
}
