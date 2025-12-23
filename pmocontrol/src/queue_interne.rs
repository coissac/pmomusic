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

use anyhow::Result;

use crate::queue_backend::{PlaybackItem, QueueBackend, QueueSnapshot};

/// Internal/local queue implementation.
///
/// This is the simplest possible queue backend:
///   - a `Vec<PlaybackItem>`
///   - plus an optional `current_index`.
///
/// It does not talk to any remote service. All operations are pure
/// structural mutations on in-memory data.
#[derive(Clone, Debug, Default)]
pub struct InternalQueue {
    items: Vec<PlaybackItem>,
    current_index: Option<usize>,
}

impl InternalQueue {
    /// Creates an empty internal queue.
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            current_index: None,
        }
    }

    /// Creates an internal queue from an initial list of items.
    ///
    /// If `set_current_to_first` is `true` and the list is non-empty,
    /// the current index is set to `Some(0)`. Otherwise, it is `None`.
    pub fn from_items(items: Vec<PlaybackItem>, set_current_to_first: bool) -> Self {
        let current_index = if set_current_to_first && !items.is_empty() {
            Some(0)
        } else {
            None
        };
        Self {
            items,
            current_index,
        }
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
    fn queue_snapshot(&self) -> Result<QueueSnapshot> {
        Ok(QueueSnapshot {
            items: self.items.clone(),
            current_index: self.current_index,
        })
    }

    fn set_index(&mut self, index: Option<usize>) -> Result<()> {
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
    ) -> Result<()> {
        self.items = items;
        self.current_index = current_index.filter(|&i| i < self.items.len());
        Ok(())
    }

    fn get_item(&self, index: usize) -> Result<Option<PlaybackItem>> {
        Ok(self.items.get(index).cloned())
    }

    fn replace_item(&mut self, index: usize, item: PlaybackItem) -> Result<()> {
        if index < self.items.len() {
            self.items[index] = item;
        }
        Ok(())
    }
}
