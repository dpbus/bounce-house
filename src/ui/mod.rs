mod device_picker;
mod footer;
mod header;
mod input;
mod modals;
mod panels;
mod take_naming;
mod text_input;
mod view;
mod widgets;

pub enum Action {
    Stay,
    Close,
}

use std::io::{self, stdout};
use std::time::Duration;

use crossterm::{
    event::{self, Event},
    execute,
    terminal::{self, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::prelude::*;

use crate::app::App;
use crate::settings::Settings;
use crate::template::Template;
use crate::ui::view::View;

pub fn run(settings: Settings, template: Option<Template>) -> io::Result<()> {
    terminal::enable_raw_mode()?;
    execute!(stdout(), EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;

    let result = bootstrap(&mut terminal, settings, template);

    terminal::disable_raw_mode()?;
    execute!(stdout(), LeaveAlternateScreen)?;

    result
}

fn bootstrap(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    settings: Settings,
    template: Option<Template>,
) -> io::Result<()> {
    let device = match device_picker::pick(terminal) {
        Ok(d) => d,
        Err(e) if e.kind() == io::ErrorKind::Interrupted => return Ok(()),
        Err(e) => return Err(e),
    };

    let mut app = App::new(device, settings);
    #[cfg(debug_assertions)]
    crate::debug::pad_channels_from_env(&mut app);
    let mut view = View::new();
    if let Some(t) = template {
        let name = t.name.clone();
        app.load_template(&t);
        view.flash_template_load(name);
    }
    main_loop(terminal, &mut app, &mut view)
}

fn main_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    view: &mut View,
) -> io::Result<()> {
    loop {
        app.tick_display();

        terminal.draw(|frame| view.draw(frame, app))?;

        if event::poll(Duration::from_millis(16))?
            && let Event::Key(key) = event::read()?
            && matches!(view.handle_key(key, app), input::Outcome::Quit)
        {
            break;
        }
    }
    Ok(())
}
