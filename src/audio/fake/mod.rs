mod signal;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, StreamTrait};
use cpal::{
    Data, DeviceDescription, DeviceDescriptionBuilder, InputCallbackInfo, InputStreamTimestamp,
    SampleFormat, StreamInstant, SupportedBufferSize, SupportedStreamConfig,
    SupportedStreamConfigRange,
};

use crate::audio::DeviceInfo;
use crate::units::SampleRate;
use signal::Signal;

const SAMPLE_RATE: u32 = 48_000;
const BUFFER_FRAMES: usize = 512;
const PRESET_CHANNEL_COUNTS: &[u16] = &[2, 16, 65];

/// Synthetic input devices added to the picker in debug builds. Useful
/// for previewing the UI with N channels on a dev machine without an
/// audio interface. Output is unsupported (None) — fake devices are
/// input-only.
pub fn devices() -> Vec<DeviceInfo> {
    PRESET_CHANNEL_COUNTS
        .iter()
        .map(|&n| {
            let fake = FakeDevice { channel_count: n };
            let custom = cpal::platform::CustomDevice::from_device(fake);
            let cpal_device = cpal::Device::from(custom);
            let label = format!("Fake ({} ch)", n);
            DeviceInfo::from_parts(cpal_device, label, n, SampleRate(SAMPLE_RATE), None)
        })
        .collect()
}

#[derive(Clone)]
struct FakeDevice {
    channel_count: u16,
}

impl FakeDevice {
    fn label(&self) -> String {
        format!("Fake ({} ch)", self.channel_count)
    }
}

struct FakeStream {
    controls: Arc<StreamControls>,
    handle: Option<JoinHandle<()>>,
}

struct StreamControls {
    exit: AtomicBool,
    pause: AtomicBool,
}

impl DeviceTrait for FakeDevice {
    type SupportedInputConfigs = std::iter::Once<SupportedStreamConfigRange>;
    type SupportedOutputConfigs = std::iter::Empty<SupportedStreamConfigRange>;
    type Stream = FakeStream;

    fn name(&self) -> Result<String, cpal::DeviceNameError> {
        Ok(self.label())
    }

    fn description(&self) -> Result<DeviceDescription, cpal::DeviceNameError> {
        Ok(DeviceDescriptionBuilder::new(self.label()).build())
    }

    fn id(&self) -> Result<cpal::DeviceId, cpal::DeviceIdError> {
        Err(cpal::DeviceIdError::UnsupportedPlatform)
    }

    fn supported_input_configs(
        &self,
    ) -> Result<Self::SupportedInputConfigs, cpal::SupportedStreamConfigsError> {
        Ok(std::iter::once(SupportedStreamConfigRange::new(
            self.channel_count,
            SAMPLE_RATE,
            SAMPLE_RATE,
            SupportedBufferSize::Unknown,
            SampleFormat::F32,
        )))
    }

    fn supported_output_configs(
        &self,
    ) -> Result<Self::SupportedOutputConfigs, cpal::SupportedStreamConfigsError> {
        Ok(std::iter::empty())
    }

    fn default_input_config(
        &self,
    ) -> Result<SupportedStreamConfig, cpal::DefaultStreamConfigError> {
        Ok(SupportedStreamConfig::new(
            self.channel_count,
            SAMPLE_RATE,
            SupportedBufferSize::Unknown,
            SampleFormat::F32,
        ))
    }

    fn default_output_config(
        &self,
    ) -> Result<SupportedStreamConfig, cpal::DefaultStreamConfigError> {
        Err(cpal::DefaultStreamConfigError::StreamTypeNotSupported)
    }

    fn build_input_stream_raw<D, E>(
        &self,
        config: &cpal::StreamConfig,
        _sample_format: SampleFormat,
        data_callback: D,
        _error_callback: E,
        _timeout: Option<Duration>,
    ) -> Result<Self::Stream, cpal::BuildStreamError>
    where
        D: FnMut(&Data, &InputCallbackInfo) + Send + 'static,
        E: FnMut(cpal::StreamError) + Send + 'static,
    {
        let controls = Arc::new(StreamControls {
            exit: AtomicBool::new(false),
            pause: AtomicBool::new(true),
        });
        let handle = run_pump(
            controls.clone(),
            config.channels as usize,
            config.sample_rate,
            data_callback,
        );
        Ok(FakeStream {
            controls,
            handle: Some(handle),
        })
    }

    fn build_output_stream_raw<D, E>(
        &self,
        _: &cpal::StreamConfig,
        _: SampleFormat,
        _: D,
        _: E,
        _: Option<Duration>,
    ) -> Result<Self::Stream, cpal::BuildStreamError>
    where
        D: FnMut(&mut Data, &cpal::OutputCallbackInfo) + Send + 'static,
        E: FnMut(cpal::StreamError) + Send + 'static,
    {
        Err(cpal::BuildStreamError::StreamConfigNotSupported)
    }
}

impl StreamTrait for FakeStream {
    fn play(&self) -> Result<(), cpal::PlayStreamError> {
        self.controls.pause.store(false, Ordering::Relaxed);
        Ok(())
    }

    fn pause(&self) -> Result<(), cpal::PauseStreamError> {
        self.controls.pause.store(true, Ordering::Relaxed);
        Ok(())
    }
}

impl Drop for FakeStream {
    fn drop(&mut self) {
        self.controls.exit.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

/// Spawns the audio-thread loop: sleep one buffer's worth, fill with
/// generated samples, hand off to the user's data callback as a
/// `cpal::Data` view. Exits when `controls.exit` is set; skips the
/// callback while `controls.pause` is set.
fn run_pump<D>(
    controls: Arc<StreamControls>,
    total_channels: usize,
    sample_rate: u32,
    mut data_callback: D,
) -> JoinHandle<()>
where
    D: FnMut(&Data, &InputCallbackInfo) + Send + 'static,
{
    thread::spawn(move || {
        let start = Instant::now();
        let mut buffer = vec![0.0f32; BUFFER_FRAMES * total_channels];
        let mut signal = Signal::new(total_channels, sample_rate);
        let tick = Duration::from_secs_f32(BUFFER_FRAMES as f32 / sample_rate as f32);

        while !controls.exit.load(Ordering::Relaxed) {
            thread::sleep(tick);
            if controls.pause.load(Ordering::Relaxed) {
                continue;
            }

            signal.fill(&mut buffer);

            let data = unsafe {
                Data::from_parts(buffer.as_mut_ptr().cast(), buffer.len(), SampleFormat::F32)
            };
            let elapsed = Instant::now().duration_since(start);
            let stream_instant =
                StreamInstant::new(elapsed.as_secs() as i64, elapsed.subsec_nanos());
            let timestamp = InputStreamTimestamp {
                callback: stream_instant,
                capture: stream_instant,
            };
            data_callback(&data, &InputCallbackInfo::new(timestamp));
        }
    })
}
