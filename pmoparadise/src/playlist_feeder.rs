//! RadioParadisePlaylistFeeder - Télécharge et alimente une playlist à partir des blocs RP
//!
//! Architecture simplifiée utilisant les URLs gapless individuelles au lieu du bloc FLAC entier.

use crate::{client::RadioParadiseClient, models::EventId};
use anyhow::Result;
use pmoaudiocache::Cache as AudioCache;
use pmocovers::Cache as CoversCache;
use pmoplaylist::{PlaylistManager, PlaylistRole, ReadHandle, WriteHandle};
use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::sync::Notify;

/// Signal de fin de blocs
pub const END_OF_BLOCKS_SIGNAL: EventId = EventId::MAX;
const RECENT_BLOCKS_CACHE_SIZE: usize = 10;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BlockStatus {
    Pending,
    InProgress,
    Done,
}

struct RecentBlocks {
    states: HashMap<EventId, BlockStatus>,
    order: VecDeque<EventId>,
    capacity: usize,
}

impl RecentBlocks {
    fn new(capacity: usize) -> Self {
        Self {
            states: HashMap::new(),
            order: VecDeque::new(),
            capacity,
        }
    }

    fn try_enqueue(&mut self, event_id: EventId) -> bool {
        match self.states.get(&event_id) {
            Some(_) => false,
            None => {
                self.order.push_back(event_id);
                self.states.insert(event_id, BlockStatus::Pending);
                self.evict_old_done();
                true
            }
        }
    }

    fn mark_in_progress(&mut self, event_id: EventId) {
        if let Some(state) = self.states.get_mut(&event_id) {
            *state = BlockStatus::InProgress;
        } else {
            self.order.push_back(event_id);
            self.states.insert(event_id, BlockStatus::InProgress);
        }
        self.evict_old_done();
    }

    fn mark_done(&mut self, event_id: EventId) {
        if let Some(state) = self.states.get_mut(&event_id) {
            *state = BlockStatus::Done;
        } else {
            self.order.push_back(event_id);
            self.states.insert(event_id, BlockStatus::Done);
        }
        self.evict_old_done();
    }

    fn purge(&mut self, event_id: EventId) {
        self.states.remove(&event_id);
    }

    fn evict_old_done(&mut self) {
        while self.order.len() > self.capacity {
            let Some(front) = self.order.front().copied() else {
                break;
            };
            match self.states.get(&front) {
                Some(BlockStatus::Done) | None => {
                    self.order.pop_front();
                    self.states.remove(&front);
                }
                Some(_) => break,
            }
        }
    }
}

/// Feeder qui télécharge les blocs RP et alimente une playlist
pub struct RadioParadisePlaylistFeeder {
    client: RadioParadiseClient,
    audio_cache: Arc<AudioCache>,
    covers_cache: Arc<CoversCache>,
    playlist_handle: Arc<WriteHandle>,
    block_queue: Arc<tokio::sync::Mutex<VecDeque<EventId>>>,
    notify: Arc<Notify>,
    collection: Option<String>,
    recent_blocks: tokio::sync::Mutex<RecentBlocks>,
}

impl RadioParadisePlaylistFeeder {
    /// Crée un nouveau feeder et retourne (feeder, read_handle)
    pub async fn new(
        client: RadioParadiseClient,
        audio_cache: Arc<AudioCache>,
        covers_cache: Arc<CoversCache>,
        playlist_id: String,
        collection: Option<String>,
    ) -> Result<(Self, ReadHandle)> {
        let manager = PlaylistManager::get();
        let write_handle = manager
            .create_persistent_playlist_with_role(playlist_id.clone(), PlaylistRole::Radio)
            .await?;
        let read_handle = manager.get_read_handle(&playlist_id).await?;

        Ok((
            Self {
                client,
                audio_cache,
                covers_cache,
                playlist_handle: Arc::new(write_handle),
                block_queue: Arc::new(tokio::sync::Mutex::new(VecDeque::new())),
                notify: Arc::new(Notify::new()),
                collection,
                recent_blocks: tokio::sync::Mutex::new(RecentBlocks::new(RECENT_BLOCKS_CACHE_SIZE)),
            },
            read_handle,
        ))
    }

    /// Enqueue un bloc pour traitement
    pub async fn push_block_id(&self, event_id: EventId) {
        {
            let mut recent = self.recent_blocks.lock().await;
            if !recent.try_enqueue(event_id) {
                tracing::debug!(
                    "RadioParadisePlaylistFeeder: Ignoring duplicate enqueue for block {}",
                    event_id
                );
                return;
            }
        }

        {
            let mut queue = self.block_queue.lock().await;
            queue.push_back(event_id);
        }
        self.notify.notify_one();
    }

    async fn mark_in_progress(&self, event_id: EventId) {
        let mut recent = self.recent_blocks.lock().await;
        recent.mark_in_progress(event_id);
    }

    async fn mark_done(&self, event_id: EventId) {
        let mut recent = self.recent_blocks.lock().await;
        recent.mark_done(event_id);
    }

    async fn purge_block_state(&self, event_id: EventId) {
        let mut recent = self.recent_blocks.lock().await;
        recent.purge(event_id);
    }

    pub(crate) async fn retry_block(&self, event_id: EventId) {
        self.purge_block_state(event_id).await;
        self.push_block_id(event_id).await;
    }

    /// Boucle principale de traitement (à exécuter dans une tâche tokio)
    pub async fn run(self: Arc<Self>) -> Result<()> {
        loop {
            // Attendre un bloc
            let event_id = loop {
                {
                    let mut queue = self.block_queue.lock().await;
                    if let Some(id) = queue.pop_front() {
                        if id == END_OF_BLOCKS_SIGNAL {
                            tracing::info!(
                                "RadioParadisePlaylistFeeder: END_OF_BLOCKS_SIGNAL received"
                            );
                            return Ok(());
                        }
                        break id;
                    }
                }
                self.notify.notified().await;
            };

            self.mark_in_progress(event_id).await;

            // Traiter le bloc
            if let Err(e) = self.process_block(event_id).await {
                tracing::error!(
                    "RadioParadisePlaylistFeeder: Failed to process block {}: {}",
                    event_id,
                    e
                );
                self.purge_block_state(event_id).await;
                tracing::debug!(
                    "RadioParadisePlaylistFeeder: Cleared block {} state after error",
                    event_id
                );
            } else {
                self.mark_done(event_id).await;
            }
        }
    }

    /// Traite un bloc : fetch, filtre, download, push playlist
    async fn process_block(&self, event_id: EventId) -> Result<()> {
        tracing::info!("RadioParadisePlaylistFeeder: Processing block {}", event_id);

        // 1. Fetch le bloc
        let block = self.client.get_block(Some(event_id)).await?;

        // 2. Timestamp actuel
        let now_ms = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as u64;

        // 3. Filtrer les chansons encore en lecture ou à venir
        let songs = block.songs_ordered();
        let mut processed = 0;

        for (idx, song) in songs {
            if !song.is_still_playing(now_ms) {
                tracing::debug!(
                    "RadioParadisePlaylistFeeder: Skipping finished song {} - {} (ended at {})",
                    idx,
                    song.title,
                    song.sched_end_time_ms().unwrap_or(0)
                );
                continue;
            }

            // 4. Télécharger la chanson
            let gapless_url = song
                .gapless_url
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("Missing gapless_url for song {}", idx))?;

            tracing::info!(
                "RadioParadisePlaylistFeeder: Downloading song {} - {} by {}",
                idx,
                song.title,
                song.artist
            );

            let pk = self
                .audio_cache
                .add_from_url(gapless_url, self.collection.as_deref())
                .await?;

            // 5. Sauvegarder les métadonnées
            self.save_metadata(&pk, song, &block).await?;

            // 6. Calculer le TTL
            let sched_end = song
                .sched_end_time_ms()
                .ok_or_else(|| anyhow::anyhow!("Cannot calculate TTL without sched_time_millis"))?;
            let ttl_ms = sched_end.saturating_sub(now_ms);
            let ttl = Duration::from_millis(ttl_ms);

            // 7. Push dans la playlist avec TTL
            self.playlist_handle.push_with_ttl(pk.clone(), ttl).await?;

            tracing::info!(
                "RadioParadisePlaylistFeeder: Added {} to playlist (pk={}, ttl={}s)",
                song.title,
                pk,
                ttl.as_secs()
            );

            processed += 1;
        }

        tracing::info!(
            "RadioParadisePlaylistFeeder: Processed block {} - added {} songs to playlist",
            event_id,
            processed
        );

        Ok(())
    }

    /// Sauvegarde les métadonnées dans le cache audio
    async fn save_metadata(
        &self,
        pk: &str,
        song: &crate::models::Song,
        block: &crate::models::Block,
    ) -> Result<()> {
        use pmoaudiocache::AudioTrackMetadataExt;

        let metadata = self.audio_cache.track_metadata(pk);
        let mut meta = metadata.write().await;

        // Métadonnées de base
        meta.set_title(Some(song.title.clone())).await?;
        meta.set_artist(Some(song.artist.clone())).await?;
        if let Some(ref album) = song.album {
            meta.set_album(Some(album.clone())).await?;
        }
        if let Some(year) = song.year {
            meta.set_year(Some(year)).await?;
        }

        // Cover
        if let Some(ref cover_large) = song.cover_large {
            if let Some(cover_url) = block.cover_url(cover_large) {
                meta.set_cover_url(Some(cover_url.clone())).await?;

                // Télécharger la cover
                match self
                    .covers_cache
                    .add_from_url(&cover_url, self.collection.as_deref())
                    .await
                {
                    Ok(cover_pk) => {
                        meta.set_cover_pk(Some(cover_pk)).await?;
                        tracing::debug!(
                            "RadioParadisePlaylistFeeder: Cached cover for {}",
                            song.title
                        );
                    }
                    Err(e) => {
                        tracing::warn!("RadioParadisePlaylistFeeder: Failed to cache cover: {}", e);
                    }
                }
            }
        }

        Ok(())
    }
}
