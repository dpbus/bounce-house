mod app;
mod audio;
mod audio_interface;
mod capture;
mod channel;
mod input;
mod level_monitor;
mod metering;
mod session;
mod ui;
mod units;

fn main() {
    ui::run().expect("TUI error");
}
