mod input;
mod take;
mod session;
mod capture;

use std::path::Path;
use std::sync::atomic::Ordering;
use cpal::traits::HostTrait;

fn main() {
    let host = cpal::default_host();
    let device = host.default_input_device().expect("no input device available");

    let channels: Vec<u8> = vec![0];
    let (stream, running, handle) = capture::start(&device, &channels, Path::new("output.wav"));

    println!("Press Enter to stop...");
    let mut input = String::new();
    std::io::stdin().read_line(&mut input).unwrap();

    running.store(false, Ordering::Relaxed);
    drop(stream);
    handle.join().expect("Writer thread panicked");
    println!("Done!");
}
