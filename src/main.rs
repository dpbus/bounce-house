mod capture;
mod input;
mod session;
mod take;
mod ui;

fn main() {
    ui::run().expect("TUI error");
}
