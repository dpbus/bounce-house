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
