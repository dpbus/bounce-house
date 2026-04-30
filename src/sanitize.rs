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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passes_through_alphanumerics_dash_underscore() {
        assert_eq!(filename_safe("verse_1-take2"), "verse_1-take2");
        assert_eq!(filename_safe("ABCxyz123"), "ABCxyz123");
    }

    #[test]
    fn replaces_path_separators() {
        assert_eq!(filename_safe("a/b\\c"), "a_b_c");
    }

    #[test]
    fn replaces_spaces_and_punctuation() {
        assert_eq!(filename_safe("hello world!"), "hello_world_");
        assert_eq!(filename_safe("a.b:c?"), "a_b_c_");
    }

    #[test]
    fn preserves_unicode_letters() {
        // is_alphanumeric is Unicode-aware in Rust.
        assert_eq!(filename_safe("café"), "café");
        assert_eq!(filename_safe("日本語"), "日本語");
    }

    #[test]
    fn returns_empty_for_empty_input() {
        assert_eq!(filename_safe(""), "");
    }
}
