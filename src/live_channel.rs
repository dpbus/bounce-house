use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
pub struct LiveChannel {
    pub index: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default)]
    pub armed: bool,
}

impl LiveChannel {
    pub fn new(index: u16) -> Self {
        LiveChannel {
            index,
            label: None,
            armed: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_initializes_unarmed_unlabeled() {
        let ch = LiveChannel::new(7);
        assert_eq!(ch.index, 7);
        assert!(!ch.armed);
        assert!(ch.label.is_none());
    }

    #[test]
    fn serde_roundtrips_through_toml() {
        let mut ch = LiveChannel::new(3);
        ch.label = Some("Vocals".into());
        ch.armed = true;

        let toml_str = toml::to_string(&ch).expect("serialize");
        let parsed: LiveChannel = toml::from_str(&toml_str).expect("deserialize");

        assert_eq!(parsed.index, 3);
        assert_eq!(parsed.label.as_deref(), Some("Vocals"));
        assert!(parsed.armed);
    }

    #[test]
    fn serde_skips_label_when_none() {
        let ch = LiveChannel::new(0);
        let toml_str = toml::to_string(&ch).expect("serialize");
        assert!(!toml_str.contains("label"), "got: {toml_str}");
    }

    #[test]
    fn serde_handles_missing_optional_fields() {
        // Older config files may omit `armed`. Should default to false.
        let toml_str = "index = 2";
        let parsed: LiveChannel = toml::from_str(toml_str).expect("deserialize");
        assert_eq!(parsed.index, 2);
        assert!(!parsed.armed);
        assert!(parsed.label.is_none());
    }
}
