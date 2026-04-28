use chrono::{DateTime, Local};

use crate::channel::Channel;

pub struct Session {
    pub channels: Vec<Channel>,
    pub started_at: DateTime<Local>,
}

impl Session {
    pub fn new(channel_count: u16) -> Self {
        let channels = (0..channel_count).map(Channel::new).collect();
        Session {
            channels,
            started_at: Local::now(),
        }
    }

    pub fn channel_mut(&mut self, index: u16) -> Option<&mut Channel> {
        self.channels.get_mut(index as usize)
    }

    pub fn armed(&self) -> impl Iterator<Item = &Channel> + '_ {
        self.channels.iter().filter(|c| c.armed)
    }
}
