//! Music source implementation for Radio Paradise built on the new
//! `paradise` orchestration layer.
//!
//! The source exposes a DIDL-Lite hierarchy compatible with UPnP
//! ContentDirectory while delegating block ingestion, caching and
//! multi-client streaming to [`ParadiseChannel`].

use crate::client::RadioParadiseClient;
use crate::paradise::{
    history_backend_from_config, ChannelDescriptor, MemoryHistoryBackend, ParadiseChannel,
    PlaylistEntry, RadioParadiseConfig, ALL_CHANNELS,
};
use anyhow::Result as AnyhowResult;
use pmoaudiocache::Cache as AudioCache;
use pmocovers::Cache as CoverCache;
use pmodidl::{Container, Item, Resource};
use pmosource::pmodidl;
use pmosource::{
    async_trait, BrowseResult, CacheStatus, MusicSource, MusicSourceError, Result,
    SourceCacheManager, SourceStatistics,
};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::SystemTime;
use tracing::warn;

/// Default image for Radio Paradise (300x300 WebP, embedded in binary)
const DEFAULT_IMAGE: &[u8] = include_bytes!("../assets/default.webp");

fn channel_collection_id(channel_id: u8) -> String {
    format!("radio-paradise:{}", channel_id)
}

fn channel_container_id(channel_id: u8) -> String {
    format!("radio-paradise:channel:{}", channel_id)
}

fn parse_channel_container_id(object_id: &str) -> Option<u8> {
    let mut parts = object_id.split(':');
    match (parts.next(), parts.next(), parts.next(), parts.next()) {
        (Some("radio-paradise"), Some("channel"), Some(id_str), None) => id_str.parse().ok(),
        _ => None,
    }
}

fn parse_track_channel(track_id: &str) -> Option<u8> {
    let mut parts = track_id.split(':');
    match (parts.next(), parts.next(), parts.next(), parts.next()) {
        (Some("rp"), Some(channel_str), Some(_rest), None) => channel_str.parse().ok(),
        _ => None,
    }
}

fn format_duration(duration_seconds: u64) -> String {
    let hours = duration_seconds / 3600;
    let minutes = (duration_seconds % 3600) / 60;
    let seconds = duration_seconds % 60;
    format!("{hours}:{minutes:02}:{seconds:02}")
}

#[derive(Clone)]
pub struct RadioParadiseSource {
    inner: Arc<RadioParadiseSourceInner>,
}

struct RadioParadiseSourceInner {
    channels: HashMap<u8, Arc<ParadiseChannel>>,
}

impl std::fmt::Debug for RadioParadiseSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RadioParadiseSource").finish()
    }
}

impl RadioParadiseSource {
    #[cfg(feature = "server")]
    pub fn from_registry(client: RadioParadiseClient) -> Result<Self> {
        let config = Arc::new(RadioParadiseConfig::load_from_pmoconfig().unwrap_or_default());
        let history_backend = history_backend_from_config(&config.history).map_err(|e| {
            MusicSourceError::SourceUnavailable(format!(
                "Failed to initialize history backend: {}",
                e
            ))
        })?;
        let mut channels = HashMap::new();

        for descriptor in ALL_CHANNELS.iter() {
            let cache_manager = Arc::new(SourceCacheManager::from_registry(
                channel_collection_id(descriptor.id),
            )?);
            let channel = Arc::new(
                ParadiseChannel::new(
                    *descriptor,
                    client.clone(),
                    config.clone(),
                    history_backend.clone(),
                    cache_manager,
                )
                .map_err(|e| {
                    MusicSourceError::SourceUnavailable(format!(
                        "Failed to initialize channel {}: {e}",
                        descriptor.slug
                    ))
                })?,
            );
            channels.insert(descriptor.id, channel);
        }

        Ok(Self {
            inner: Arc::new(RadioParadiseSourceInner { channels }),
        })
    }

    #[cfg(feature = "server")]
    pub fn from_registry_default(client: RadioParadiseClient) -> Result<Self> {
        Self::from_registry(client)
    }

    pub fn new(
        client: RadioParadiseClient,
        cover_cache: Arc<CoverCache>,
        audio_cache: Arc<AudioCache>,
    ) -> Self {
        let config = Arc::new(RadioParadiseConfig::load_from_pmoconfig().unwrap_or_default());
        let history_backend: Arc<dyn crate::paradise::HistoryBackend> =
            history_backend_from_config(&config.history).unwrap_or_else(|err| {
                warn!("Falling back to in-memory history backend: {err}");
                Arc::new(MemoryHistoryBackend::new()) as Arc<dyn crate::paradise::HistoryBackend>
            });
        let mut channels = HashMap::new();

        for descriptor in ALL_CHANNELS.iter() {
            let cache_manager = Arc::new(SourceCacheManager::new(
                channel_collection_id(descriptor.id),
                Arc::clone(&cover_cache),
                Arc::clone(&audio_cache),
            ));
            match ParadiseChannel::new(
                *descriptor,
                client.clone(),
                config.clone(),
                history_backend.clone(),
                cache_manager,
            ) {
                Ok(channel) => {
                    channels.insert(descriptor.id, Arc::new(channel));
                }
                Err(err) => {
                    warn!(
                        channel = descriptor.slug,
                        "Failed to initialize channel: {err:?}"
                    );
                }
            }
        }

        Self {
            inner: Arc::new(RadioParadiseSourceInner { channels }),
        }
    }

    pub fn new_default(
        client: RadioParadiseClient,
        cover_cache: Arc<CoverCache>,
        audio_cache: Arc<AudioCache>,
    ) -> Self {
        Self::new(client, cover_cache, audio_cache)
    }

    pub fn client_for_channel(&self, channel: u8) -> Option<RadioParadiseClient> {
        self.inner
            .channels
            .get(&channel)
            .map(|ch| ch.client().clone())
    }

    pub fn channel(&self, id: u8) -> Option<Arc<ParadiseChannel>> {
        self.inner.channels.get(&id).cloned()
    }

    fn build_root_container(&self) -> Container {
        Container {
            id: "radio-paradise".to_string(),
            parent_id: "0".to_string(),
            restricted: Some("1".to_string()),
            child_count: Some(ALL_CHANNELS.len().to_string()),
            title: "Radio Paradise".to_string(),
            class: "object.container".to_string(),
            containers: vec![],
            items: vec![],
        }
    }

    async fn build_channel_containers(&self) -> Vec<Container> {
        let mut containers = Vec::new();
        for descriptor in ALL_CHANNELS.iter() {
            if let Some(channel) = self.channel(descriptor.id) {
                let len = channel.playlist().active_len().await;
                containers.push(Container {
                    id: channel_container_id(descriptor.id),
                    parent_id: "radio-paradise".to_string(),
                    restricted: Some("1".to_string()),
                    child_count: Some(len.to_string()),
                    title: descriptor.display_name.to_string(),
                    class: "object.container.playlistContainer".to_string(),
                    containers: vec![],
                    items: vec![],
                });
            }
        }
        containers
    }

    async fn channel_items(
        &self,
        descriptor: ChannelDescriptor,
        offset: usize,
        limit: Option<usize>,
    ) -> Result<Vec<Item>> {
        let channel = self
            .channel(descriptor.id)
            .ok_or_else(|| MusicSourceError::ObjectNotFound(descriptor.slug.to_string()))?;

        channel
            .ensure_started()
            .await
            .map_err(|e| MusicSourceError::SourceUnavailable(e.to_string()))?;

        let entries = channel.playlist().active_snapshot().await;
        if entries.is_empty() || offset >= entries.len() {
            return Ok(Vec::new());
        }

        let end = limit
            .map(|count| offset + count)
            .unwrap_or(entries.len())
            .min(entries.len());

        let parent_id = channel_container_id(descriptor.id);

        let mut items = Vec::with_capacity(end - offset);
        for entry in entries.into_iter().skip(offset).take(end - offset) {
            match self.entry_to_item(channel.clone(), &parent_id, entry).await {
                Ok(item) => items.push(item),
                Err(err) => warn!(
                    channel = descriptor.slug,
                    "Failed to build DIDL item: {err:?}"
                ),
            }
        }
        Ok(items)
    }

    async fn entry_to_item(
        &self,
        channel: Arc<ParadiseChannel>,
        parent_id: &str,
        entry: Arc<PlaylistEntry>,
    ) -> AnyhowResult<Item> {
        let cache_manager = channel.cache_manager();
        let metadata = cache_manager.get_metadata(&entry.track_id).await;

        let resource_url = cache_manager
            .resolve_uri(&entry.track_id)
            .await
            .or_else(|_| {
                metadata
                    .as_ref()
                    .map(|meta| meta.original_uri.clone())
                    .ok_or_else(|| MusicSourceError::ObjectNotFound(entry.track_id.clone()))
            })?;

        let mut album_art = metadata
            .as_ref()
            .and_then(|meta| meta.cached_cover_pk.as_ref())
            .and_then(|pk| cache_manager.cover_url(pk, None).ok());

        if album_art.is_none() {
            album_art = entry.song.cover.clone();
        }

        let duration_seconds = entry.duration_ms / 1000;
        let duration_str = if duration_seconds > 0 {
            Some(format_duration(duration_seconds as u64))
        } else {
            None
        };

        let resource = Resource {
            protocol_info: "http-get:*:audio/flac:*".to_string(),
            bits_per_sample: None,
            sample_frequency: None,
            nr_audio_channels: None,
            duration: duration_str.clone(),
            url: resource_url,
        };

        Ok(Item {
            id: entry.track_id.clone(),
            parent_id: parent_id.to_string(),
            restricted: Some("1".to_string()),
            title: entry.song.title.clone(),
            creator: Some(entry.song.artist.clone()),
            class: "object.item.audioItem.musicTrack".to_string(),
            artist: Some(entry.song.artist.clone()),
            album: entry.song.album.clone(),
            genre: None,
            album_art,
            album_art_pk: None,
            date: None,
            original_track_number: None,
            resources: vec![resource],
            descriptions: vec![],
        })
    }

    fn channels_iter(&self) -> impl Iterator<Item = (&u8, &Arc<ParadiseChannel>)> {
        self.inner.channels.iter()
    }
}

#[async_trait]
impl MusicSource for RadioParadiseSource {
    fn name(&self) -> &str {
        "Radio Paradise"
    }

    fn id(&self) -> &str {
        "radio-paradise"
    }

    fn default_image(&self) -> &[u8] {
        DEFAULT_IMAGE
    }

    async fn root_container(&self) -> Result<Container> {
        Ok(self.build_root_container())
    }

    async fn browse(&self, object_id: &str) -> Result<BrowseResult> {
        match object_id {
            "0" => Ok(BrowseResult::Containers(vec![self.build_root_container()])),
            "radio-paradise" => {
                let containers = self.build_channel_containers().await;
                Ok(BrowseResult::Containers(containers))
            }
            _ => {
                if let Some(channel_id) = parse_channel_container_id(object_id) {
                    let descriptor = ALL_CHANNELS
                        .iter()
                        .find(|desc| desc.id == channel_id)
                        .ok_or_else(|| MusicSourceError::ObjectNotFound(object_id.to_string()))?;

                    let items = self.channel_items(*descriptor, 0, None).await?;
                    Ok(BrowseResult::Items(items))
                } else {
                    Err(MusicSourceError::ObjectNotFound(object_id.to_string()))
                }
            }
        }
    }

    async fn resolve_uri(&self, object_id: &str) -> Result<String> {
        let channel_id = parse_track_channel(object_id)
            .ok_or_else(|| MusicSourceError::ObjectNotFound(object_id.to_string()))?;
        let channel = self
            .channel(channel_id)
            .ok_or_else(|| MusicSourceError::ObjectNotFound(object_id.to_string()))?;
        channel
            .cache_manager()
            .resolve_uri(object_id)
            .await
            .map_err(|e| MusicSourceError::CacheError(e.to_string()))
    }

    fn supports_fifo(&self) -> bool {
        false
    }

    async fn append_track(&self, _track: Item) -> Result<()> {
        Err(MusicSourceError::FifoNotSupported)
    }

    async fn remove_oldest(&self) -> Result<Option<Item>> {
        Err(MusicSourceError::FifoNotSupported)
    }

    async fn update_id(&self) -> u32 {
        self.channels_iter()
            .map(|(_, channel)| channel.playlist().update_id())
            .max()
            .unwrap_or(0)
    }

    async fn last_change(&self) -> Option<SystemTime> {
        let mut latest: Option<SystemTime> = None;
        for (_, channel) in self.channels_iter() {
            if let Some(change) = channel.playlist().last_change().await {
                latest = Some(match latest {
                    Some(current) if change <= current => current,
                    _ => change,
                });
            }
        }
        latest
    }

    async fn get_items(&self, offset: usize, count: usize) -> Result<Vec<Item>> {
        let mut all = Vec::new();
        for descriptor in ALL_CHANNELS.iter() {
            let mut items = self.channel_items(*descriptor, 0, None).await?;
            all.append(&mut items);
        }

        if offset >= all.len() {
            return Ok(Vec::new());
        }

        let end = if count == 0 {
            all.len()
        } else {
            (offset + count).min(all.len())
        };

        Ok(all.into_iter().skip(offset).take(end - offset).collect())
    }

    async fn get_available_formats(&self, _object_id: &str) -> Result<Vec<pmosource::AudioFormat>> {
        Ok(vec![pmosource::AudioFormat {
            format_id: "flac".to_string(),
            mime_type: "audio/flac".to_string(),
            sample_rate: Some(44100),
            bit_depth: Some(16),
            bitrate: None,
            channels: Some(2),
        }])
    }

    async fn get_cache_status(&self, object_id: &str) -> Result<CacheStatus> {
        let channel_id = parse_track_channel(object_id)
            .ok_or_else(|| MusicSourceError::ObjectNotFound(object_id.to_string()))?;
        let channel = self
            .channel(channel_id)
            .ok_or_else(|| MusicSourceError::ObjectNotFound(object_id.to_string()))?;
        channel
            .cache_manager()
            .get_cache_status(object_id)
            .await
            .map_err(|e| MusicSourceError::CacheError(e.to_string()))
    }

    async fn cache_item(&self, object_id: &str) -> Result<CacheStatus> {
        let channel_id = parse_track_channel(object_id)
            .ok_or_else(|| MusicSourceError::ObjectNotFound(object_id.to_string()))?;
        let channel = self
            .channel(channel_id)
            .ok_or_else(|| MusicSourceError::ObjectNotFound(object_id.to_string()))?;
        channel
            .cache_manager()
            .get_cache_status(object_id)
            .await
            .map_err(|e| MusicSourceError::CacheError(e.to_string()))
    }

    async fn browse_paginated(
        &self,
        object_id: &str,
        offset: usize,
        limit: usize,
    ) -> Result<BrowseResult> {
        match object_id {
            "0" => {
                if offset == 0 {
                    Ok(BrowseResult::Containers(vec![self.build_root_container()]))
                } else {
                    Ok(BrowseResult::Containers(Vec::new()))
                }
            }
            "radio-paradise" => {
                let containers = self.build_channel_containers().await;
                let total = containers.len();
                if offset >= total {
                    return Ok(BrowseResult::Containers(Vec::new()));
                }
                let end = if limit == 0 {
                    total
                } else {
                    (offset + limit).min(total)
                };
                Ok(BrowseResult::Containers(
                    containers
                        .into_iter()
                        .skip(offset)
                        .take(end - offset)
                        .collect(),
                ))
            }
            _ => {
                if let Some(channel_id) = parse_channel_container_id(object_id) {
                    let descriptor = ALL_CHANNELS
                        .iter()
                        .find(|desc| desc.id == channel_id)
                        .ok_or_else(|| MusicSourceError::ObjectNotFound(object_id.to_string()))?;

                    let items = self.channel_items(*descriptor, offset, Some(limit)).await?;
                    Ok(BrowseResult::Items(items))
                } else {
                    Err(MusicSourceError::ObjectNotFound(object_id.to_string()))
                }
            }
        }
    }

    async fn get_item_count(&self, object_id: &str) -> Result<usize> {
        match object_id {
            "0" => Ok(1),
            "radio-paradise" => Ok(ALL_CHANNELS.len()),
            _ => {
                if let Some(channel_id) = parse_channel_container_id(object_id) {
                    let channel = self
                        .channel(channel_id)
                        .ok_or_else(|| MusicSourceError::ObjectNotFound(object_id.to_string()))?;
                    Ok(channel.playlist().active_len().await)
                } else {
                    Err(MusicSourceError::ObjectNotFound(object_id.to_string()))
                }
            }
        }
    }

    async fn statistics(&self) -> Result<SourceStatistics> {
        let mut total_tracks = 0usize;
        let mut cached_tracks = 0usize;

        for (_, channel) in self.channels_iter() {
            total_tracks += channel.playlist().active_len().await;
            let stats = channel.cache_manager().statistics().await;
            cached_tracks += stats.cached_tracks;
        }

        Ok(SourceStatistics {
            total_items: Some(total_tracks),
            total_containers: Some(ALL_CHANNELS.len() + 1),
            cached_items: Some(cached_tracks),
            cache_size_bytes: None,
        })
    }
}
