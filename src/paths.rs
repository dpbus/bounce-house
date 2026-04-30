use std::path::{Path, PathBuf};

/// Maps `s` to filename-safe characters: alphanumerics, `-`, and `_`
/// pass through; everything else becomes `_`. Caller handles empty
/// input.
pub fn filename_safe(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Expands a leading `~/` to the user's home directory; returns the
/// path verbatim otherwise.
pub fn expand_home_dir(s: &str) -> PathBuf {
    if let Some(rest) = s.strip_prefix("~/")
        && let Some(home) = std::env::var_os("HOME")
    {
        return PathBuf::from(home).join(rest);
    }
    PathBuf::from(s)
}

/// Picks an unused MP3 path in `dir`. Sanitizes `name`, prepends
/// `prefix` if any, then appends `-2`, `-3`, ... until the path
/// doesn't exist on disk.
pub fn unique_mp3_path(dir: &Path, prefix: Option<&str>, name: &str) -> PathBuf {
    let trimmed = name.trim();
    let safe = if trimmed.is_empty() {
        "take".to_string()
    } else {
        filename_safe(trimmed)
    };
    let stem = match prefix {
        Some(p) if !p.is_empty() => format!("{}_{}", p, safe),
        _ => safe,
    };
    let base = dir.join(format!("{}.mp3", stem));
    if !base.exists() {
        return base;
    }
    for n in 2.. {
        let candidate = dir.join(format!("{}-{}.mp3", stem, n));
        if !candidate.exists() {
            return candidate;
        }
    }
    unreachable!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn filename_safe_passes_through_alphanumerics_dash_underscore() {
        assert_eq!(filename_safe("verse_1-take2"), "verse_1-take2");
        assert_eq!(filename_safe("ABCxyz123"), "ABCxyz123");
    }

    #[test]
    fn filename_safe_replaces_path_separators() {
        assert_eq!(filename_safe("a/b\\c"), "a_b_c");
    }

    #[test]
    fn filename_safe_replaces_spaces_and_punctuation() {
        assert_eq!(filename_safe("hello world!"), "hello_world_");
        assert_eq!(filename_safe("a.b:c?"), "a_b_c_");
    }

    #[test]
    fn filename_safe_preserves_unicode_letters() {
        assert_eq!(filename_safe("café"), "café");
        assert_eq!(filename_safe("日本語"), "日本語");
    }

    #[test]
    fn filename_safe_returns_empty_for_empty_input() {
        assert_eq!(filename_safe(""), "");
    }

    #[test]
    fn unique_mp3_path_uses_take_name_when_no_prefix() {
        let dir = tempdir().unwrap();
        let path = unique_mp3_path(dir.path(), None, "verse");
        assert_eq!(path, dir.path().join("verse.mp3"));
    }

    #[test]
    fn unique_mp3_path_prepends_prefix_when_provided() {
        let dir = tempdir().unwrap();
        let path = unique_mp3_path(dir.path(), Some("session1"), "verse");
        assert_eq!(path, dir.path().join("session1_verse.mp3"));
    }

    #[test]
    fn unique_mp3_path_appends_counter_on_collision() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("verse.mp3"), b"existing").unwrap();
        let path = unique_mp3_path(dir.path(), None, "verse");
        assert_eq!(path, dir.path().join("verse-2.mp3"));
    }

    #[test]
    fn unique_mp3_path_finds_next_available_counter() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("verse.mp3"), b"x").unwrap();
        std::fs::write(dir.path().join("verse-2.mp3"), b"x").unwrap();
        let path = unique_mp3_path(dir.path(), None, "verse");
        assert_eq!(path, dir.path().join("verse-3.mp3"));
    }

    #[test]
    fn unique_mp3_path_falls_back_to_take_when_name_blank() {
        let dir = tempdir().unwrap();
        let path = unique_mp3_path(dir.path(), None, "   ");
        assert_eq!(path, dir.path().join("take.mp3"));
    }

    #[test]
    fn unique_mp3_path_sanitizes_unsafe_characters() {
        let dir = tempdir().unwrap();
        let path = unique_mp3_path(dir.path(), None, "take/with/slashes");
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        assert!(!name.contains('/'), "sanitized name leaked '/': {name}");
    }
}
