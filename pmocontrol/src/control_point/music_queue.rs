use anyhow::{Result, anyhow};

use crate::control_point::openhome_queue::OpenHomeQueue;
use crate::openhome_playlist::OpenHomePlaylistSnapshot;
use crate::queue_backend::{PlaybackItem, QueueBackend, QueueSnapshot};
use crate::queue_interne::InternalQueue;

#[derive(Debug, Clone)]
pub enum MusicQueue {
    Internal(InternalQueue),
    OpenHome(OpenHomeQueue),
}

impl MusicQueue {
    pub fn new_internal() -> Self {
        MusicQueue::Internal(InternalQueue::default())
    }

    pub fn openhome_playlist_snapshot(&self) -> Result<OpenHomePlaylistSnapshot> {
        match self {
            MusicQueue::OpenHome(queue) => queue.openhome_playlist_snapshot(),
            _ => Err(anyhow!(
                "OpenHome playlist snapshot is only available for OpenHome queues"
            )),
        }
    }
}

impl Default for MusicQueue {
    fn default() -> Self {
        MusicQueue::new_internal()
    }
}

impl QueueBackend for MusicQueue {
    fn queue_snapshot(&self) -> Result<QueueSnapshot> {
        match self {
            MusicQueue::Internal(q) => q.queue_snapshot(),
            MusicQueue::OpenHome(q) => q.queue_snapshot(),
        }
    }

    fn set_index(&mut self, index: Option<usize>) -> Result<()> {
        match self {
            MusicQueue::Internal(q) => q.set_index(index),
            MusicQueue::OpenHome(q) => q.set_index(index),
        }
    }

    fn replace_queue(
        &mut self,
        items: Vec<PlaybackItem>,
        current_index: Option<usize>,
    ) -> Result<()> {
        match self {
            MusicQueue::Internal(q) => q.replace_queue(items, current_index),
            MusicQueue::OpenHome(q) => q.replace_queue(items, current_index),
        }
    }

    fn get_item(&self, index: usize) -> Result<Option<PlaybackItem>> {
        match self {
            MusicQueue::Internal(q) => q.get_item(index),
            MusicQueue::OpenHome(q) => q.get_item(index),
        }
    }

    fn replace_item(&mut self, index: usize, item: PlaybackItem) -> Result<()> {
        match self {
            MusicQueue::Internal(q) => q.replace_item(index, item),
            MusicQueue::OpenHome(q) => q.replace_item(index, item),
        }
    }
}
