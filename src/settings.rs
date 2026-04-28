use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub projects_dir: PathBuf,
    pub bounces_dir: PathBuf,
    pub templates_dir: PathBuf,
}

impl Default for Settings {
    fn default() -> Self {
        let home = home_dir();
        Self {
            projects_dir: home.join("Music/BounceHouse/Projects"),
            bounces_dir: home.join("Music/BounceHouse/Bounces"),
            templates_dir: home.join("Music/BounceHouse/Templates"),
        }
    }
}

impl Settings {
    /// Read settings from disk, writing defaults if absent. Ensures
    /// every directory exists on the filesystem.
    pub fn load_or_create() -> io::Result<Self> {
        let path = settings_path();
        let settings = if path.exists() {
            let text = fs::read_to_string(&path)?;
            toml::from_str::<Settings>(&text)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?
        } else {
            let settings = Settings::default();
            settings.write(&path)?;
            settings
        };
        fs::create_dir_all(&settings.projects_dir)?;
        fs::create_dir_all(&settings.bounces_dir)?;
        fs::create_dir_all(&settings.templates_dir)?;
        Ok(settings)
    }

    /// Persists the current settings to disk at the canonical path.
    pub fn save(&self) -> io::Result<()> {
        self.write(&settings_path())
    }

    /// Replaces the three on-disk paths from raw user input. Trims,
    /// expands a leading `~/`, rejects blanks, ensures each directory
    /// exists, then writes the settings file.
    pub fn update_paths(
        &mut self,
        projects: &str,
        bounces: &str,
        templates: &str,
    ) -> io::Result<()> {
        let projects = expand_home_dir(projects.trim());
        let bounces = expand_home_dir(bounces.trim());
        let templates = expand_home_dir(templates.trim());
        if projects.as_os_str().is_empty()
            || bounces.as_os_str().is_empty()
            || templates.as_os_str().is_empty()
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "paths cannot be empty",
            ));
        }
        fs::create_dir_all(&projects)?;
        fs::create_dir_all(&bounces)?;
        fs::create_dir_all(&templates)?;
        self.projects_dir = projects;
        self.bounces_dir = bounces;
        self.templates_dir = templates;
        self.save()
    }

    fn write(&self, path: &Path) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let text = toml::to_string_pretty(self).map_err(io::Error::other)?;
        fs::write(path, text)
    }
}

/// Expands a leading `~/` to the user's home directory; returns the
/// path verbatim otherwise.
pub fn expand_home_dir(s: &str) -> PathBuf {
    if let Some(rest) = s.strip_prefix("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home).join(rest);
        }
    }
    PathBuf::from(s)
}

fn settings_path() -> PathBuf {
    settings_dir().join("config.toml")
}

/// `$XDG_CONFIG_HOME/bounce-house` if set, else `~/.config/bounce-house`.
fn settings_dir() -> PathBuf {
    if let Some(xdg) = env::var_os("XDG_CONFIG_HOME") {
        return PathBuf::from(xdg).join("bounce-house");
    }
    home_dir().join(".config").join("bounce-house")
}

fn home_dir() -> PathBuf {
    PathBuf::from(env::var_os("HOME").expect("HOME not set"))
}
