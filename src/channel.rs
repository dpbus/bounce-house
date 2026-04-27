use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct Channel {
    pub index: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default)]
    pub armed: bool,
}

impl Channel {
    pub fn new(index: u16) -> Self {
        Channel {
            index,
            label: None,
            armed: false,
        }
    }
}
