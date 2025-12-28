use crate::RendererInfo;
use crate::control_point::openhome_queue::OpenHomeQueue;
use crate::errors::ControlPointError;
use crate::openhome_playlist::OpenHomePlaylistSnapshot;
use crate::queue::backend::{PlaybackItem, QueueBackend, QueueSnapshot};
use crate::queue::interne::InternalQueue;

#[derive(Debug, Clone)]
pub enum MusicQueue {
    Internal(InternalQueue),
    OpenHome(OpenHomeQueue),
}

impl MusicQueue {

    pub fn from_renderer_info(info: &RendererInfo) -> Result<MusicQueue, ControlPointError> {
        if info.capabilities().has_oh_playlist() {

            Ok(MusicQueue::OpenHome(OpenHomeQueue::from_renderer_info(info)?))   
        } else {
            Ok(MusicQueue::Internal(InternalQueue::from_renderer_info(info)?))
        }
    }

    pub fn openhome_playlist_snapshot(&self) -> Result<OpenHomePlaylistSnapshot, ControlPointError> {
        match self {
            MusicQueue::OpenHome(queue) => queue.openhome_playlist_snapshot(),
            _ => Err(ControlPointError::QueueError(format!(
                "OpenHome playlist snapshot is only available for OpenHome queues"
            ))),
        }
    }

    pub fn replace_with_attached_playlist(
        &mut self,
        items: Vec<PlaybackItem>,
        current_index: Option<usize>,
    ) -> Result<(), ControlPointError> {
        match self {
            MusicQueue::OpenHome(queue) => queue.replace_entire_playlist(items, current_index),
            MusicQueue::Internal(queue) => queue.replace_queue(items, current_index),
        }
    }
}


impl QueueBackend for MusicQueue {
    fn queue_snapshot(&self) -> Result<QueueSnapshot, ControlPointError> {
        match self {
            MusicQueue::Internal(q) => q.queue_snapshot(),
            MusicQueue::OpenHome(q) => q.queue_snapshot(),
        }
    }

    fn set_index(&mut self, index: Option<usize>) -> Result<(), ControlPointError> {
        match self {
            MusicQueue::Internal(q) => q.set_index(index),
            MusicQueue::OpenHome(q) => q.set_index(index),
        }
    }

    fn replace_queue(
        &mut self,
        items: Vec<PlaybackItem>,
        current_index: Option<usize>,
    ) -> Result<(), ControlPointError> {
        match self {
            MusicQueue::Internal(q) => q.replace_queue(items, current_index),
            MusicQueue::OpenHome(q) => q.replace_queue(items, current_index),
        }
    }

    fn get_item(&self, index: usize) -> Result<Option<PlaybackItem>, ControlPointError> {
        match self {
            MusicQueue::Internal(q) => q.get_item(index),
            MusicQueue::OpenHome(q) => q.get_item(index),
        }
    }

    fn replace_item(&mut self, index: usize, item: PlaybackItem) -> Result<(), ControlPointError> {
        match self {
            MusicQueue::Internal(q) => q.replace_item(index, item),
            MusicQueue::OpenHome(q) => q.replace_item(index, item),
        }
    }
}
