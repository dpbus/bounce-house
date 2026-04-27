use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub projects_dir: PathBuf,
    pub bounces_dir: PathBuf,
    pub templates_dir: PathBuf,
}

impl Default for Config {
    fn default() -> Self {
        let home = home_dir();
        Self {
            projects_dir: home.join("Music/BounceHouse/Projects"),
            bounces_dir: home.join("Music/BounceHouse/Bounces"),
            templates_dir: home.join("Music/BounceHouse/Templates"),
        }
    }
}

impl Config {
    /// Read the config from disk, writing defaults if absent. Ensures
    /// `projects_dir` and `bounces_dir` exist on the filesystem.
    pub fn load_or_create() -> io::Result<Self> {
        let path = config_path();
        let cfg = if path.exists() {
            let text = fs::read_to_string(&path)?;
            toml::from_str::<Config>(&text)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?
        } else {
            let cfg = Config::default();
            cfg.write(&path)?;
            cfg
        };
        fs::create_dir_all(&cfg.projects_dir)?;
        fs::create_dir_all(&cfg.bounces_dir)?;
        fs::create_dir_all(&cfg.templates_dir)?;
        Ok(cfg)
    }

    fn write(&self, path: &Path) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let text = toml::to_string_pretty(self)
            .map_err(|e| io::Error::other(e))?;
        fs::write(path, text)
    }
}

fn config_path() -> PathBuf {
    config_dir().join("config.toml")
}

/// `$XDG_CONFIG_HOME/bounce-house` if set, else `~/.config/bounce-house`.
fn config_dir() -> PathBuf {
    if let Some(xdg) = env::var_os("XDG_CONFIG_HOME") {
        return PathBuf::from(xdg).join("bounce-house");
    }
    home_dir().join(".config").join("bounce-house")
}

fn home_dir() -> PathBuf {
    PathBuf::from(env::var_os("HOME").expect("HOME not set"))
}
