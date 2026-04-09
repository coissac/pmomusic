use std::collections::HashMap;
use std::sync::{atomic::AtomicBool, Arc, Mutex};
use std::time::SystemTime;
use std::usize;

use quick_xml::escape::escape;
use tracing::{debug, trace, warn};

use crate::errors::ControlPointError;
use crate::upnp_clients::{
    OhInfoClient, OhPlaylistClient, OhProductClient, OhTrack, OhTrackEntry,
    OPENHOME_PLAYLIST_HEAD_ID,
};
// use crate::openhome_playlist::{OpenHomePlaylistSnapshot, OpenHomePlaylistTrack};
use crate::queue::{
    EnqueueMode, MusicQueue, PlaybackItem, QueueBackend, QueueFromRendererInfo, QueueSnapshot,
};
use crate::{DeviceId, DeviceIdentity, RendererInfo};

/// Cache for OpenHome track IDs to avoid redundant SOAP calls
#[derive(Debug)]
struct TrackIdsCache {
    /// Cached track IDs
    ids: Option<Vec<u32>>,
    /// Timestamp of last cache update
    last_update: Option<SystemTime>,
}

impl TrackIdsCache {
    fn new() -> Self {
        Self {
            ids: None,
            last_update: None,
        }
    }

    /// Check if cache is valid (not expired and has data)
    fn is_valid(&self) -> bool {
        if let (Some(_), Some(last_update)) = (&self.ids, self.last_update) {
            if let Ok(elapsed) = SystemTime::now().duration_since(last_update) {
                return elapsed.as_millis() < 1000; // TTL: 1 second
            }
        }
        false
    }

    /// Get cached IDs if valid
    fn get(&self) -> Option<Vec<u32>> {
        if self.is_valid() {
            self.ids.clone()
        } else {
            None
        }
    }

    /// Update cache with new IDs
    fn set(&mut self, ids: Vec<u32>) {
        self.ids = Some(ids);
        self.last_update = Some(SystemTime::now());
    }

    /// Invalidate cache (called on write operations)
    fn invalidate(&mut self) {
        self.ids = None;
        self.last_update = None;
    }
}

/// Cache for ReadList results to avoid redundant SOAP calls within a short window.
/// Key: sorted list of requested IDs. TTL: 500ms.
#[derive(Debug)]
struct ReadListCache {
    ids: Option<Vec<u32>>,
    entries: Option<Vec<OhTrackEntry>>,
    last_update: Option<SystemTime>,
}

impl ReadListCache {
    fn new() -> Self {
        Self {
            ids: None,
            entries: None,
            last_update: None,
        }
    }

    fn get(&self, id_list: &[u32]) -> Option<Vec<OhTrackEntry>> {
        if let (Some(cached_ids), Some(entries), Some(last_update)) =
            (&self.ids, &self.entries, self.last_update)
        {
            if let Ok(elapsed) = SystemTime::now().duration_since(last_update) {
                if elapsed.as_millis() < 500 && cached_ids.as_slice() == id_list {
                    return Some(entries.clone());
                }
            }
        }
        None
    }

    fn set(&mut self, ids: Vec<u32>, entries: Vec<OhTrackEntry>) {
        self.ids = Some(ids);
        self.entries = Some(entries);
        self.last_update = Some(SystemTime::now());
    }

    fn invalidate(&mut self) {
        self.ids = None;
        self.entries = None;
        self.last_update = None;
    }
}

/// Cache for current track ID to avoid redundant Id SOAP calls
#[derive(Debug)]
struct CurrentTrackIdCache {
    /// Cached current track ID (None means no track playing, id=0)
    current_id: Option<Option<u32>>,
    /// Timestamp of last cache update
    last_update: Option<SystemTime>,
}

impl CurrentTrackIdCache {
    fn new() -> Self {
        Self {
            current_id: None,
            last_update: None,
        }
    }

    /// Check if cache is valid (not expired and has data)
    fn is_valid(&self) -> bool {
        if let (Some(_), Some(last_update)) = (&self.current_id, self.last_update) {
            if let Ok(elapsed) = SystemTime::now().duration_since(last_update) {
                return elapsed.as_millis() < 250; // TTL: 250ms
            }
        }
        false
    }

    /// Get cached current track ID if valid
    fn get(&self) -> Option<Option<u32>> {
        if self.is_valid() {
            self.current_id
        } else {
            None
        }
    }

    /// Update cache with new current track ID
    fn set(&mut self, id: Option<u32>) {
        self.current_id = Some(id);
        self.last_update = Some(SystemTime::now());
    }

    /// Invalidate cache (called on write operations)
    fn invalidate(&mut self) {
        self.current_id = None;
        self.last_update = None;
    }
}

/// Local mirror of an OpenHome playlist for a single renderer.
#[derive(Debug)]
pub struct OpenHomeQueue {
    renderer_id: DeviceId,
    playlist_client: OhPlaylistClient,
    info_client: Option<OhInfoClient>,
    product_client: Option<OhProductClient>,
    /// Cache des métadonnées par ID OpenHome.
    /// Permet de maintenir des métadonnées à jour même si le service OpenHome
    /// ne permet pas de les modifier directement.
    metadata_cache: Mutex<HashMap<u32, Option<crate::model::TrackMetadata>>>,
    /// Cache URI by track ID for fast path matching
    uri_by_id: Mutex<HashMap<u32, String>>,
    /// Cache for track IDs to avoid redundant IdArray SOAP calls
    track_ids_cache: Arc<Mutex<TrackIdsCache>>,
    /// Cache for current track ID to avoid redundant Id SOAP calls
    current_track_id_cache: Arc<Mutex<CurrentTrackIdCache>>,
    /// Cache for ReadList results (TTL 500ms) to avoid redundant SOAP calls
    read_list_cache: Arc<Mutex<ReadListCache>>,
}

/// Fast path result for queue sync optimization
enum FastPathResult {
    /// New items are an append to the current queue
    AppendOnly { new_items: Vec<PlaybackItem> },
    /// Items were deleted from the end
    DeleteFromEnd { delete_ids: Vec<u32> },
    /// No fast path possible - need full LCS sync
    NeedFullSync,
}

impl OpenHomeQueue {
    pub fn new(
        renderer_id: DeviceId,
        playlist: OhPlaylistClient,
        info_client: Option<OhInfoClient>,
        product_client: Option<OhProductClient>,
    ) -> Self {
        Self {
            renderer_id,
            playlist_client: playlist,
            info_client,
            product_client,
            metadata_cache: Mutex::new(HashMap::new()),
            uri_by_id: Mutex::new(HashMap::new()),
            track_ids_cache: Arc::new(Mutex::new(TrackIdsCache::new())),
            current_track_id_cache: Arc::new(Mutex::new(CurrentTrackIdCache::new())),
            read_list_cache: Arc::new(Mutex::new(ReadListCache::new())),
        }
    }

    pub fn from_renderer_info(info: &RendererInfo) -> Result<OpenHomeQueue, ControlPointError> {
        let playlist_client = OhPlaylistClient::from_renderer_info(info)?;
        let info_client = OhInfoClient::from_renderer_info(&info).ok();
        let product_client = OhProductClient::from_renderer_info(&info).ok();

        Ok(OpenHomeQueue::new(
            info.id(),
            playlist_client,
            info_client,
            product_client,
        ))
    }

    fn ensure_playlist_source_selected(&self) -> Result<(), ControlPointError> {
        if let Some(product) = &self.product_client {
            product.ensure_playlist_source_selected()
        } else {
            Ok(())
        }
    }

    /// Invalide les caches track_ids et read_list (après insert/delete sans impact sur la piste courante).
    fn invalidate_track_caches(&self) {
        self.track_ids_cache.lock().unwrap().invalidate();
        self.read_list_cache.lock().unwrap().invalidate();
    }

    /// Invalide tous les caches (après delete_all, seek, stop — opérations qui changent la piste courante).
    fn invalidate_all_caches(&self) {
        self.track_ids_cache.lock().unwrap().invalidate();
        self.read_list_cache.lock().unwrap().invalidate();
        self.current_track_id_cache.lock().unwrap().invalidate();
        self.uri_by_id.lock().unwrap().clear();
    }

    /// Tries to detect a simple append-only or delete-from-end pattern without ReadList.
    ///
    /// This is a fast path optimization that avoids the expensive ReadList calls when:
    /// 1. New items are an append to existing queue (only inserts needed)
    /// 2. Items were deleted from the end (only deletes needed)
    ///
    /// Returns `FastPathResult::NeedFullSync` if the pattern doesn't match or
    /// if cache data is insufficient to verify.
    fn try_fast_path(&self, new_items: &[PlaybackItem]) -> FastPathResult {
        let current_ids = match self.track_ids() {
            Ok(ids) => ids,
            Err(e) => {
                debug!(
                    renderer = self.renderer_id.0.as_str(),
                    error = %e,
                    "try_fast_path: failed to get current IDs, falling back to full sync"
                );
                return FastPathResult::NeedFullSync;
            }
        };

        let current_len = current_ids.len();
        let new_len = new_items.len();

        if new_len > current_len {
            let prefix_matches = self.check_prefix_matches(&current_ids, &new_items[..current_len]);
            if prefix_matches {
                debug!(
                    renderer = self.renderer_id.0.as_str(),
                    current_len, new_len, "try_fast_path: detected append-only pattern"
                );
                return FastPathResult::AppendOnly {
                    new_items: new_items[current_len..].to_vec(),
                };
            }
        } else if new_len < current_len {
            let prefix_matches = self.check_prefix_matches(&current_ids[..new_len], new_items);
            if prefix_matches {
                let delete_ids: Vec<u32> = current_ids[new_len..].to_vec();
                debug!(
                    renderer = self.renderer_id.0.as_str(),
                    current_len,
                    new_len,
                    delete_count = delete_ids.len(),
                    "try_fast_path: detected delete-from-end pattern"
                );
                return FastPathResult::DeleteFromEnd { delete_ids };
            }
        }

        debug!(
            renderer = self.renderer_id.0.as_str(),
            current_len, new_len, "try_fast_path: no fast path detected, falling back to full sync"
        );
        FastPathResult::NeedFullSync
    }

    /// Checks if the prefix of the new items matches the current queue items.
    /// Uses local metadata cache or falls back to URI comparison.
    fn check_prefix_matches(&self, current_ids: &[u32], new_items: &[PlaybackItem]) -> bool {
        if current_ids.len() != new_items.len() {
            return false;
        }

        let metadata_cache = match self.metadata_cache.lock() {
            Ok(cache) => cache,
            Err(_) => return false,
        };

        let uri_cache = match self.uri_by_id.lock() {
            Ok(cache) => cache,
            Err(_) => return false,
        };

        for (idx, new_item) in new_items.iter().enumerate() {
            let current_id = current_ids[idx];

            // First check if we have cached metadata for this track_id that matches
            if let Some(cached_metadata) = metadata_cache.get(&current_id) {
                if let Some(cached) = cached_metadata {
                    // Compare using title + artist as identity (for streams)
                    if let Some(ref new_metadata) = new_item.metadata {
                        let same_title = cached.title.as_ref() == new_metadata.title.as_ref();
                        let same_artist = cached.artist.as_ref() == new_metadata.artist.as_ref();
                        if same_title && same_artist {
                            continue;
                        }
                    }
                }
            }

            // No metadata match - fall back to URI comparison using uri_by_id cache
            if new_item.uri.is_empty() {
                return false;
            }

            // Compare against cached URI for this track_id
            if let Some(cached_uri) = uri_cache.get(&current_id) {
                if cached_uri == &new_item.uri {
                    continue;
                }
            }

            // No match found - fall back to full sync
            return false;
        }

        true
    }

    /// Met à jour les métadonnées d'un item de la queue à l'index spécifié.
    ///
    /// Contrairement au service OpenHome qui ne permet pas de modifier les métadonnées,
    /// cette méthode met à jour le cache local de métadonnées, permettant ainsi au
    /// control point de maintenir des métadonnées à jour même si le média serveur
    /// les modifie.
    ///
    /// # Arguments
    /// * `index` - Position de l'item dans la queue (0-based)
    /// * `metadata` - Nouvelles métadonnées à associer à l'item
    ///
    /// # Errors
    /// Retourne une erreur si l'index est hors limites.
    pub fn update_item_metadata(
        &mut self,
        index: usize,
        metadata: Option<crate::model::TrackMetadata>,
    ) -> Result<(), ControlPointError> {
        let track_id = self.position_to_id(index)?;
        self.cache_metadata(track_id, metadata, "");
        Ok(())
    }

    /// Insère ou met à jour les métadonnées dans le cache.
    /// RÈGLE: Pour les streams continus, pour une même chanson (même titre ET même artiste),
    /// la durée ne peut jamais diminuer. Si le titre ou l'artiste change, c'est une nouvelle
    /// chanson donc toute durée est acceptée.
    /// Pour les fichiers normaux (non-streams), les métadonnées sont acceptées telles quelles.
    /// Cette fonction est le SEUL point d'entrée pour modifier le cache.
    fn cache_metadata(
        &self,
        track_id: u32,
        new_metadata: Option<crate::model::TrackMetadata>,
        uri: &str,
    ) {
        let mut cache = self.metadata_cache.lock().unwrap();
        let mut uri_cache = self.uri_by_id.lock().unwrap();

        // Update URI cache
        if !uri.is_empty() {
            uri_cache.insert(track_id, uri.to_string());
        }

        // Vérifier s'il y a déjà des métadonnées en cache
        if let Some(cached_meta) = cache.get(&track_id) {
            // Vérifier si c'est un stream continu
            let is_stream = new_metadata
                .as_ref()
                .map(|m| m.is_continuous_stream)
                .unwrap_or(false);

            if is_stream {
                // Pour les streams: vérifier si c'est la même chanson (titre ET artiste identiques)
                let same_title = cached_meta.as_ref().and_then(|m| m.title.as_ref())
                    == new_metadata.as_ref().and_then(|m| m.title.as_ref());
                let same_artist = cached_meta.as_ref().and_then(|m| m.artist.as_ref())
                    == new_metadata.as_ref().and_then(|m| m.artist.as_ref());

                let same_track = same_title && same_artist;

                if same_track {
                    // Même chanson: vérifier que la durée n'a pas diminué
                    let should_update = match (
                        cached_meta.as_ref().and_then(|m| m.duration.as_ref()),
                        new_metadata.as_ref().and_then(|m| m.duration.as_ref()),
                    ) {
                        (Some(cached_dur), Some(new_dur)) => {
                            if super::stream_duration_decreased(cached_dur, new_dur) {
                                tracing::trace!(
                                    "OpenHome cache_metadata: track_id={}, REJECTING update (same track, duration decreased): {} -> {}",
                                    track_id,
                                    cached_dur,
                                    new_dur
                                );
                                false
                            } else {
                                if super::stream_duration_increased(cached_dur, new_dur) {
                                    tracing::debug!(
                                        "OpenHome cache_metadata: track_id={}, same track, duration increased: {} -> {}",
                                        track_id,
                                        cached_dur,
                                        new_dur
                                    );
                                }
                                true
                            }
                        }
                        _ => true, // Pas de durée ou une seule des deux: accepter
                    };

                    if should_update {
                        cache.insert(track_id, new_metadata);
                    }
                } else {
                    // Chanson différente sur un stream: accepter sans vérification
                    tracing::debug!(
                        "OpenHome cache_metadata: track_id={}, different stream track (title or artist changed), accepting update",
                        track_id
                    );
                    cache.insert(track_id, new_metadata);
                }
            } else {
                // Fichier normal (non-stream): accepter toute mise à jour
                tracing::trace!(
                    "OpenHome cache_metadata: track_id={}, non-stream file, accepting update",
                    track_id
                );
                cache.insert(track_id, new_metadata);
            }
        } else {
            // Pas dans le cache: insérer directement
            tracing::trace!(
                "OpenHome cache_metadata: track_id={}, inserting first time, duration={:?}, is_stream={:?}",
                track_id,
                new_metadata.as_ref().and_then(|m| m.duration.as_ref()),
                new_metadata.as_ref().map(|m| m.is_continuous_stream)
            );
            cache.insert(track_id, new_metadata);
        }
    }

    fn playback_item_from_entry(&self, entry: &OhTrackEntry) -> PlaybackItem {
        // TOUJOURS préférer les métadonnées du cache si disponibles
        // Le cache contient les métadonnées stables mises lors de l'insertion
        // Les métadonnées de l'entry (venant de ReadList) changent pour les streams
        let metadata = {
            let cache = self.metadata_cache.lock().unwrap();
            if let Some(cached_meta) = cache.get(&entry.id) {
                // Utiliser les métadonnées stables du cache
                tracing::trace!(
                    "OpenHome playback_item_from_entry: track_id={}, using CACHE, duration={:?}",
                    entry.id,
                    cached_meta.as_ref().and_then(|m| m.duration.as_ref())
                );
                cached_meta.clone()
            } else {
                // Pas dans le cache (piste existante avant démarrage de PMOMusic ou ajoutée par autre control point)
                // Utiliser les métadonnées fraîches de l'entry et les mettre en cache pour stabiliser
                let fresh = entry.metadata();
                tracing::debug!(
                    "OpenHome playback_item_from_entry: track_id={}, caching metadata from entry (first read), duration={:?}",
                    entry.id,
                    fresh.as_ref().and_then(|m| m.duration.as_ref())
                );
                drop(cache); // Libérer le lock avant d'appeler cache_metadata
                             // Mettre en cache pour éviter les oscillations sur les flux radio
                self.cache_metadata(entry.id, fresh.clone(), entry.uri());
                fresh
            }
        };

        let didl_id = entry
            .didl_id()
            .unwrap_or_else(|| format!("openhome:{}", entry.id));
        PlaybackItem {
            media_server_id: DeviceId(format!("openhome:{}", self.renderer_id.0)),
            backend_id: entry.id as usize,
            didl_id,
            uri: entry.uri().to_string(),
            // OpenHome tracks don't provide protocolInfo, use generic default
            protocol_info: "http-get:*:audio/*:*".to_string(),
            metadata,
        }
    }

    fn add_playback_item(
        &mut self,
        item: PlaybackItem,
        after_id: u32,
    ) -> Result<u32, ControlPointError> {
        self.ensure_playlist_source_selected()?;
        let metadata_xml = build_metadata_xml(&item);
        let new_id = self
            .playlist_client
            .insert(after_id, &item.uri, &metadata_xml)?;

        // Enregistrer les métadonnées dans le cache
        self.cache_metadata(new_id, item.metadata, &item.uri);

        Ok(new_id)
    }

    /// CASE 1: Replace queue while preserving the currently playing item as first.
    /// The currently playing item is NOT in the new playlist, so we keep it as the first
    /// item and append the entire new playlist after it.
    fn replace_queue_preserve_current(
        &mut self,
        new_items: Vec<PlaybackItem>,
        playing_id: usize,
        cancel_token: &Arc<AtomicBool>,
        on_ready: &mut Option<Box<dyn FnOnce() + Send>>,
    ) -> Result<(), ControlPointError> {
        use std::sync::atomic::Ordering::SeqCst;

        let current_track_ids = self.track_ids()?;

        for &track_id in current_track_ids.iter().rev() {
            if cancel_token.load(SeqCst) {
                return Err(ControlPointError::SyncCancelled);
            }
            if track_id as usize != playing_id {
                self.playlist_client.delete_id_if_exists(track_id)?;
                self.metadata_cache.lock().unwrap().remove(&track_id);
            }
        }

        let mut previous_id = playing_id as u32;
        let mut first_insert_done = false;
        for item in new_items {
            if cancel_token.load(SeqCst) {
                return Err(ControlPointError::SyncCancelled);
            }
            let metadata = build_metadata_xml(&item);
            let new_id = self
                .playlist_client
                .insert(previous_id, &item.uri, &metadata)?;

            self.cache_metadata(new_id, item.metadata, &item.uri);

            previous_id = new_id;

            if !first_insert_done && on_ready.is_some() {
                first_insert_done = true;
                if let Some(f) = on_ready.take() {
                    f();
                }
            }
        }

        debug!(
            renderer = self.renderer_id.0.as_str(),
            "Gentle sync completed: preserved playing track as first item (not in new playlist)"
        );

        self.invalidate_track_caches();

        Ok(())
    }

    /// Helper: Delete items marked for deletion in reverse order with logging.
    fn delete_marked_items(
        &mut self,
        old_ids: &[u32],
        keep_flags: &[bool],
        position_label: &str,
        cancel_token: &Arc<AtomicBool>,
    ) -> Result<(), ControlPointError> {
        use std::sync::atomic::Ordering::SeqCst;
        for (idx, &track_id) in old_ids.iter().enumerate().rev() {
            if cancel_token.load(SeqCst) {
                return Err(ControlPointError::SyncCancelled);
            }
            if !keep_flags[idx] {
                debug!(
                    renderer = self.renderer_id.0.as_str(),
                    track_id,
                    position = position_label,
                    "RENDERER OP: DeleteId({})",
                    track_id
                );
                self.playlist_client.delete_id_if_exists(track_id)?;
                self.metadata_cache.lock().unwrap().remove(&track_id);
            }
        }
        Ok(())
    }

    /// Helper: Rebuild a playlist section (before or after pivot) using LCS results.
    fn rebuild_playlist_section(
        &mut self,
        new_items: &[PlaybackItem],
        keep_new_flags: &[bool],
        old_ids: &[u32],
        keep_old_flags: &[bool],
        mut previous_id: u32,
        position_label: &str,
        cancel_token: &Arc<AtomicBool>,
        on_ready: &mut Option<Box<dyn FnOnce() + Send>>,
    ) -> Result<u32, ControlPointError> {
        use std::sync::atomic::Ordering::SeqCst;

        let remaining_ids: Vec<u32> = old_ids
            .iter()
            .enumerate()
            .filter_map(|(idx, &id)| if keep_old_flags[idx] { Some(id) } else { None })
            .collect();

        let mut remaining_idx = 0;
        let mut first_insert_done = false;

        for (idx, item) in new_items.iter().enumerate() {
            if cancel_token.load(SeqCst) {
                return Err(ControlPointError::SyncCancelled);
            }
            if keep_new_flags[idx] {
                let existing_id = remaining_ids[remaining_idx];
                remaining_idx += 1;
                previous_id = existing_id;

                self.cache_metadata(existing_id, item.metadata.clone(), &item.uri);

                debug!(
                    renderer = self.renderer_id.0.as_str(),
                    track_id = existing_id,
                    position = position_label,
                    "KEPT existing track ID {}",
                    existing_id
                );
            } else {
                let metadata = build_metadata_xml(item);
                let new_id = self
                    .playlist_client
                    .insert(previous_id, &item.uri, &metadata)?;

                self.cache_metadata(new_id, item.metadata.clone(), &item.uri);

                debug!(
                    renderer = self.renderer_id.0.as_str(),
                    after_id = previous_id,
                    new_id,
                    position = position_label,
                    "RENDERER OP: Insert(after={}) -> new_id={}",
                    previous_id,
                    new_id
                );
                previous_id = new_id;

                if !first_insert_done && on_ready.is_some() {
                    first_insert_done = true;
                    if let Some(f) = on_ready.take() {
                        f();
                    }
                }
            }
        }

        Ok(previous_id)
    }

    /// CASE 2: Replace queue with double-LCS (before and after the pivot).
    /// The currently playing item IS in the new playlist, so we use it as a pivot
    /// and apply LCS separately to the portions before and after it.
    fn replace_queue_with_pivot(
        &mut self,
        new_items: Vec<PlaybackItem>,
        pivot_idx_new: usize,
        pivot_id: usize,
        snapshot: &QueueSnapshot,
        current_track_ids: &[u32],
        cancel_token: &Arc<AtomicBool>,
        on_ready: &mut Option<Box<dyn FnOnce() + Send>>,
    ) -> Result<(), ControlPointError> {
        // Find the pivot index in our current state
        let pivot_idx = current_track_ids
            .iter()
            .position(|&id| id as usize == pivot_id)
            .ok_or_else(|| {
                ControlPointError::OpenHomeError(format!(
                    "Pivot track ID {} not found in playlist",
                    pivot_id
                ))
            })?;

        // Split current data at the pivot
        let old_before: Vec<PlaybackItem> = snapshot.items[..pivot_idx].to_vec();
        let old_after: Vec<PlaybackItem> = snapshot.items[pivot_idx + 1..].to_vec();
        let old_ids_before: Vec<u32> = current_track_ids[..pivot_idx].to_vec();
        let old_ids_after: Vec<u32> = current_track_ids[pivot_idx + 1..].to_vec();

        let new_before = &new_items[..pivot_idx_new];
        let new_after = &new_items[pivot_idx_new + 1..];

        // LCS on the AFTER part (using fresh data from OpenHome)
        let (keep_old_after, keep_new_after) = lcs_flags_optimized(&old_after, new_after);

        // LCS on the BEFORE part (using fresh data from OpenHome)
        let (keep_old_before, keep_new_before) = lcs_flags_optimized(&old_before, new_before);

        // Delete items marked for deletion in AFTER part (reverse order)
        self.delete_marked_items(&old_ids_after, &keep_old_after, "AFTER pivot", cancel_token)?;

        // Delete items marked for deletion in BEFORE part (reverse order)
        self.delete_marked_items(
            &old_ids_before,
            &keep_old_before,
            "BEFORE pivot",
            cancel_token,
        )?;

        // Rebuild the playlist: [BEFORE, PIVOT, AFTER]
        // Rebuild BEFORE part (we don't need the returned previous_id)
        self.rebuild_playlist_section(
            new_before,
            &keep_new_before,
            &old_ids_before,
            &keep_old_before,
            OPENHOME_PLAYLIST_HEAD_ID,
            "BEFORE pivot",
            cancel_token,
            on_ready,
        )?;

        // PIVOT keeps its ID and position - it's the anchor point
        let previous_id = pivot_id as u32;

        // Mettre à jour les métadonnées du pivot
        self.cache_metadata(
            previous_id,
            new_items[pivot_idx_new].metadata.clone(),
            &new_items[pivot_idx_new].uri,
        );

        debug!(
            renderer = self.renderer_id.0.as_str(),
            pivot_id,
            pivot_idx_new,
            "PIVOT preserved with ID {} at index {}",
            pivot_id,
            pivot_idx_new
        );

        // Rebuild AFTER part
        self.rebuild_playlist_section(
            new_after,
            &keep_new_after,
            &old_ids_after,
            &keep_old_after,
            previous_id,
            "AFTER pivot",
            cancel_token,
            on_ready,
        )?;

        debug!(
            renderer = self.renderer_id.0.as_str(),
            pivot_idx = pivot_idx_new,
            pivot_id,
            "Gentle sync completed: double-LCS with pivot (playing track preserved)"
        );

        // Invalidate cache after playlist modifications
        self.invalidate_track_caches();

        Ok(())
    }

    /// Standard LCS-based replacement (used when no currently playing item).
    fn replace_queue_standard_lcs(
        &mut self,
        items: Vec<PlaybackItem>,
        snapshot: &QueueSnapshot,
        current_track_ids: &[u32],
        cancel_token: &Arc<AtomicBool>,
        on_ready: &mut Option<Box<dyn FnOnce() + Send>>,
    ) -> Result<(), ControlPointError> {
        use std::sync::atomic::Ordering::SeqCst;

        debug!(
            renderer = self.renderer_id.0.as_str(),
            current_count = snapshot.items.len(),
            desired_count = items.len(),
            current_uris = ?snapshot.items.iter().map(|i| i.uri.as_str()).collect::<Vec<_>>(),
            current_didl_ids = ?snapshot.items.iter().map(|i| i.didl_id.as_str()).collect::<Vec<_>>(),
            desired_uris = ?items.iter().map(|i| i.uri.as_str()).collect::<Vec<_>>(),
            desired_didl_ids = ?items.iter().map(|i| i.didl_id.as_str()).collect::<Vec<_>>(),
            "LCS input: current vs desired items"
        );

        let (keep_current, keep_desired) = lcs_flags_optimized(&snapshot.items, &items);

        let items_to_keep = keep_current.iter().filter(|&&k| k).count();
        let items_to_delete = keep_current.iter().filter(|&k| !k).count();
        let items_to_add = keep_desired.iter().filter(|&k| !k).count();

        debug!(
            renderer = self.renderer_id.0.as_str(),
            keep = items_to_keep,
            delete = items_to_delete,
            add = items_to_add,
            "LCS computed: minimizing OpenHome playlist operations"
        );

        let current_track_id = self.playlist_client.id().ok().filter(|&id| id != 0);

        let current_track_in_new_playlist = current_track_id.and_then(|current_id| {
            items
                .iter()
                .position(|item| item.backend_id as u32 == current_id)
        });

        if items_to_keep == 0 && items_to_delete > 0 {
            if current_track_in_new_playlist.is_some() {
                debug!(
                    renderer = self.renderer_id.0.as_str(),
                    current_track_in_playlist = true,
                    "Preserving currently playing track - using insert/delete instead of delete_all"
                );
            } else {
                debug!(
                    renderer = self.renderer_id.0.as_str(),
                    "Using delete_all() for complete replacement (safe - no current track or not in new playlist)"
                );
                self.playlist_client.delete_all()?;
                self.metadata_cache.lock().unwrap().clear();
            }
        } else {
            for idx in (0..current_track_ids.len()).rev() {
                if cancel_token.load(SeqCst) {
                    return Err(ControlPointError::SyncCancelled);
                }
                if !keep_current[idx] {
                    let track_id = current_track_ids[idx];
                    self.playlist_client.delete_id_if_exists(track_id)?;
                    self.metadata_cache.lock().unwrap().remove(&track_id);
                }
            }
        }

        let remaining_ids: Vec<u32> = current_track_ids
            .iter()
            .enumerate()
            .filter_map(|(idx, &id)| {
                if keep_current.get(idx).copied().unwrap_or(false) {
                    Some(id)
                } else {
                    None
                }
            })
            .collect();

        let mut remaining_idx = 0usize;
        let mut previous_id = OPENHOME_PLAYLIST_HEAD_ID;
        let mut first_insert_done = false;

        for (idx, item) in items.into_iter().enumerate() {
            if cancel_token.load(SeqCst) {
                return Err(ControlPointError::SyncCancelled);
            }
            if keep_desired[idx] {
                if remaining_idx >= remaining_ids.len() {
                    return Err(ControlPointError::OpenHomeError(format!(
                        "OpenHome playlist refresh bookkeeping mismatch (kept entries underflow)"
                    )));
                }
                let existing_id = remaining_ids[remaining_idx];
                remaining_idx += 1;
                previous_id = existing_id;

                self.cache_metadata(existing_id, item.metadata, &item.uri);
            } else {
                let metadata = build_metadata_xml(&item);
                let new_id = self
                    .playlist_client
                    .insert(previous_id, &item.uri, &metadata)?;

                self.cache_metadata(new_id, item.metadata, &item.uri);

                previous_id = new_id;

                if !first_insert_done && on_ready.is_some() {
                    first_insert_done = true;
                    if let Some(f) = on_ready.take() {
                        f();
                    }
                }
            }
        }

        if remaining_idx != remaining_ids.len() {
            return Err(ControlPointError::OpenHomeError(format!(
                "OpenHome playlist refresh bookkeeping mismatch (kept entries overflow)"
            )));
        }

        self.invalidate_track_caches();

        Ok(())
    }
}

fn build_metadata_xml(item: &PlaybackItem) -> String {
    let title = item
        .metadata
        .as_ref()
        .and_then(|m| m.title.as_deref())
        .unwrap_or("Unknown");
    let escaped_title = escape(title);
    let escaped_uri = escape(item.uri.as_str());
    let escaped_id = escape(item.didl_id.as_str());

    let mut xml = String::from(
        r#"<DIDL-Lite xmlns="urn:schemas-upnp-org:metadata-1-0/DIDL-Lite/" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:upnp="urn:schemas-upnp-org:metadata-1-0/upnp/">"#,
    );
    xml.push_str(&format!(
        r#"<item id="{}" parentID="-1" restricted="1">"#,
        escaped_id
    ));
    xml.push_str(&format!("<dc:title>{}</dc:title>", escaped_title));

    if let Some(meta) = &item.metadata {
        if let Some(artist) = meta.artist.as_deref() {
            let escaped = escape(artist);
            xml.push_str(&format!("<upnp:artist>{}</upnp:artist>", escaped));
            xml.push_str(&format!("<dc:creator>{}</dc:creator>", escaped));
        }
        if let Some(album) = meta.album.as_deref() {
            let escaped = escape(album);
            xml.push_str(&format!("<upnp:album>{}</upnp:album>", escaped));
        }
        if let Some(genre) = meta.genre.as_deref() {
            let escaped = escape(genre);
            xml.push_str(&format!("<upnp:genre>{}</upnp:genre>", escaped));
        }
        if let Some(uri) = meta.album_art_uri.as_deref() {
            let escaped = escape(uri);
            xml.push_str(&format!("<upnp:albumArtURI>{}</upnp:albumArtURI>", escaped));
        }
        if let Some(date) = meta.date.as_deref() {
            let escaped = escape(date);
            xml.push_str(&format!("<dc:date>{}</dc:date>", escaped));
        }
        if let Some(track_no) = meta.track_number.as_deref() {
            let escaped = escape(track_no);
            xml.push_str(&format!(
                "<upnp:originalTrackNumber>{}</upnp:originalTrackNumber>",
                escaped
            ));
        }
    }

    let escaped_protocol_info = escape(item.protocol_info.as_str());

    // Build <res> element with optional duration attribute
    xml.push_str(&format!(r#"<res protocolInfo="{}""#, escaped_protocol_info));
    if let Some(meta) = &item.metadata {
        if let Some(duration) = meta.duration.as_deref() {
            let escaped_duration = escape(duration);
            xml.push_str(&format!(r#" duration="{}""#, escaped_duration));
        }
    }
    xml.push_str(&format!(r#">{}</res>"#, escaped_uri));
    xml.push_str(r#"<upnp:class>object.item.audioItem.musicTrack</upnp:class></item></DIDL-Lite>"#);
    xml
}

/// Compare two PlaybackItems for equality.
/// Items are considered equal if they have the same URI OR the same didl_id.
/// This allows matching items even when the MediaServer returns different URIs
/// for the same logical track (e.g., with session tokens or different encodings).
fn items_match(a: &PlaybackItem, b: &PlaybackItem) -> bool {
    a.uri == b.uri || a.didl_id == b.didl_id
}

fn lcs_flags(current: &[PlaybackItem], desired: &[PlaybackItem]) -> (Vec<bool>, Vec<bool>) {
    let m = current.len();
    let n = desired.len();
    let mut dp = vec![vec![0u32; n + 1]; m + 1];

    for i in 0..m {
        for j in 0..n {
            if items_match(&current[i], &desired[j]) {
                dp[i + 1][j + 1] = dp[i][j] + 1;
            } else {
                dp[i + 1][j + 1] = dp[i + 1][j].max(dp[i][j + 1]);
            }
        }
    }

    let mut keep_current = vec![false; m];
    let mut keep_desired = vec![false; n];
    let (mut i, mut j) = (m, n);

    while i > 0 && j > 0 {
        if items_match(&current[i - 1], &desired[j - 1]) {
            keep_current[i - 1] = true;
            keep_desired[j - 1] = true;
            i -= 1;
            j -= 1;
        } else if dp[i - 1][j] >= dp[i][j - 1] {
            i -= 1;
        } else {
            j -= 1;
        }
    }

    (keep_current, keep_desired)
}

fn lcs_flags_optimized(
    current: &[PlaybackItem],
    desired: &[PlaybackItem],
) -> (Vec<bool>, Vec<bool>) {
    if current.is_empty() {
        return (vec![], vec![true; desired.len()]);
    }
    if desired.is_empty() {
        return (vec![true; current.len()], vec![]);
    }

    let leading = current
        .iter()
        .zip(desired.iter())
        .take_while(|(c, d)| items_match(c, d))
        .count();

    let c_tail = &current[leading..];
    let d_tail = &desired[leading..];
    let trailing = c_tail
        .iter()
        .rev()
        .zip(d_tail.iter().rev())
        .take_while(|(c, d)| items_match(c, d))
        .count();

    let c_mid = &c_tail[..c_tail.len().saturating_sub(trailing)];
    let d_mid = &d_tail[..d_tail.len().saturating_sub(trailing)];

    if c_mid.is_empty() && d_mid.is_empty() {
        return (vec![true; current.len()], vec![true; desired.len()]);
    }

    let (keep_c_mid, keep_d_mid) = lcs_flags(c_mid, d_mid);

    let mut keep_current = vec![true; leading];
    keep_current.extend(keep_c_mid);
    keep_current.extend(vec![true; trailing]);

    let mut keep_desired = vec![true; leading];
    keep_desired.extend(keep_d_mid);
    keep_desired.extend(vec![true; trailing]);

    (keep_current, keep_desired)
}

impl QueueBackend for OpenHomeQueue {
    fn len(&self) -> Result<usize, ControlPointError> {
        Ok(self.track_ids()?.len())
    }

    /// Return the list of OpenHome track IDs in order.
    fn track_ids(&self) -> Result<Vec<u32>, ControlPointError> {
        self.ensure_playlist_source_selected()?;

        // Lock the cache for the entire operation to prevent race conditions
        let mut cache = self.track_ids_cache.lock().unwrap();

        // Check if cache is valid
        if let Some(cached_ids) = cache.get() {
            return Ok(cached_ids);
        }

        // Cache miss or expired - fetch from service (keep lock held to prevent concurrent calls)
        let ids = self.playlist_client.id_array()?;

        tracing::trace!(
            renderer = self.renderer_id.0.as_str(),
            ids_count = ids.len(),
            ids = ?ids,
            "track_ids: cache miss, fetched from Pizzicato"
        );

        // Update cache before releasing lock
        cache.set(ids.clone());

        Ok(ids)
    }

    fn id_to_position(&self, id: u32) -> Result<usize, ControlPointError> {
        self.track_ids()?
            .iter()
            .position(|&tid| tid == id)
            .ok_or_else(|| {
                ControlPointError::QueueError(format!("Item {} id is not present in the queue", id))
            })
    }

    fn position_to_id(&self, index: usize) -> Result<u32, ControlPointError> {
        let idxs = self.track_ids()?;

        if index < idxs.len() {
            Ok(idxs[index])
        } else {
            Err(ControlPointError::QueueError(format!(
                "Index out of bound {} >= {}",
                index,
                idxs.len()
            )))
        }
    }

    fn current_track(&self) -> Result<Option<u32>, ControlPointError> {
        // Hold lock during entire operation to prevent race conditions
        let mut cache = self.current_track_id_cache.lock().unwrap();

        // Return cached value if valid
        if let Some(cached_id) = cache.get() {
            return Ok(cached_id);
        }

        // Cache miss - fetch from backend
        let id = self.playlist_client.id()?;
        // OpenHome returns 0 when no track is selected/playing
        let result = if id == 0 { None } else { Some(id) };

        // Update cache
        cache.set(result);

        Ok(result)
    }

    fn current_index(&self) -> Result<Option<usize>, ControlPointError> {
        if let Some(id) = self.current_track()? {
            return Ok(Some(self.id_to_position(id)?));
        }

        Ok(None)
    }

    fn queue_snapshot(&self) -> Result<QueueSnapshot, ControlPointError> {
        self.ensure_playlist_source_selected()?;

        // Use cached track_ids() instead of calling read_all_tracks() which bypasses cache
        let ids = self.track_ids()?;

        if ids.is_empty() {
            return Ok(QueueSnapshot {
                items: Vec::new(),
                current_index: None,
                playlist_id: None,
            });
        }

        // Read metadata for all tracks (batched), with 500ms cache to avoid
        // redundant SOAP calls during sync_queue (which calls queue_snapshot twice).
        const MAX_BATCH: usize = 256;
        let mut entries = Vec::with_capacity(ids.len());
        for chunk in ids.chunks(MAX_BATCH) {
            if let Some(cached) = self.read_list_cache.lock().unwrap().get(chunk) {
                trace!(
                    renderer = self.renderer_id.0.as_str(),
                    "ReadList cache hit for {} IDs",
                    chunk.len()
                );
                entries.extend(cached);
                continue;
            }
            match self.playlist_client.read_list(chunk) {
                Ok(batch) => {
                    self.read_list_cache
                        .lock()
                        .unwrap()
                        .set(chunk.to_vec(), batch.clone());
                    entries.extend(batch);
                }
                Err(err) => {
                    // If batch fails, try one by one
                    if chunk.len() > 1 {
                        for id in chunk {
                            match self.playlist_client.read_list(&[*id]) {
                                Ok(mut single) => entries.append(&mut single),
                                Err(inner_err) => return Err(inner_err),
                            }
                        }
                    } else {
                        return Err(err);
                    }
                }
            }
        }

        let mut items = Vec::with_capacity(entries.len());

        for entry in &entries {
            items.push(self.playback_item_from_entry(entry));
        }

        // Get the currently playing track ID from the renderer (may be None if no track is playing)
        let current_id = self.playlist_client.id().ok();

        // Find the index of the current track in the playlist
        let current_index = current_id.and_then(|id| {
            items
                .iter()
                .position(|entry_id| entry_id.backend_id == id as usize)
        });

        Ok(QueueSnapshot {
            items: items,
            current_index: current_index,
            playlist_id: None,
        })
    }

    fn set_index(&mut self, index: Option<usize>) -> Result<(), ControlPointError> {
        if let Some(index) = index {
            let track_id = self.position_to_id(index)?;
            self.ensure_playlist_source_selected()?;
            self.playlist_client.seek_id(track_id)?;
        } else {
            self.ensure_playlist_source_selected()?;
            tracing::trace!(
                renderer = self.renderer_id.0.as_str(),
                "STOP command via set_index(None) on OpenHome playlist"
            );
            self.playlist_client.stop()?;
        }
        // Invalidate caches (seek_id/stop modifies playlist state and current track)
        self.invalidate_all_caches();
        Ok(())
    }

    fn replace_queue(
        &mut self,
        items: Vec<PlaybackItem>,
        current_index: Option<usize>,
    ) -> Result<(), ControlPointError> {
        if let Some(ci) = current_index {
            if ci >= items.len() {
                return Err(ControlPointError::QueueError(format!(
                    "Invalid Current index parameter {} >= {}",
                    ci,
                    items.len()
                )));
            }
        }

        self.ensure_playlist_source_selected()?;
        self.playlist_client.delete_all()?;
        self.metadata_cache.lock().unwrap().clear();

        // Invalidate caches after delete_all (clears queue and current track)
        self.invalidate_all_caches();

        if items.is_empty() {
            return Ok(());
        }

        let mut previous_id = OPENHOME_PLAYLIST_HEAD_ID;

        for item in items {
            let metadata = build_metadata_xml(&item);
            let new_id = self
                .playlist_client
                .insert(previous_id, &item.uri, &metadata)?;

            // Enregistrer les métadonnées dans le cache
            self.cache_metadata(new_id, item.metadata, &item.uri);

            previous_id = new_id;
        }

        // Invalidate cache after insertions
        self.invalidate_track_caches();

        Ok(())
    }

    fn sync_queue(
        &mut self,
        items: Vec<PlaybackItem>,
        cancel_token: &Arc<AtomicBool>,
        mut on_ready: Option<Box<dyn FnOnce() + Send>>,
    ) -> Result<(), ControlPointError> {
        use std::sync::atomic::Ordering::SeqCst;

        self.ensure_playlist_source_selected()?;

        if cancel_token.load(SeqCst) {
            return Err(ControlPointError::SyncCancelled);
        }

        let pre_current_track = self.playlist_client.id().ok();
        tracing::warn!(
            renderer = self.renderer_id.0.as_str(),
            pre_current_track_id = pre_current_track,
            pre_items_count = items.len(),
            "sync_queue: START - current track before modification"
        );

        if items.is_empty() {
            tracing::warn!(
                renderer = self.renderer_id.0.as_str(),
                "sync_queue: Empty playlist - clearing queue with delete_all"
            );
            self.playlist_client.delete_all()?;
            self.metadata_cache.lock().unwrap().clear();
            self.invalidate_all_caches();

            let post_current_track = self.playlist_client.id().ok();
            tracing::warn!(
                renderer = self.renderer_id.0.as_str(),
                post_current_track_id = post_current_track,
                "sync_queue: END - current track after delete_all (should be 0)"
            );
            return Ok(());
        }

        match self.try_fast_path(&items) {
            FastPathResult::AppendOnly { new_items } => {
                debug!(
                    renderer = self.renderer_id.0.as_str(),
                    new_items_count = new_items.len(),
                    "sync_queue: fast path - append-only detected"
                );
                let current_ids = self.track_ids()?;
                let mut after_id = current_ids
                    .last()
                    .copied()
                    .unwrap_or(OPENHOME_PLAYLIST_HEAD_ID);
                let mut uri_cache = self.uri_by_id.lock().unwrap();
                for item in &new_items {
                    if cancel_token.load(SeqCst) {
                        return Err(ControlPointError::SyncCancelled);
                    }
                    let metadata = build_metadata_xml(item);
                    let uri = item.uri.as_str();
                    let metadata_xml = metadata.as_str();
                    after_id = self.playlist_client.insert(after_id, uri, metadata_xml)?;
                    if !item.uri.is_empty() {
                        uri_cache.insert(after_id, item.uri.clone());
                    }
                }
                drop(uri_cache);
                self.invalidate_track_caches();
                return Ok(());
            }
            FastPathResult::DeleteFromEnd { delete_ids } => {
                debug!(
                    renderer = self.renderer_id.0.as_str(),
                    delete_count = delete_ids.len(),
                    "sync_queue: fast path - delete-from-end detected"
                );
                for id in delete_ids.iter().rev() {
                    if cancel_token.load(SeqCst) {
                        return Err(ControlPointError::SyncCancelled);
                    }
                    self.playlist_client.delete_id(*id)?;
                }
                self.invalidate_all_caches();
                return Ok(());
            }
            FastPathResult::NeedFullSync => {
                debug!(
                    renderer = self.renderer_id.0.as_str(),
                    "sync_queue: fast path not applicable, proceeding with full sync"
                );
            }
        }

        let snapshot = self.queue_snapshot()?;

        debug!(
            renderer = self.renderer_id.0.as_str(),
            snapshot_items_len = snapshot.items.len(),
            snapshot_current_index = snapshot.current_index,
            new_items_count = items.len(),
            "sync_queue: snapshot vs new items comparison"
        );

        let playing_info = snapshot.current_index.and_then(|idx| {
            if idx < snapshot.items.len() {
                Some((
                    idx,
                    snapshot.items[idx].backend_id,
                    snapshot.items[idx].uri.clone(),
                    snapshot.items[idx].didl_id.clone(),
                ))
            } else {
                warn!(
                    renderer = self.renderer_id.0.as_str(),
                    current_index = idx,
                    items_len = snapshot.items.len(),
                    "OpenHome renderer in inconsistent state: current_index out of bounds, treating as no current track"
                );
                None
            }
        });

        debug!(
            renderer = self.renderer_id.0.as_str(),
            actual_items = snapshot.items.len(),
            playing_info_detected = playing_info.is_some(),
            "OpenHome playlist state"
        );

        let has_pivot = playing_info.is_some();
        if has_pivot {
            if let Some(f) = on_ready.take() {
                f();
            }
        }

        if let Some((playing_idx, playing_id, playing_uri, playing_didl_id)) = playing_info {
            let new_playing_idx = items
                .iter()
                .position(|item| item.uri == playing_uri)
                .or_else(|| {
                    items
                        .iter()
                        .position(|item| item.didl_id == playing_didl_id)
                });

            tracing::trace!(
                renderer = self.renderer_id.0.as_str(),
                playing_uri = playing_uri.as_str(),
                playing_didl_id = ?playing_didl_id,
                pivot_found = new_playing_idx.is_some(),
                desired_uris = ?items.iter().map(|i| i.uri.as_str()).collect::<Vec<_>>(),
                "sync_queue: pivot search result"
            );

            if let Some(pivot_idx) = new_playing_idx {
                debug!(
                    renderer = self.renderer_id.0.as_str(),
                    playing_idx,
                    pivot_idx,
                    "Gentle sync: currently playing item found in new playlist at index {}",
                    pivot_idx
                );

                let current_ids_for_pivot: Vec<u32> =
                    snapshot.items.iter().map(|i| i.backend_id as u32).collect();
                self.replace_queue_with_pivot(
                    items,
                    pivot_idx,
                    playing_id,
                    &snapshot,
                    &current_ids_for_pivot,
                    cancel_token,
                    &mut on_ready,
                )?;
            } else {
                debug!(
                    renderer = self.renderer_id.0.as_str(),
                    playing_idx,
                    "Gentle sync: currently playing item not in new playlist, preserving as first item"
                );

                self.replace_queue_preserve_current(
                    items,
                    playing_id,
                    cancel_token,
                    &mut on_ready,
                )?;
            }
        } else {
            if snapshot.items.is_empty() && !items.is_empty() {
                tracing::warn!(
                    renderer = self.renderer_id.0.as_str(),
                    snapshot_items = snapshot.items.len(),
                    new_items = items.len(),
                    "OpenHome playlist appears empty - possible stale cache or device issue, NOT clearing queue"
                );
                return self.enqueue_items(items, crate::queue::EnqueueMode::AppendToEnd);
            }

            debug!(
                renderer = self.renderer_id.0.as_str(),
                "No currently playing item, using standard LCS sync"
            );
            let current_ids_for_lcs: Vec<u32> =
                snapshot.items.iter().map(|i| i.backend_id as u32).collect();
            self.replace_queue_standard_lcs(
                items,
                &snapshot,
                &current_ids_for_lcs,
                cancel_token,
                &mut on_ready,
            )?;
        }

        let post_current_track = self.playlist_client.id().ok();
        let post_ids = self.track_ids();
        tracing::warn!(
            renderer = self.renderer_id.0.as_str(),
            post_current_track_id = post_current_track,
            post_items_count = post_ids.as_ref().map(|v| v.len()).unwrap_or(0),
            "sync_queue: END - current track after modifications"
        );

        Ok(())
    }

    fn get_item(&self, index: usize) -> Result<Option<PlaybackItem>, ControlPointError> {
        let snapshot = self.queue_snapshot()?;

        if index < snapshot.items.len() {
            return Ok(Some(snapshot.items[index].clone()));
        }

        Err(ControlPointError::QueueError(format!(
            "get_item index out of bound {} >= {}",
            index,
            snapshot.items.len()
        )))
    }

    fn replace_item(&mut self, index: usize, item: PlaybackItem) -> Result<(), ControlPointError> {
        let metadata = build_metadata_xml(&item);

        let ids = self.track_ids()?;

        if index >= ids.len() {
            return Err(ControlPointError::QueueError(format!(
                "get_item index out of bound {} >= {}",
                index,
                ids.len()
            )));
        }

        self.ensure_playlist_source_selected()?;

        let track_id = ids[index];
        let before_id = if index == 0 {
            OPENHOME_PLAYLIST_HEAD_ID
        } else {
            ids[index - 1]
        };

        let ci = self.current_index()?;
        // Use delete_id_if_exists() to handle cases where another control point
        // may have already modified the playlist
        self.playlist_client.delete_id(track_id)?;
        let new_id = self
            .playlist_client
            .insert(before_id, &item.uri, &metadata)?;

        // Mettre à jour le cache avec les nouvelles métadonnées
        self.metadata_cache.lock().unwrap().remove(&track_id);
        self.uri_by_id.lock().unwrap().remove(&track_id);
        self.cache_metadata(new_id, item.metadata, &item.uri);

        if ci == Some(index) {
            self.playlist_client.seek_id(new_id)?;
        }

        // Invalidate cache after playlist modifications
        self.invalidate_track_caches();

        Ok(())
    }

    /// Override enqueue_items to add items directly to the OpenHome playlist.
    fn enqueue_items(
        &mut self,
        items: Vec<PlaybackItem>,
        mode: EnqueueMode,
    ) -> Result<(), ControlPointError> {
        if items.is_empty() {
            return Ok(());
        }

        match mode {
            EnqueueMode::AppendToEnd => {
                let ids = self.track_ids()?;
                let mut after_id = if ids.len() > 0 {
                    ids[ids.len() - 1]
                } else {
                    OPENHOME_PLAYLIST_HEAD_ID
                };
                // Append to the end of the OpenHome playlist
                for item in items {
                    after_id = self.add_playback_item(item, after_id)?;
                }
            }
            EnqueueMode::InsertAfterCurrent => {
                if let Some(mut after_id) = self.current_track()? {
                    for item in items {
                        after_id = self.add_playback_item(item, after_id)?;
                    }
                } else {
                    self.enqueue_items(items, EnqueueMode::AppendToEnd)?;
                }
            }
            EnqueueMode::ReplaceAll => {
                // Replace the entire playlist
                self.replace_queue(items, None)?;
                return Ok(());
            }
        }

        // Invalidate cache after playlist modifications (except ReplaceAll which already does it)
        self.invalidate_track_caches();

        Ok(())
    }

    // Optimized helpers to avoid unnecessary network calls

    /// Optimized clear_queue: use delete_all() directly instead of replace_queue.
    fn clear_queue(&mut self) -> Result<(), ControlPointError> {
        self.ensure_playlist_source_selected()?;
        self.playlist_client.delete_all()?;
        // Invalidate caches after clearing playlist (clears queue and current track)
        self.invalidate_all_caches();
        Ok(())
    }

    /// Optimized is_empty: only fetch track IDs, not the full playlist.
    fn is_empty(&self) -> Result<bool, ControlPointError> {
        Ok(self.track_ids()?.is_empty())
    }

    /// Optimized upcoming_len: calculate from len() and current_index() without fetching items.
    fn upcoming_len(&self) -> Result<usize, ControlPointError> {
        let len = self.len()?;
        match self.current_index()? {
            None => Ok(len),
            Some(idx) => Ok(len.saturating_sub(idx + 1)),
        }
    }

    /// Optimized peek_current: use primitives instead of full snapshot.
    fn peek_current(&mut self) -> Result<Option<(PlaybackItem, usize)>, ControlPointError> {
        let len = self.len()?;
        if len == 0 {
            return Ok(None);
        }

        let current_idx = self.current_index()?;
        let resolved_index = match current_idx {
            Some(idx) if idx < len => Some(idx),
            _ => None,
        };

        let item_index = resolved_index.unwrap_or(0);
        let item = match self.get_item(item_index)? {
            Some(item) => item,
            None => return Ok(None),
        };

        let remaining = match resolved_index {
            Some(idx) => len.saturating_sub(idx + 1),
            None => len,
        };

        Ok(Some((item, remaining)))
    }

    /// Optimized dequeue_next: use primitives instead of full snapshot.
    fn dequeue_next(&mut self) -> Result<Option<(PlaybackItem, usize)>, ControlPointError> {
        let len = self.len()?;
        if len == 0 {
            return Ok(None);
        }

        let current_idx = self.current_index()?;
        let next_index = match current_idx {
            None => 0,
            Some(idx) => {
                let candidate = idx + 1;
                if candidate >= len {
                    return Ok(None);
                }
                candidate
            }
        };

        let Some(item) = self.get_item(next_index)? else {
            return Ok(None);
        };

        let remaining = len.saturating_sub(next_index + 1);
        self.set_index(Some(next_index))?;
        Ok(Some((item, remaining)))
    }

    /// Optimized append_or_init_index: use enqueue_items which is already optimized.
    fn append_or_init_index(&mut self, items: Vec<PlaybackItem>) -> Result<(), ControlPointError> {
        let was_empty = self.is_empty()?;

        // Use the already optimized enqueue_items(AppendToEnd)
        self.enqueue_items(items, EnqueueMode::AppendToEnd)?;

        // If the queue was empty before, set index to 0
        if was_empty && !self.is_empty()? {
            self.set_index(Some(0))?;
        }

        Ok(())
    }
}

impl QueueFromRendererInfo for OpenHomeQueue {
    fn from_renderer_info(renderer: &RendererInfo) -> Result<Self, ControlPointError> {
        OpenHomeQueue::from_renderer_info(renderer)
    }

    fn to_backend(self) -> MusicQueue {
        MusicQueue::from_openhome(self)
    }
}
