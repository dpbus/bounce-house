mod channel_picker;
mod device_picker;
mod footer;
mod input;
mod panels;
mod template_save;
mod view;
mod widgets;

use std::io::{self, stdout};
use std::time::Duration;

use crossterm::{
    event::{self, Event},
    execute,
    terminal::{self, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::prelude::*;

use crate::app::{App, AppState};
use crate::config::Config;
use crate::template::Template;

pub fn run(config: Config, template: Option<Template>) -> io::Result<()> {
    terminal::enable_raw_mode()?;
    execute!(stdout(), EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;

    let result = bootstrap(&mut terminal, config, template);

    terminal::disable_raw_mode()?;
    execute!(stdout(), LeaveAlternateScreen)?;

    result
}

fn bootstrap(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    config: Config,
    template: Option<Template>,
) -> io::Result<()> {
    let device = match device_picker::pick(terminal) {
        Ok(d) => d,
        Err(e) if e.kind() == io::ErrorKind::Interrupted => return Ok(()),
        Err(e) => return Err(e),
    };

    let mut app = App::new(device, config);
    if let Some(t) = template {
        app.load_template(&t);
    }
    main_loop(terminal, &mut app)
}

fn main_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
) -> io::Result<()> {
    loop {
        app.tick_display();

        terminal.draw(|frame| {
            view::draw(frame, app);
            if matches!(app.state, AppState::PickingChannel { .. }) {
                channel_picker::draw(frame, app);
            }
            if matches!(app.state, AppState::SavingTemplate { .. }) {
                template_save::draw(frame, app);
            }
        })?;

        if event::poll(Duration::from_millis(16))?
            && let Event::Key(key) = event::read()?
            && matches!(input::handle(app, key), input::Outcome::Quit)
        {
            break;
        }
    }
    Ok(())
}
