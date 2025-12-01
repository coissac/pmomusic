use std::collections::VecDeque;

use crate::media_server::ServerId;

#[derive(Clone, Debug)]
pub struct PlaybackItem {
    pub uri: String,
    pub title: Option<String>,
    pub server_id: Option<ServerId>,
    pub object_id: Option<String>,
}

impl PlaybackItem {
    pub fn new(uri: impl Into<String>) -> Self {
        Self {
            uri: uri.into(),
            title: None,
            server_id: None,
            object_id: None,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct PlaybackQueue {
    items: VecDeque<PlaybackItem>,
}

impl PlaybackQueue {
    pub fn new() -> Self {
        Self {
            items: VecDeque::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn clear(&mut self) {
        self.items.clear();
    }

    pub fn enqueue(&mut self, item: PlaybackItem) {
        self.items.push_back(item);
    }

    pub fn enqueue_many<I: IntoIterator<Item = PlaybackItem>>(&mut self, items: I) {
        for item in items {
            self.items.push_back(item);
        }
    }

    pub fn enqueue_front(&mut self, item: PlaybackItem) {
        self.items.push_front(item);
    }

    pub fn dequeue(&mut self) -> Option<PlaybackItem> {
        self.items.pop_front()
    }

    pub fn peek(&self) -> Option<&PlaybackItem> {
        self.items.front()
    }

    pub fn snapshot(&self) -> Vec<PlaybackItem> {
        self.items.iter().cloned().collect()
    }
}
