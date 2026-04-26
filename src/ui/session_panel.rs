use chrono::Local;
use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

use crate::app::App;
use crate::ui::widgets::{labeled, panel};

pub fn draw(frame: &mut Frame, area: Rect, app: &App) {
    let inner = panel(frame, area, "Session", None);

    let duration = Local::now() - app.session.started_at;
    let secs = duration.num_seconds().max(0);
    let duration_text = format!(
        "{:02}:{:02}:{:02}",
        secs / 3600,
        (secs % 3600) / 60,
        secs % 60,
    );

    let lines = vec![
        labeled("Device:   ", app.engine.name().to_string()),
        labeled(
            "Started:  ",
            app.session.started_at.format("%H:%M:%S").to_string(),
        ),
        labeled("Duration: ", duration_text),
        labeled(
            "Channels: ",
            format!(
                "{} armed / {}",
                app.session.armed().count(),
                app.engine.channel_count(),
            ),
        ),
        labeled("Output:   ", app.session.raw_dir.display().to_string()),
    ];
    frame.render_widget(Paragraph::new(lines), inner);
}
