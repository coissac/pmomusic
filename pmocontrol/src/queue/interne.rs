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
    errors::ControlPointError,
    queue::{MusicQueue, PlaybackItem, QueueBackend, QueueFromRendererInfo, QueueSnapshot},
    DeviceId, DeviceIdentity, RendererInfo,
};

/// Internal/local queue implementation.
///
/// This is the simplest possible queue backend:
///   - a `Vec<PlaybackItem>`
///   - plus an optional `current_index`.
///
/// It does not talk to any remote service. All operations are pure
/// structural mutations on in-memory data.
#[derive(Debug)]
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

    /// Assure l'invariant : si length > 0 et current_index est None, alors current_index = Some(0)
    fn ensure_current_index_invariant(&mut self) {
        if !self.items.is_empty() && self.current_index.is_none() {
            self.current_index = Some(0);
        }
    }

    /// Vérifie si une durée a diminué (format HH:MM:SS).
    /// Retourne true si new_duration < old_duration.
    fn duration_decreased(old_duration: &str, new_duration: &str) -> bool {
        let parse_duration = |dur: &str| -> Option<u32> {
            let parts: Vec<&str> = dur.split(':').collect();
            if parts.len() == 3 {
                let h: u32 = parts[0].parse().ok()?;
                let m: u32 = parts[1].parse().ok()?;
                let s: u32 = parts[2].parse().ok()?;
                Some(h * 3600 + m * 60 + s)
            } else {
                None
            }
        };

        if let (Some(old_secs), Some(new_secs)) =
            (parse_duration(old_duration), parse_duration(new_duration))
        {
            new_secs < old_secs
        } else {
            false // Impossible de parser: considérer que ça n'a pas diminué
        }
    }

    /// Protège les durées des streams contre la diminution.
    /// Pour chaque item de `new_items`, si c'est un stream avec la même URI qu'un item existant,
    /// et que c'est la même chanson (même titre/artiste), ne met à jour la durée que si elle augmente.
    fn protect_stream_durations(&self, mut new_items: Vec<PlaybackItem>) -> Vec<PlaybackItem> {
        use std::collections::HashMap;

        // Construire une HashMap URI -> (durée, titre, artiste) pour les streams de l'ancienne queue
        let old_stream_metadata: HashMap<String, (String, Option<String>, Option<String>)> = self
            .items
            .iter()
            .filter_map(|item| {
                if let Some(ref meta) = item.metadata {
                    if meta.is_continuous_stream {
                        if let Some(ref duration) = meta.duration {
                            return Some((
                                item.uri.clone(),
                                (duration.clone(), meta.title.clone(), meta.artist.clone()),
                            ));
                        }
                    }
                }
                None
            })
            .collect();

        // Mettre à jour les durées des nouveaux items streams en protégeant contre la diminution
        for item in &mut new_items {
            if let (Some(new_meta), Some((old_duration, old_title, old_artist))) =
                (&mut item.metadata, old_stream_metadata.get(&item.uri))
            {
                if new_meta.is_continuous_stream {
                    // Vérifier si c'est la même chanson
                    let same_track = new_meta.title == *old_title && new_meta.artist == *old_artist;

                    if same_track {
                        // Même chanson: protéger contre la diminution de durée
                        if let Some(ref new_duration) = new_meta.duration {
                            if Self::duration_decreased(old_duration, new_duration) {
                                // Garder l'ancienne durée
                                tracing::trace!(
                                    "InternalQueue protect_stream_durations: uri={}, keeping old duration (decreased): {} vs {}",
                                    item.uri,
                                    old_duration,
                                    new_duration
                                );
                                new_meta.duration = Some(old_duration.clone());
                            }
                        }
                    }
                }
            }
        }

        new_items
    }

    /// Fusionne les métadonnées en protégeant les streams contre la diminution de durée.
    /// Pour les streams continus, si c'est la même chanson (même titre ET même artiste ET même URI),
    /// la durée ne peut jamais diminuer.
    fn merge_metadata_protecting_streams(
        old_metadata: &Option<crate::model::TrackMetadata>,
        new_metadata: &Option<crate::model::TrackMetadata>,
        uri: &str,
    ) -> Option<crate::model::TrackMetadata> {
        match (old_metadata, new_metadata) {
            (Some(old_meta), Some(new_meta)) => {
                // Vérifier si c'est un stream continu
                if new_meta.is_continuous_stream {
                    // Vérifier si c'est la même chanson (titre ET artiste identiques)
                    let same_title = old_meta.title == new_meta.title;
                    let same_artist = old_meta.artist == new_meta.artist;
                    let same_track = same_title && same_artist;

                    if same_track {
                        // Même chanson sur un stream: vérifier que la durée n'a pas diminué
                        let should_update = match (&old_meta.duration, &new_meta.duration) {
                            (Some(old_dur), Some(new_dur)) => {
                                // Parser les durées (format HH:MM:SS)
                                let parse_duration = |dur: &str| -> Option<u32> {
                                    let parts: Vec<&str> = dur.split(':').collect();
                                    if parts.len() == 3 {
                                        let h: u32 = parts[0].parse().ok()?;
                                        let m: u32 = parts[1].parse().ok()?;
                                        let s: u32 = parts[2].parse().ok()?;
                                        Some(h * 3600 + m * 60 + s)
                                    } else {
                                        None
                                    }
                                };

                                if let (Some(old_secs), Some(new_secs)) =
                                    (parse_duration(old_dur), parse_duration(new_dur))
                                {
                                    if new_secs < old_secs {
                                        // Durée a diminué: garder l'ancienne
                                        tracing::trace!(
                                            "InternalQueue merge_metadata: uri={}, REJECTING update (same stream track, duration decreased): {} -> {}",
                                            uri,
                                            old_dur,
                                            new_dur
                                        );
                                        false
                                    } else {
                                        // Durée a augmenté ou est égale: accepter
                                        if new_secs > old_secs {
                                            tracing::debug!(
                                                "InternalQueue merge_metadata: uri={}, same stream track, duration increased: {} -> {}",
                                                uri,
                                                old_dur,
                                                new_dur
                                            );
                                        }
                                        true
                                    }
                                } else {
                                    // Impossible de parser: accepter par défaut
                                    true
                                }
                            }
                            _ => true, // Pas de durée ou une seule des deux: accepter
                        };

                        if should_update {
                            Some(new_meta.clone())
                        } else {
                            // Garder l'ancienne durée
                            Some(old_meta.clone())
                        }
                    } else {
                        // Chanson différente sur un stream: accepter les nouvelles métadonnées
                        tracing::debug!(
                            "InternalQueue merge_metadata: uri={}, different stream track (title or artist changed), accepting update",
                            uri
                        );
                        Some(new_meta.clone())
                    }
                } else {
                    // Fichier normal (non-stream): accepter les nouvelles métadonnées
                    Some(new_meta.clone())
                }
            }
            (_, new_meta) => new_meta.clone(), // Pas d'anciennes métadonnées: utiliser les nouvelles
        }
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
        self.ensure_current_index_invariant();
        Ok(())
    }

    fn sync_queue(&mut self, items: Vec<PlaybackItem>) -> Result<(), ControlPointError> {
        use tracing::debug;

        if items.is_empty() {
            return self.replace_queue(Vec::new(), None);
        }

        // Protéger les durées des streams contre la diminution
        let updated_items = self.protect_stream_durations(items);

        // Récupérer l'item actuel
        let current = self.current_index.and_then(|idx| {
            self.items
                .get(idx)
                .map(|item| (idx, item.uri.clone(), item.didl_id.clone()))
        });

        if let Some((_current_idx, current_uri, current_didl_id)) = current {
            // Chercher l'item actuel dans la nouvelle liste (par URI d'abord, puis par didl_id)
            let new_idx = updated_items
                .iter()
                .position(|item| item.uri == current_uri)
                .or_else(|| {
                    updated_items
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
                self.replace_queue(updated_items, Some(new_idx))
            } else {
                // Item pas trouvé - cela ne devrait pas arriver si la playlist n'a pas changé
                // Loguer pour diagnostic
                debug!(
                    renderer = self.renderer_id.0.as_str(),
                    current_uri = current_uri.as_str(),
                    current_didl_id = current_didl_id.as_str(),
                    new_items_count = updated_items.len(),
                    "sync_queue: current item NOT found in new playlist, preserving as first item"
                );
                let current_item = self.items[self.current_index.unwrap()].clone();
                let mut new_items = Vec::with_capacity(updated_items.len() + 1);
                new_items.push(current_item);
                new_items.extend(updated_items);
                self.replace_queue(new_items, Some(0))
            }
        } else {
            // Pas d'item actuel
            self.replace_queue(updated_items, None)
        }
    }

    fn enqueue_items(
        &mut self,
        items: Vec<PlaybackItem>,
        mode: crate::queue::EnqueueMode,
    ) -> Result<(), ControlPointError> {
        use crate::queue::EnqueueMode;

        // DIAGNOSTIC: Log current queue state before enqueue
        tracing::warn!(
            renderer = self.renderer_id.0.as_str(),
            current_index = self.current_index,
            items_count_before = self.items.len(),
            mode = ?mode,
            items_to_enqueue = items.len(),
            "enqueue_items: START"
        );

        // Protéger les durées des streams contre la diminution
        let protected_items = self.protect_stream_durations(items);

        match mode {
            EnqueueMode::AppendToEnd => {
                self.items.extend(protected_items);
            }
            EnqueueMode::InsertAfterCurrent => {
                let insert_pos = self
                    .current_index
                    .map(|i| (i + 1).min(self.items.len()))
                    .unwrap_or(0);

                for (offset, item) in protected_items.into_iter().enumerate() {
                    self.items.insert(insert_pos + offset, item);
                }
            }
            EnqueueMode::ReplaceAll => {
                self.items = protected_items;
                self.current_index = None;
            }
        }

        self.ensure_current_index_invariant();

        // DIAGNOSTIC: Log queue state after enqueue
        tracing::warn!(
            renderer = self.renderer_id.0.as_str(),
            current_index = self.current_index,
            items_count_after = self.items.len(),
            "enqueue_items: END"
        );

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
        // Protéger les durées des streams contre la diminution
        let protected_items = self.protect_stream_durations(items);
        self.items.extend(protected_items);
        self.ensure_current_index_invariant();
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
