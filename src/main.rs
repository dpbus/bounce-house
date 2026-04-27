mod app;
mod audio;
mod bounce;
mod channel;
mod config;
mod recording;
mod session;
mod template;
mod timeline;
mod ui;
mod units;

use std::io;

use clap::Parser;

use crate::config::Config;
use crate::template::Template;

#[derive(Parser)]
#[command(version, about = "Multitrack capture TUI", long_about = None)]
struct Cli {
    /// Project template to load on startup (name in templates_dir, or a path).
    #[arg(short = 't', long = "template")]
    template: Option<String>,
}

fn main() -> io::Result<()> {
    let cli = Cli::parse();
    let config = Config::load_or_create()?;
    let template = cli.template.and_then(|arg| load_template_from_cli(&arg, &config));
    ui::run(config, template)
}

/// Resolves the user's `-t` argument and loads the template from disk,
/// returning the display name (file stem) alongside the data. Errors
/// print a warning and return None — the app boots without the template.
fn load_template_from_cli(arg: &str, config: &Config) -> Option<(String, Template)> {
    let path = template::resolve_arg(arg, &config.templates_dir);
    match Template::load(&path) {
        Ok(t) => {
            let name = path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| arg.to_string());
            Some((name, t))
        }
        Err(e) => {
            eprintln!("warning: failed to load template '{}': {}", arg, e);
            None
        }
    }
}
