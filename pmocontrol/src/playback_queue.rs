use crate::media_server::ServerId;

#[derive(Clone, Debug)]
pub struct PlaybackItem {
    pub uri: String,
    pub title: Option<String>,
    pub server_id: Option<ServerId>,
    pub object_id: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub genre: Option<String>,
    pub album_art_uri: Option<String>,
    pub date: Option<String>,
    pub track_number: Option<String>,
    pub creator: Option<String>,
}

impl PlaybackItem {
    pub fn new(uri: impl Into<String>) -> Self {
        Self {
            uri: uri.into(),
            title: None,
            server_id: None,
            object_id: None,
            artist: None,
            album: None,
            genre: None,
            album_art_uri: None,
            date: None,
            track_number: None,
            creator: None,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct PlaybackQueue {
    items: Vec<PlaybackItem>,
    current_index: Option<usize>,
}

impl PlaybackQueue {
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            current_index: None,
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
        self.current_index = None;
    }

    pub fn enqueue(&mut self, item: PlaybackItem) {
        self.items.push(item);
    }

    pub fn enqueue_many<I: IntoIterator<Item = PlaybackItem>>(&mut self, items: I) {
        for item in items {
            self.items.push(item);
        }
    }

    pub fn enqueue_front(&mut self, item: PlaybackItem) {
        let insert_at = match self.current_index {
            Some(idx) => {
                let next = idx.saturating_add(1);
                next.min(self.items.len())
            }
            None => 0,
        };
        self.items.insert(insert_at, item);
        // current_index remains unchanged; insertion happens after the cursor.
    }

    pub fn dequeue(&mut self) -> Option<PlaybackItem> {
        if self.items.is_empty() {
            return None;
        }

        match self.current_index {
            None => {
                self.current_index = Some(0);
                self.items.get(0).cloned()
            }
            Some(idx) => {
                let next_idx = idx + 1;
                if next_idx >= self.items.len() {
                    None
                } else {
                    self.current_index = Some(next_idx);
                    self.items.get(next_idx).cloned()
                }
            }
        }
    }

    pub fn peek(&self) -> Option<&PlaybackItem> {
        if let Some(idx) = self.current_index {
            self.items.get(idx)
        } else {
            self.items.first()
        }
    }

    pub fn snapshot(&self) -> Vec<PlaybackItem> {
        match self.current_index {
            None => self.items.clone(),
            Some(idx) => self.items.iter().skip(idx + 1).cloned().collect(),
        }
    }

    pub fn upcoming_len(&self) -> usize {
        match self.current_index {
            None => self.items.len(),
            Some(idx) => self.items.len().saturating_sub(idx + 1),
        }
    }

    pub fn full_snapshot(&self) -> (Vec<PlaybackItem>, Option<usize>) {
        (self.items.clone(), self.current_index)
    }
}
