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

pub fn path_for_name(templates_dir: &Path, name: &str) -> PathBuf {
    templates_dir.join(format!("{}.toml", name))
}

/// Loads every `*.toml` template in `templates_dir`, sorted by name.
/// Files that fail to deserialize are silently skipped.
pub fn list(templates_dir: &Path) -> io::Result<Vec<Template>> {
    let mut entries: Vec<Template> = Vec::new();
    if !templates_dir.exists() {
        return Ok(entries);
    }
    for entry in fs::read_dir(templates_dir)? {
        let path = entry?.path();
        if path.extension().and_then(|s| s.to_str()) != Some("toml") {
            continue;
        }
        if let Ok(t) = Template::load(&path) {
            entries.push(t);
        }
    }
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(entries)
}

pub fn is_valid_name(name: &str) -> bool {
    let trimmed = name.trim();
    !trimmed.is_empty()
        && trimmed != "."
        && trimmed != ".."
        && !trimmed.contains('/')
        && !trimmed.contains('\\')
}
