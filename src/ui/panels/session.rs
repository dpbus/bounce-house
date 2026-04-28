use chrono::Local;
use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

use crate::app::App;
use crate::ui::view::View;
use crate::ui::widgets::{key_hint_when, labeled, panel};

pub fn draw(frame: &mut Frame, area: Rect, app: &App, view: &View) {
    let inner = panel(
        frame,
        area,
        "Session",
        Some(template_action_hint(app, view)),
        None,
    );

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
            app.settings.projects_dir.display().to_string(),
        ),
        labeled("Bounces:  ", app.settings.bounces_dir.display().to_string()),
    ];

    frame.render_widget(Paragraph::new(lines), inner);
}

fn template_action_hint(app: &App, view: &View) -> Line<'static> {
    if let Some(name) = view.recent_template_save() {
        return flash_line("saved", name);
    }
    if let Some(name) = view.recent_template_load() {
        return flash_line("loaded", name);
    }
    let actionable = !app.is_recording();
    let mut spans = Vec::new();
    spans.extend(key_hint_when(
        actionable,
        "Ctrl+S",
        "save template  ",
        Color::Cyan,
    ));
    spans.extend(key_hint_when(
        actionable,
        "Ctrl+O",
        "load template ",
        Color::Cyan,
    ));
    Line::from(spans).right_aligned()
}

fn flash_line(verb: &str, name: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(" ✓ ", Style::default().fg(Color::Green)),
        Span::styled(
            format!("{} '{}' ", verb, name),
            Style::default().fg(Color::Green),
        ),
    ])
    .right_aligned()
}
