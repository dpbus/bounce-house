use chrono::Local;
use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

use crate::app::App;
use crate::ui::widgets::{labeled, panel};

pub fn draw(frame: &mut Frame, area: Rect, app: &App) {
    let inner = panel(frame, area, "Session", Some(save_template_action_hint(app)), None);

    let duration = Local::now() - app.session.started_at;
    let secs = duration.num_seconds().max(0);
    let duration_text = format!(
        "{:02}:{:02}:{:02}",
        secs / 3600,
        (secs % 3600) / 60,
        secs % 60,
    );

    let lines = vec![
        labeled("Device:   ", app.engine.device_name().to_string()),
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
        labeled(
            "Projects: ",
            app.config.projects_dir.display().to_string(),
        ),
        labeled(
            "Bounces:  ",
            app.config.bounces_dir.display().to_string(),
        ),
    ];

    frame.render_widget(Paragraph::new(lines), inner);
}

fn save_template_action_hint(app: &App) -> Line<'static> {
    if let Some(save) = app.recent_template_save() {
        return Line::from(vec![
            Span::styled(" ✓ ", Style::default().fg(Color::Green)),
            Span::styled(
                format!("saved '{}' ", save.name),
                Style::default().fg(Color::Green),
            ),
        ])
        .right_aligned();
    }
    Line::from(vec![
        Span::styled("[Ctrl+S]", Style::default().fg(Color::Cyan)),
        Span::raw(" save template "),
    ])
    .right_aligned()
}
