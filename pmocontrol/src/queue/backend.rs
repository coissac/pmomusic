//! Generic queue abstraction for PMOControl.
//!
//! This module defines:
//!   - the canonical `PlaybackItem` structure used by the ControlPoint queues,
//!   - a generic `QueueSnapshot` view,
//!   - the `EnqueueMode` enum,
//!   - the `QueueBackend` trait, which abstracts queue manipulation for
//!     different backends (internal/local queue, OpenHome playlist, …).
//!
//! Design goals:
//!   - All queue manipulation logic (length, current index, enqueue, replace,
//!     navigation, sync with MediaServer, …) is centralized here.
//!   - Backends only implement a extremely small set of primitives; all
//!     higher–level operations are provided as default methods.
//!   - This trait NEVER starts playback. It only manipulates the queue
//!     structure. Transport/renderer logic (play/pause/seek/…) is handled
//!     elsewhere (e.g. `TransportControl` / `MusicRenderer`).
//!
//! Identity model:
//!   - We are in a UPnP Control Point context.
//!   - Every `PlaybackItem` comes from a UPnP MediaServer (ContentDirectory)
//!     and is a projection of a DIDL-Lite `item`.
//!   - The logical identity of a track is the pair
//!       (media_server_id, didl_id)
//!     where:
//!       * `media_server_id` identifies the UPnP MediaServer,
//!       * `didl_id` is the DIDL-Lite `id` attribute for the item.
//!   - This identity is used by the sync helpers to preserve the current
//!     track across queue rebuilds when the MediaServer content changes.

use crate::{PlaybackItem, QueueSnapshot, errors::ControlPointError};

/// High-level enqueue mode.
///
/// This enum specifies how new items should be inserted relative to the
/// existing queue when using `QueueBackend::enqueue_items`.
#[derive(Clone, Copy, Debug)]
pub enum EnqueueMode {
    /// Append new items at the end of the queue.
    AppendToEnd,
    /// Insert new items immediately after the current playing index
    /// (or at the beginning if there is no current index).
    InsertAfterCurrent,
    /// Replace the whole queue with the new items.
    ReplaceAll,
}

/// Backend abstraction for a renderer queue.
///
/// A `QueueBackend` exposes and manipulates the structural state of a queue
/// for a given renderer instance:
///
///   - list of items,
///   - current index,
///   - replacement and mutation of items.
///
/// It does **not** control playback. Transport actions (“play current item”,
/// “seek”, …) are handled by other components (e.g. `TransportControl`).
///
/// Each queue instance is bound to a single renderer by construction. The
/// trait therefore does not take a `RendererId` parameter; all methods
/// operate directly on `self`.
///
/// Implementors must provide a small set of primitives. All other methods
/// are default helpers that can usually be reused as-is.
pub trait QueueBackend {
    // =====================================================================
    //  BACKEND PRIMITIVES (must be implemented)
    // =====================================================================

    /// Returns the length of this queue.
    fn len(&self) -> Result<usize, ControlPointError>;

    /// Lists all the items in this queue.
    fn track_ids(&self) -> Result<Vec<u32>, ControlPointError>;

    /// Converts a track ID to its position in the queue.
    fn id_to_position(&self, id: u32) -> Result<usize, ControlPointError>;

    /// Converts a track ID to its position in the queue.
    fn position_to_id(&self, id: usize) -> Result<u32, ControlPointError>;

    /// Return the current playing track identifier
    fn current_track(&self) -> Result<Option<u32>, ControlPointError>;

    /// Returns the current playing track index in the queue
    fn current_index(&self) -> Result<Option<usize>, ControlPointError>;

    /// Returns the full snapshot (items + current index) of this queue.
    fn queue_snapshot(&self) -> Result<QueueSnapshot, ControlPointError>;

    /// Sets the current index for this queue.
    ///
    /// This method only updates the queue structure (pointer to the current
    /// item). It MUST NOT start playback.
    fn set_index(&mut self, index: Option<usize>) -> Result<(), ControlPointError>;

    /// Replaces the entire queue with a new list of items and a new
    /// current index.
    fn replace_queue(
        &mut self,
        items: Vec<PlaybackItem>,
        current_index: Option<usize>,
    ) -> Result<(), ControlPointError>;

    /// Updates the queue while adjusting so that the new current_index
    /// corresponds to the old current index track.
    /// If the old current index track is absent from the new queue,
    /// it is kept as the first item and the new items are appended after it.
    fn sync_queue(&mut self, items: Vec<PlaybackItem>) -> Result<(), ControlPointError>;

    /// Returns the item at `index`, if it exists.
    fn get_item(&self, index: usize) -> Result<Option<PlaybackItem>, ControlPointError>;

    /// Replaces the item at `index` with `item`.
    fn replace_item(&mut self, index: usize, item: PlaybackItem) -> Result<(), ControlPointError>;

    /// Enqueues items according to the selected `EnqueueMode`.
    ///
    /// This method only manipulates the queue structure; it does not
    /// start playback.
    fn enqueue_items(
        &mut self,
        items: Vec<PlaybackItem>,
        mode: EnqueueMode,
    ) -> Result<(), ControlPointError>;

    // =====================================================================
    //  DEFAULT HELPERS (backend-agnostic logic)
    // =====================================================================

    /// Clears the queue.
    fn clear_queue(&mut self) -> Result<(), ControlPointError> {
        self.replace_queue(Vec::new(), None)
    }

    /// Returns `true` if the queue is empty.
    fn is_empty(&self) -> Result<bool, ControlPointError> {
        Ok(self.queue_snapshot()?.is_empty())
    }

    /// Returns the list of items that come strictly after the current index.
    fn upcoming_items(&self) -> Result<Vec<PlaybackItem>, ControlPointError> {
        let snapshot = self.queue_snapshot()?;
        let items = match snapshot.current_index {
            None => snapshot.items,
            Some(idx) => snapshot.items.into_iter().skip(idx + 1).collect(),
        };
        Ok(items)
    }

    /// Returns how many items remain in the queue after the current index.
    fn upcoming_len(&self) -> Result<usize, ControlPointError> {
        let snapshot = self.queue_snapshot()?;
        let len = snapshot.items.len();
        match snapshot.current_index {
            None => Ok(len),
            Some(idx) => Ok(len.saturating_sub(idx + 1)),
        }
    }

    /// Returns the current item (or the first pending item if no index is set)
    /// along with the count of remaining items.
    fn peek_current(&mut self) -> Result<Option<(PlaybackItem, usize)>, ControlPointError> {
        let snapshot = self.queue_snapshot()?;
        let QueueSnapshot {
            items,
            current_index,
        } = snapshot;

        if items.is_empty() {
            return Ok(None);
        }

        let len = items.len();
        let (item, resolved_index) = match current_index {
            Some(idx) if idx < len => (items.get(idx).cloned(), Some(idx)),
            _ => (items.first().cloned(), None),
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

    /// Advances the queue to the next item (respecting the current index) and
    /// returns it with the number of remaining items.
    fn dequeue_next(&mut self) -> Result<Option<(PlaybackItem, usize)>, ControlPointError> {
        let snapshot = self.queue_snapshot()?;
        let QueueSnapshot {
            items,
            current_index,
        } = snapshot;

        if items.is_empty() {
            return Ok(None);
        }

        let len = items.len();
        let next_index = match current_index {
            None => 0,
            Some(idx) => {
                let candidate = idx + 1;
                if candidate >= len {
                    return Ok(None);
                }
                candidate
            }
        };

        let Some(item) = items.get(next_index).cloned() else {
            return Ok(None);
        };

        let remaining = len.saturating_sub(next_index + 1);
        self.set_index(Some(next_index))?;
        Ok(Some((item, remaining)))
    }

    /// Replaces the queue with `items` and sets a default index.
    fn replace_all(&mut self, items: Vec<PlaybackItem>) -> Result<(), ControlPointError> {
        if items.is_empty() {
            self.replace_queue(Vec::new(), None)
        } else {
            self.replace_queue(items, Some(0))
        }
    }

    /// Appends items and, if the queue was previously empty, initializes
    /// the current index to `0`.
    fn append_or_init_index(&mut self, items: Vec<PlaybackItem>) -> Result<(), ControlPointError> {
        let was_empty = self.is_empty()?;
        let mut snapshot = self.queue_snapshot()?;
        snapshot.items.extend(items);

        let new_index = if was_empty && !snapshot.items.is_empty() {
            Some(0)
        } else {
            snapshot.current_index
        };

        self.replace_queue(snapshot.items, new_index)
    }

    /// Computes the “next” index.
    fn next_index(&self) -> Result<Option<usize>, ControlPointError> {
        let len = self.len()?;
        if len == 0 {
            return Ok(None);
        }

        match self.current_index()? {
            None => Ok(Some(0)),
            Some(i) if i + 1 < len => Ok(Some(i + 1)),
            _ => Ok(None),
        }
    }

    /// Computes the “previous” index.
    fn previous_index(&self) -> Result<Option<usize>, ControlPointError> {
        match self.current_index()? {
            None => Ok(None),
            Some(0) => Ok(None),
            Some(i) => Ok(Some(i - 1)),
        }
    }

    /// Advances the current index to the next item, if any.
    fn advance(&mut self) -> Result<bool, ControlPointError> {
        if let Some(next) = self.next_index()? {
            self.set_index(Some(next))?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Rewinds the current index to the previous item, if any.
    fn rewind(&mut self) -> Result<bool, ControlPointError> {
        if let Some(prev) = self.previous_index()? {
            self.set_index(Some(prev))?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Convenience helper to update an item "in place" at the given index.
    fn update_item(
        &mut self,
        index: usize,
        update: impl FnOnce(PlaybackItem) -> PlaybackItem,
    ) -> Result<(), ControlPointError> {
        if let Some(item) = self.get_item(index)? {
            let new_item = update(item);
            self.replace_item(index, new_item)
        } else {
            Err(ControlPointError::QueueError(format!(
                "Queue index {} out of range",
                index
            )))
        }
    }
}
