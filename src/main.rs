mod app;
mod audio;
mod bounce;
mod capture;
mod channel;
mod dispatch;
mod level_history;
mod meters;
mod mixer;
mod paths;
mod session;
mod settings;
mod template;
mod timeline;
mod track;
mod transport;
mod ui;
mod units;

use std::io;
use std::path::{Path, PathBuf};

use clap::Parser;

use crate::settings::Settings;
use crate::template::Template;

#[derive(Parser)]
#[command(version, about = "Multitrack capture TUI", long_about = None)]
struct Cli {
    #[arg(short = 't', long = "template")]
    template: Option<String>,
}

fn main() -> io::Result<()> {
    let cli = Cli::parse();
    let settings = Settings::load_or_create()?;
    let template = cli
        .template
        .and_then(|arg| load_template_from_arg(&arg, &settings));
    ui::run(settings, template)
}

/// Loads the template name or path from command line arg (`-t`)
fn load_template_from_arg(arg: &str, settings: &Settings) -> Option<Template> {
    let path = template_path_from_arg(arg, &settings.templates_dir);
    match Template::load(&path) {
        Ok(t) => Some(t),
        Err(e) => {
            eprintln!("warning: failed to load template '{}': {}", arg, e);
            None
        }
    }
}

fn template_path_from_arg(arg: &str, templates_dir: &Path) -> PathBuf {
    if arg.starts_with("~/") || arg.contains('/') {
        return crate::paths::expand_home_dir(arg);
    }
    template::path_for_name(templates_dir, arg)
}
