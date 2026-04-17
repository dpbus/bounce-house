pub mod channel_select;
pub mod device_select;

use crossterm::{
    event::{self, Event},
    execute,
    terminal::{self, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::prelude::*;
use std::io::{self, stdout};

use crate::ui::channel_select::ChannelSelectState;
use crate::ui::device_select::DeviceSelectState;

enum Screen {
    DeviceSelect(DeviceSelectState),
    ChannelSelect(ChannelSelectState),
}

pub enum Action {
    None,
    Quit,
    NextScreen,
}

pub fn run() -> io::Result<()> {
    // Set up terminal
    terminal::enable_raw_mode()?;
    execute!(stdout(), EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;

    let screen = Screen::DeviceSelect(DeviceSelectState::new());

    let result = run_loop(&mut terminal, screen);

    // Restore terminal — always runs, even if the loop errored
    terminal::disable_raw_mode()?;
    execute!(stdout(), LeaveAlternateScreen)?;

    result
}

fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    mut screen: Screen,
) -> io::Result<()> {
    loop {
        // Draw
        terminal.draw(|frame| match &screen {
            Screen::DeviceSelect(state) => device_select::draw(frame, state),
            Screen::ChannelSelect(state) => channel_select::draw(frame, state),
        })?;

        // Handle input
        if event::poll(std::time::Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                let action = match &mut screen {
                    Screen::DeviceSelect(state) => device_select::handle_input(state, key),
                    Screen::ChannelSelect(state) => channel_select::handle_input(state, key),
                };

                match action {
                    Action::Quit => break,
                    Action::NextScreen => {
                        screen = match screen {
                            Screen::DeviceSelect(state) => {
                                let device = state.selected_device();
                                Screen::ChannelSelect(ChannelSelectState::new(device))
                            }
                            Screen::ChannelSelect(_state) => {
                                // TODO: transition to recording
                                break;
                            }
                        };
                    }
                    Action::None => {}
                }
            }
        }
    }

    Ok(())
}
