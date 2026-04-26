mod app;
mod audio;
mod channel;
mod recording;
mod session;
mod timeline;
mod ui;
mod units;

fn main() {
    ui::run().expect("TUI error");
}
