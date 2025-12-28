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

use std::sync::{Arc, Mutex};

use anyhow::Result;

use crate::{
    DeviceId, RendererInfo,
    DeviceIdentity,
    errors::ControlPointError,
    queue::{
        MusicQueue,
        backend::{PlaybackItem, QueueBackend, QueueSnapshot},
    },
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

    pub fn from_renderer_info(
        info: &RendererInfo,
    ) -> Result<InternalQueue, ControlPointError> {
        Ok(InternalQueue::new(
                info.id(),
            ),
        )
    }

    /// Exposes a read-only view of the underlying items.
    pub fn items(&self) -> &[PlaybackItem] {
        &self.items
    }

    /// Exposes the current index (read-only).
    pub fn current_index(&self) -> Option<usize> {
        self.current_index
    }
}

impl QueueBackend for InternalQueue {
    fn queue_snapshot(&self) -> Result<QueueSnapshot, ControlPointError> {
        let mut items = self.items.clone();
        for (i, item) in items.iter_mut().enumerate() {
            item.backend_id = i;
        }

        Ok(QueueSnapshot {
            items,
            current_index: self.current_index,
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
                    self.current_index = None;
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

    fn get_item(&self, index: usize) -> Result<Option<PlaybackItem>, ControlPointError> {
        Ok(self.items.get(index).cloned())
    }

    fn replace_item(&mut self, index: usize, item: PlaybackItem) -> Result<(), ControlPointError> {
        if index < self.items.len() {
            self.items[index] = item;
        }
        Ok(())
    }
}
