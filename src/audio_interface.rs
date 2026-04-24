use cpal::traits::{DeviceTrait, HostTrait};

pub struct AudioInterface {
    device: cpal::Device,
    stream_config: cpal::StreamConfig,
}

impl AudioInterface {
    pub fn new(device: cpal::Device) -> Self {
        let stream_config = device
            .default_input_config()
            .expect("No default input config")
            .into();

        AudioInterface {
            device,
            stream_config,
        }
    }

    pub fn name(&self) -> String {
        self.device.name().unwrap_or_else(|_| "Unknown".to_string())
    }

    pub fn channel_count(&self) -> usize {
        self.stream_config.channels as usize
    }

    pub fn sample_rate(&self) -> u32 {
        self.stream_config.sample_rate.0
    }

    pub fn build_input_stream<F>(&self, mut callback: F) -> cpal::Stream
    where
        F: FnMut(&[f32]) + Send + 'static,
    {
        self.device
            .build_input_stream(
                &self.stream_config,
                move |data, _| callback(data),
                |err| eprintln!("Stream error: {}", err),
                None,
            )
            .expect("Failed to build input stream")
    }

    pub fn list() -> Vec<AudioInterface> {
        let host = cpal::default_host();
        let devices = host.input_devices().expect("Failed to enumerate input devices");
        devices.map(AudioInterface::new).collect()
    }
}
