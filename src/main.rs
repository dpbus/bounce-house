mod audio_interface;
mod capture;
mod input;
mod level_monitor;
mod metering;
mod session;
mod take;
mod ui;

fn main() {
    ui::run().expect("TUI error");
}
