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
use std::path::{Path, PathBuf};

use clap::Parser;

use crate::config::Config;
use crate::template::Template;

#[derive(Parser)]
#[command(version, about = "Multitrack capture TUI", long_about = None)]
struct Cli {
    #[arg(short = 't', long = "template")]
    template: Option<String>,
}

fn main() -> io::Result<()> {
    let cli = Cli::parse();
    let config = Config::load_or_create()?;
    let template = cli.template.and_then(|arg| load_template_from_arg(&arg, &config));
    ui::run(config, template)
}

/// Loads the template name or path from command line arg (`-t`)
fn load_template_from_arg(arg: &str, config: &Config) -> Option<Template> {
    let path = template_path_from_arg(arg, &config.templates_dir);
    match Template::load(&path) {
        Ok(t) => Some(t),
        Err(e) => {
            eprintln!("warning: failed to load template '{}': {}", arg, e);
            None
        }
    }
}

fn template_path_from_arg(arg: &str, templates_dir: &Path) -> PathBuf {
    if arg.starts_with("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home).join(&arg[2..]);
        }
    }
    if arg.contains('/') {
        return PathBuf::from(arg);
    }
    template::path_for_name(templates_dir, arg)
}
