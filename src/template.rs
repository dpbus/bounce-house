use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::channel::Channel;

#[derive(Serialize, Deserialize)]
pub struct Template {
    pub name: String,
    pub device_name: String,
    #[serde(default)]
    pub channels: Vec<Channel>,
}

impl Template {
    pub fn load(path: &Path) -> io::Result<Self> {
        let text = fs::read_to_string(path)?;
        toml::from_str(&text).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

    pub fn save(&self, path: &Path) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let text = toml::to_string_pretty(self).map_err(io::Error::other)?;
        fs::write(path, text)
    }
}

/// Resolves a `-t` argument: paths (anything containing `/` or starting with
/// `~/`) load directly from disk; bare names look up `<templates_dir>/<name>.toml`.
pub fn resolve_arg(arg: &str, templates_dir: &Path) -> PathBuf {
    if arg.starts_with("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home).join(&arg[2..]);
        }
    }
    if arg.contains('/') {
        return PathBuf::from(arg);
    }
    path_for_name(templates_dir, arg)
}

pub fn path_for_name(templates_dir: &Path, name: &str) -> PathBuf {
    templates_dir.join(format!("{}.toml", name))
}

pub fn is_valid_name(name: &str) -> bool {
    let trimmed = name.trim();
    !trimmed.is_empty()
        && trimmed != "."
        && trimmed != ".."
        && !trimmed.contains('/')
        && !trimmed.contains('\\')
}
