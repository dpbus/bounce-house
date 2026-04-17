mod input;
mod take;
mod session;

use cpal::traits::{DeviceTrait, HostTrait};

fn main() {
    let host = cpal::default_host();

    let devices = host.input_devices().expect("Failed to get input devices");

    for device in devices {
        let name = device.name().unwrap_or_else(|_| "Unknown".to_string());
        let config = device.default_input_config();

        match config {
            Ok(config) => {
                println!("{}", name);
                println!("  Channels: {}", config.channels());
                println!("  Sample Rate: {}", config.sample_rate().0);
                println!("  Sample Format: {:?}", config.sample_format());
                println!();
            }
            Err(e) => {
                println!("{}", name);
                println!("  Error getting config: {}", e);
                println!();
            }
        }
    }
}
