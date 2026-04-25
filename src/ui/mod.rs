pub mod channel_select;
pub mod device_select;
pub mod recording;
pub mod widgets;

use crossterm::{
    event::{self, Event},
    execute,
    terminal::{self, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::prelude::*;
use std::io::{self, stdout};

use crate::ui::channel_select::ChannelSelectState;
use crate::ui::device_select::DeviceSelectState;
use crate::ui::recording::RecordingState;

enum Screen {
    DeviceSelect(DeviceSelectState),
    ChannelSelect(ChannelSelectState),
    Recording(RecordingState),
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
        // Per-frame state updates (UI-side decay, etc.)
        if let Screen::Recording(state) = &mut screen {
            state.update_display();
        }

        // Draw
        terminal.draw(|frame| match &screen {
            Screen::DeviceSelect(state) => device_select::draw(frame, state),
            Screen::ChannelSelect(state) => channel_select::draw(frame, state),
            Screen::Recording(state) => recording::draw(frame, state),
        })?;

        // Handle input
        if event::poll(std::time::Duration::from_millis(16))? {
            if let Event::Key(key) = event::read()? {
                let action = match &mut screen {
                    Screen::DeviceSelect(state) => device_select::handle_input(state, key),
                    Screen::ChannelSelect(state) => channel_select::handle_input(state, key),
                    Screen::Recording(state) => recording::handle_input(state, key),
                };

                match action {
                    Action::Quit => break,
                    Action::NextScreen => {
                        screen = match screen {
                            Screen::DeviceSelect(state) => {
                                let interface = state.take_selected();
                                Screen::ChannelSelect(ChannelSelectState::new(interface))
                            }
                            Screen::ChannelSelect(state) => {
                                let channels = state.selected_channels();
                                Screen::Recording(RecordingState::new(state.interface, channels))
                            }
                            Screen::Recording(_) => break,
                        };
                    }
                    Action::None => {}
                }
            }
        }
    }

    Ok(())
}
