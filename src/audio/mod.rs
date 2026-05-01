mod device;
mod disk_writer;
mod engine;
#[cfg(debug_assertions)]
mod fake;
mod levels;

pub use device::Device;
pub use disk_writer::{ChannelOutput, DiskWriter};
pub use engine::EngineHandle;
pub use levels::LevelObservation;

pub fn list_devices() -> Vec<Device> {
    #[allow(unused_mut)]
    let mut devices = Device::list(&cpal::default_host());
    #[cfg(debug_assertions)]
    devices.extend(fake::devices());
    devices
}
