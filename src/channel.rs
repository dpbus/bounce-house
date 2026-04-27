pub struct Channel {
    pub index: u16,
    pub label: Option<String>,
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
