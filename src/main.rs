mod app;
mod audio;
mod channel;
mod session;
mod ui;
mod units;

fn main() {
    ui::run().expect("TUI error");
}
