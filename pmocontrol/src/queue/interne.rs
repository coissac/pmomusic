//! Internal (local) queue implementation for PMOControl.
//!
//! This module provides a concrete implementation of the generic
//! `QueueBackend` trait for queues that are fully managed inside the
//! ControlPoint, without delegating playlist management to a remote
//! backend (like OpenHome).
//!
//! In this design, each queue instance is associated to exactly one
//! renderer. The queue does not need to know the renderer identifier:
//! it is "bound" to the renderer by construction, and will be stored
//! directly in the runtime (inside a higher-level `MusicQueue` enum).
//!
//! This internal queue:
//!   - owns its list of `PlaybackItem`s,
//!   - maintains a `current_index`,
//!   - never starts playback (transport control is handled elsewhere).

use crate::{
    DeviceId, DeviceIdentity, RendererInfo,
    errors::ControlPointError,
    queue::{MusicQueue, PlaybackItem, QueueBackend, QueueFromRendererInfo, QueueSnapshot},
};

/// Internal/local queue implementation.
///
/// This is the simplest possible queue backend:
///   - a `Vec<PlaybackItem>`
///   - plus an optional `current_index`.
///
/// It does not talk to any remote service. All operations are pure
/// structural mutations on in-memory data.
#[derive(Clone, Debug)]
pub struct InternalQueue {
    renderer_id: DeviceId,
    items: Vec<PlaybackItem>,
    current_index: Option<usize>,
}

impl InternalQueue {
    /// Creates an empty internal queue.
    pub fn new(renderer_id: DeviceId) -> Self {
        Self {
            renderer_id,
            items: Vec::new(),
            current_index: None,
        }
    }

    pub fn from_renderer_info(info: &RendererInfo) -> Result<InternalQueue, ControlPointError> {
        Ok(InternalQueue::new(info.id()))
    }

    /// Exposes a read-only view of the underlying items.
    pub fn items(&self) -> &[PlaybackItem] {
        &self.items
    }
}

impl QueueBackend for InternalQueue {
    fn len(&self) -> Result<usize, ControlPointError> {
        Ok(self.items.len())
    }

    fn track_ids(&self) -> Result<Vec<u32>, ControlPointError> {
        let ids: Vec<u32> = (0..self.len()?).map(|i| i as u32).collect();
        Ok(ids)
    }

    fn id_to_position(&self, id: u32) -> Result<usize, ControlPointError> {
        Ok(id as usize)
    }

    fn position_to_id(&self, id: usize) -> Result<u32, ControlPointError> {
        u32::try_from(id)
            .map_err(|_| ControlPointError::QueueError(format!("Position {} exceeds u32::MAX", id)))
    }

    fn current_track(&self) -> Result<Option<u32>, ControlPointError> {
        match self.current_index {
            None => Ok(None),
            Some(i) => u32::try_from(i).map(Some).map_err(|_| {
                ControlPointError::QueueError(format!("Current index {} exceeds u32::MAX", i))
            }),
        }
    }

    fn current_index(&self) -> Result<Option<usize>, ControlPointError> {
        Ok(self.current_index)
    }

    fn queue_snapshot(&self) -> Result<QueueSnapshot, ControlPointError> {
        let mut items = self.items.clone();
        for (i, item) in items.iter_mut().enumerate() {
            item.backend_id = i;
        }

        Ok(QueueSnapshot {
            items,
            current_index: self.current_index,
            playlist_id: None,
        })
    }

    fn set_index(&mut self, index: Option<usize>) -> Result<(), ControlPointError> {
        match index {
            None => {
                self.current_index = None;
            }
            Some(i) => {
                if i < self.items.len() {
                    self.current_index = Some(i);
                } else {
                    return Err(ControlPointError::QueueError(format!(
                        "Index out of bound {} >= {}",
                        i,
                        self.items.len()
                    )));
                }
            }
        }
        Ok(())
    }

    fn replace_queue(
        &mut self,
        items: Vec<PlaybackItem>,
        current_index: Option<usize>,
    ) -> Result<(), ControlPointError> {
        self.items = items;
        self.current_index = current_index.filter(|&i| i < self.items.len());
        Ok(())
    }

    fn sync_queue(&mut self, items: Vec<PlaybackItem>) -> Result<(), ControlPointError> {
        use tracing::debug;

        if items.is_empty() {
            return self.replace_queue(Vec::new(), None);
        }

        // Récupérer l'item actuel
        let current = self.current_index.and_then(|idx| {
            self.items
                .get(idx)
                .map(|item| (idx, item.uri.clone(), item.didl_id.clone()))
        });

        if let Some((_current_idx, current_uri, current_didl_id)) = current {
            // Chercher l'item actuel dans la nouvelle liste (par URI d'abord, puis par didl_id)
            let new_idx = items
                .iter()
                .position(|item| item.uri == current_uri)
                .or_else(|| {
                    items
                        .iter()
                        .position(|item| item.didl_id == current_didl_id)
                });

            if let Some(new_idx) = new_idx {
                // Item trouvé dans la nouvelle liste
                debug!(
                    renderer = self.renderer_id.0.as_str(),
                    current_uri = current_uri.as_str(),
                    new_idx,
                    "sync_queue: current item found in new playlist"
                );
                self.replace_queue(items, Some(new_idx))
            } else {
                // Item pas trouvé - cela ne devrait pas arriver si la playlist n'a pas changé
                // Loguer pour diagnostic
                debug!(
                    renderer = self.renderer_id.0.as_str(),
                    current_uri = current_uri.as_str(),
                    current_didl_id = current_didl_id.as_str(),
                    new_items_count = items.len(),
                    "sync_queue: current item NOT found in new playlist, preserving as first item"
                );
                let current_item = self.items[self.current_index.unwrap()].clone();
                let mut new_items = Vec::with_capacity(items.len() + 1);
                new_items.push(current_item);
                new_items.extend(items);
                self.replace_queue(new_items, Some(0))
            }
        } else {
            // Pas d'item actuel
            self.replace_queue(items, None)
        }
    }

    fn enqueue_items(
        &mut self,
        items: Vec<PlaybackItem>,
        mode: crate::queue::EnqueueMode,
    ) -> Result<(), ControlPointError> {
        use crate::queue::EnqueueMode;

        match mode {
            EnqueueMode::AppendToEnd => {
                self.items.extend(items);
            }
            EnqueueMode::InsertAfterCurrent => {
                let insert_pos = self
                    .current_index
                    .map(|i| (i + 1).min(self.items.len()))
                    .unwrap_or(0);

                for (offset, item) in items.into_iter().enumerate() {
                    self.items.insert(insert_pos + offset, item);
                }
            }
            EnqueueMode::ReplaceAll => {
                self.items = items;
                self.current_index = None;
            }
        }
        Ok(())
    }

    fn get_item(&self, index: usize) -> Result<Option<PlaybackItem>, ControlPointError> {
        if index < self.items.len() {
            Ok(self.items.get(index).cloned())
        } else {
            Err(ControlPointError::QueueError(format!(
                "get_item index out of bound {} >= {}",
                index,
                self.items.len()
            )))
        }
    }

    // Optimized helpers for InternalQueue
    fn clear_queue(&mut self) -> Result<(), ControlPointError> {
        self.items.clear();
        self.current_index = None;
        Ok(())
    }

    fn is_empty(&self) -> Result<bool, ControlPointError> {
        Ok(self.items.is_empty())
    }

    fn upcoming_len(&self) -> Result<usize, ControlPointError> {
        let len = self.items.len();
        match self.current_index {
            None => Ok(len),
            Some(idx) => Ok(len.saturating_sub(idx + 1)),
        }
    }

    fn upcoming_items(&self) -> Result<Vec<PlaybackItem>, ControlPointError> {
        let items = match self.current_index {
            None => self.items.clone(),
            Some(idx) => self.items.iter().skip(idx + 1).cloned().collect(),
        };
        Ok(items)
    }

    fn peek_current(&mut self) -> Result<Option<(PlaybackItem, usize)>, ControlPointError> {
        if self.items.is_empty() {
            return Ok(None);
        }

        let len = self.items.len();
        let (item, resolved_index) = match self.current_index {
            Some(idx) if idx < len => (self.items.get(idx).cloned(), Some(idx)),
            _ => {
                // Si current_index est None ou invalide, initialiser à 0
                self.current_index = Some(0);
                (self.items.first().cloned(), Some(0))
            }
        };

        let item = match item {
            Some(item) => item,
            None => return Ok(None),
        };

        let remaining = match resolved_index {
            Some(idx) => len.saturating_sub(idx + 1),
            None => len,
        };

        Ok(Some((item, remaining)))
    }

    fn dequeue_next(&mut self) -> Result<Option<(PlaybackItem, usize)>, ControlPointError> {
        if self.items.is_empty() {
            return Ok(None);
        }

        let len = self.items.len();
        let next_index = match self.current_index {
            None => 0,
            Some(idx) => {
                let candidate = idx + 1;
                if candidate >= len {
                    return Ok(None);
                }
                candidate
            }
        };

        let Some(item) = self.items.get(next_index).cloned() else {
            return Ok(None);
        };

        let remaining = len.saturating_sub(next_index + 1);
        self.current_index = Some(next_index);
        Ok(Some((item, remaining)))
    }

    fn append_or_init_index(&mut self, items: Vec<PlaybackItem>) -> Result<(), ControlPointError> {
        let was_empty = self.items.is_empty();
        self.items.extend(items);

        if was_empty && !self.items.is_empty() {
            self.current_index = Some(0);
        }

        Ok(())
    }

    fn replace_item(&mut self, index: usize, item: PlaybackItem) -> Result<(), ControlPointError> {
        if index < self.items.len() {
            self.items[index] = item;
            Ok(())
        } else {
            Err(ControlPointError::QueueError(format!(
                "Index out of bound {} >= {}",
                index,
                self.items.len()
            )))
        }
    }
}

impl QueueFromRendererInfo for InternalQueue {
    fn from_renderer_info(renderer: &RendererInfo) -> Result<Self, ControlPointError> {
        InternalQueue::from_renderer_info(renderer)
    }

    fn to_backend(self) -> MusicQueue {
        MusicQueue::Internal(self)
    }
}
