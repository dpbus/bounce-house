mod app;
mod audio;
mod channel;
mod session;
mod timeline;
mod ui;
mod units;

fn main() {
    ui::run().expect("TUI error");
}
