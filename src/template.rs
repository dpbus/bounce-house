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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn make(name: &str) -> Template {
        let mut ch0 = Channel::new(0);
        ch0.label = Some("Vocals".into());
        ch0.armed = true;
        Template {
            name: name.into(),
            device_name: "Mock Device".into(),
            channels: vec![ch0, Channel::new(1)],
        }
    }

    #[test]
    fn save_then_load_roundtrips() {
        let dir = tempdir().unwrap();
        let path = path_for_name(dir.path(), "drums");
        let original = make("drums");
        original.save(&path).expect("save");

        let loaded = Template::load(&path).expect("load");
        assert_eq!(loaded.name, "drums");
        assert_eq!(loaded.device_name, "Mock Device");
        assert_eq!(loaded.channels.len(), 2);
        assert_eq!(loaded.channels[0].label.as_deref(), Some("Vocals"));
        assert!(loaded.channels[0].armed);
    }

    #[test]
    fn save_creates_parent_directory() {
        let dir = tempdir().unwrap();
        let nested = dir.path().join("a").join("b").join("c");
        let path = path_for_name(&nested, "x");
        make("x").save(&path).expect("save");
        assert!(path.exists());
    }

    #[test]
    fn path_for_name_appends_toml_extension() {
        let dir = std::path::Path::new("/templates");
        assert_eq!(path_for_name(dir, "foo"), dir.join("foo.toml"));
    }

    #[test]
    fn list_returns_empty_when_dir_missing() {
        let dir = tempdir().unwrap();
        let templates = list(&dir.path().join("nope")).expect("list");
        assert!(templates.is_empty());
    }

    #[test]
    fn list_returns_only_toml_files_sorted_by_name() {
        let dir = tempdir().unwrap();
        make("zebra").save(&path_for_name(dir.path(), "zebra")).unwrap();
        make("apple").save(&path_for_name(dir.path(), "apple")).unwrap();
        make("mango").save(&path_for_name(dir.path(), "mango")).unwrap();
        // Non-toml shouldn't appear:
        std::fs::write(dir.path().join("readme.txt"), "ignore me").unwrap();

        let names: Vec<String> = list(dir.path()).unwrap().into_iter().map(|t| t.name).collect();
        assert_eq!(names, vec!["apple", "mango", "zebra"]);
    }

    #[test]
    fn list_skips_corrupt_toml_files() {
        let dir = tempdir().unwrap();
        make("good").save(&path_for_name(dir.path(), "good")).unwrap();
        std::fs::write(dir.path().join("broken.toml"), "this is not toml = = =").unwrap();

        let names: Vec<String> = list(dir.path()).unwrap().into_iter().map(|t| t.name).collect();
        assert_eq!(names, vec!["good"]);
    }

    #[test]
    fn is_valid_name_accepts_normal_strings() {
        assert!(is_valid_name("drums"));
        assert!(is_valid_name("vocals_take_2"));
        assert!(is_valid_name("a"));
    }

    #[test]
    fn is_valid_name_trims_before_checking() {
        assert!(is_valid_name("  drums  "));
        assert!(!is_valid_name("   "));
    }

    #[test]
    fn is_valid_name_rejects_empty_and_dot_paths() {
        assert!(!is_valid_name(""));
        assert!(!is_valid_name("."));
        assert!(!is_valid_name(".."));
    }

    #[test]
    fn is_valid_name_rejects_path_separators() {
        assert!(!is_valid_name("a/b"));
        assert!(!is_valid_name("a\\b"));
        assert!(!is_valid_name("../escape"));
    }
}
