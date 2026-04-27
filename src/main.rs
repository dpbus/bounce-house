mod app;
mod audio;
mod bounce;
mod channel;
mod config;
mod recording;
mod session;
mod template;
mod timeline;
mod ui;
mod units;

fn main() {
    ui::run().expect("TUI error");
}
