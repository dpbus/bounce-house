use crate::units::ChannelIndex;

pub struct Channel {
    pub index: ChannelIndex,
    pub label: Option<String>,
    pub armed: bool,
}

impl Channel {
    pub fn new(index: ChannelIndex) -> Self {
        Channel {
            index,
            label: None,
            armed: false,
        }
    }
}
