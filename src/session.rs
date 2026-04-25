use std::path::PathBuf;

use chrono::{DateTime, Local};

use crate::channel::Channel;
use crate::units::ChannelIndex;

pub struct Session {
    pub channels: Vec<Channel>,
    pub raw_dir: PathBuf,
    pub started_at: DateTime<Local>,
}

impl Session {
    pub fn new(channel_count: u16, raw_dir: PathBuf) -> Self {
        let channels = (0..channel_count)
            .map(|i| Channel::new(ChannelIndex(i)))
            .collect();
        Session {
            channels,
            raw_dir,
            started_at: Local::now(),
        }
    }

    pub fn channel(&self, idx: ChannelIndex) -> Option<&Channel> {
        self.channels.get(idx.as_usize())
    }

    pub fn channel_mut(&mut self, idx: ChannelIndex) -> Option<&mut Channel> {
        self.channels.get_mut(idx.as_usize())
    }

    pub fn armed(&self) -> impl Iterator<Item = &Channel> + '_ {
        self.channels.iter().filter(|c| c.armed)
    }
}
