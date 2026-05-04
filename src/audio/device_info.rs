use cpal::traits::{DeviceTrait, HostTrait};

use crate::units::SampleRate;

/// Lightweight descriptor for the picker. Carries enough metadata to
/// display "name — N in / M out" without starting any audio streams.
/// Pass to `InputDevice::start(&info)` (or future
/// `OutputDevice::start(&info)`) to construct a running device.
pub struct DeviceInfo {
    pub(super) cpal_device: cpal::Device,
    pub(super) name: String,
    input_channel_count: u16,
    input_sample_rate: SampleRate,
    /// `None` for input-only devices (e.g., debug fake devices).
    output_channel_count: Option<u16>,
}

impl DeviceInfo {
    pub fn list() -> Vec<DeviceInfo> {
        #[allow(unused_mut)]
        let mut devices: Vec<DeviceInfo> = cpal::default_host()
            .input_devices()
            .map(|iter| iter.filter_map(Self::from_cpal_device).collect())
            .unwrap_or_default();
        #[cfg(debug_assertions)]
        devices.extend(crate::audio::fake::devices());
        devices
    }

    pub(super) fn from_cpal_device(cpal_device: cpal::Device) -> Option<Self> {
        let input_config: cpal::StreamConfig = cpal_device.default_input_config().ok()?.into();
        let name = cpal_device
            .description()
            .map(|d| d.name().to_string())
            .unwrap_or_else(|_| "Unknown".to_string());
        let input_channel_count = input_config.channels;
        let input_sample_rate = SampleRate(input_config.sample_rate);

        let output_channel_count = cpal_device.default_output_config().ok().map(|config| {
            let cfg: cpal::StreamConfig = config.into();
            cfg.channels
        });

        Some(DeviceInfo {
            cpal_device,
            name,
            input_channel_count,
            input_sample_rate,
            output_channel_count,
        })
    }

    pub(super) fn from_parts(
        cpal_device: cpal::Device,
        name: String,
        input_channel_count: u16,
        input_sample_rate: SampleRate,
        output_channel_count: Option<u16>,
    ) -> Self {
        DeviceInfo {
            cpal_device,
            name,
            input_channel_count,
            input_sample_rate,
            output_channel_count,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn input_channel_count(&self) -> u16 {
        self.input_channel_count
    }

    pub fn input_sample_rate(&self) -> SampleRate {
        self.input_sample_rate
    }

    pub fn output_channel_count(&self) -> Option<u16> {
        self.output_channel_count
    }
}
