use crate::errors::ControlPointError;
use crate::queue::{
    EnqueueMode, InternalQueue, OpenHomeQueue, QueueBackend, QueueFromRendererInfo,
};
use crate::{PlaybackItem, QueueSnapshot, RendererInfo};

#[derive(Debug, Clone)]
pub enum MusicQueue {
    Internal(InternalQueue),
    OpenHome(OpenHomeQueue),
}

impl MusicQueue {
    /// Creates a queue appropriate for the given renderer.
    /// This is the factory method used by QueueFromRendererInfo trait.
    pub fn from_renderer_info(info: &RendererInfo) -> Result<MusicQueue, ControlPointError> {
        if info.capabilities().has_oh_playlist() {
            Ok(MusicQueue::OpenHome(OpenHomeQueue::from_renderer_info(
                info,
            )?))
        } else {
            Ok(MusicQueue::Internal(InternalQueue::from_renderer_info(
                info,
            )?))
        }
    }
}

impl QueueBackend for MusicQueue {
    // Primitives
    fn len(&self) -> Result<usize, ControlPointError> {
        match self {
            MusicQueue::Internal(q) => q.len(),
            MusicQueue::OpenHome(q) => q.len(),
        }
    }

    fn track_ids(&self) -> Result<Vec<u32>, ControlPointError> {
        match self {
            MusicQueue::Internal(q) => q.track_ids(),
            MusicQueue::OpenHome(q) => q.track_ids(),
        }
    }

    fn id_to_position(&self, id: u32) -> Result<usize, ControlPointError> {
        match self {
            MusicQueue::Internal(q) => q.id_to_position(id),
            MusicQueue::OpenHome(q) => q.id_to_position(id),
        }
    }

    fn position_to_id(&self, id: usize) -> Result<u32, ControlPointError> {
        match self {
            MusicQueue::Internal(q) => q.position_to_id(id),
            MusicQueue::OpenHome(q) => q.position_to_id(id),
        }
    }

    fn current_track(&self) -> Result<Option<u32>, ControlPointError> {
        match self {
            MusicQueue::Internal(q) => q.current_track(),
            MusicQueue::OpenHome(q) => q.current_track(),
        }
    }

    fn current_index(&self) -> Result<Option<usize>, ControlPointError> {
        match self {
            MusicQueue::Internal(q) => q.current_index(),
            MusicQueue::OpenHome(q) => q.current_index(),
        }
    }

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

    fn sync_queue(&mut self, items: Vec<PlaybackItem>) -> Result<(), ControlPointError> {
        match self {
            MusicQueue::Internal(q) => q.sync_queue(items),
            MusicQueue::OpenHome(q) => q.sync_queue(items),
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

    fn enqueue_items(
        &mut self,
        items: Vec<PlaybackItem>,
        mode: EnqueueMode,
    ) -> Result<(), ControlPointError> {
        match self {
            MusicQueue::Internal(q) => q.enqueue_items(items, mode),
            MusicQueue::OpenHome(q) => q.enqueue_items(items, mode),
        }
    }

    // Optimized helpers
    fn clear_queue(&mut self) -> Result<(), ControlPointError> {
        match self {
            MusicQueue::Internal(q) => q.clear_queue(),
            MusicQueue::OpenHome(q) => q.clear_queue(),
        }
    }

    fn is_empty(&self) -> Result<bool, ControlPointError> {
        match self {
            MusicQueue::Internal(q) => q.is_empty(),
            MusicQueue::OpenHome(q) => q.is_empty(),
        }
    }

    fn upcoming_len(&self) -> Result<usize, ControlPointError> {
        match self {
            MusicQueue::Internal(q) => q.upcoming_len(),
            MusicQueue::OpenHome(q) => q.upcoming_len(),
        }
    }

    fn upcoming_items(&self) -> Result<Vec<PlaybackItem>, ControlPointError> {
        match self {
            MusicQueue::Internal(q) => q.upcoming_items(),
            MusicQueue::OpenHome(q) => q.upcoming_items(),
        }
    }

    fn peek_current(&mut self) -> Result<Option<(PlaybackItem, usize)>, ControlPointError> {
        match self {
            MusicQueue::Internal(q) => q.peek_current(),
            MusicQueue::OpenHome(q) => q.peek_current(),
        }
    }

    fn dequeue_next(&mut self) -> Result<Option<(PlaybackItem, usize)>, ControlPointError> {
        match self {
            MusicQueue::Internal(q) => q.dequeue_next(),
            MusicQueue::OpenHome(q) => q.dequeue_next(),
        }
    }

    fn append_or_init_index(&mut self, items: Vec<PlaybackItem>) -> Result<(), ControlPointError> {
        match self {
            MusicQueue::Internal(q) => q.append_or_init_index(items),
            MusicQueue::OpenHome(q) => q.append_or_init_index(items),
        }
    }
}

impl QueueFromRendererInfo for MusicQueue {
    fn from_renderer_info(renderer: &RendererInfo) -> Result<Self, ControlPointError> {
        MusicQueue::from_renderer_info(renderer)
    }

    fn to_backend(self) -> MusicQueue {
        self
    }
}
