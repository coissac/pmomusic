//! Music source implementation for Radio Paradise
//!
//! This module implements the [`pmosource::MusicSource`] trait for Radio Paradise,
//! providing a complete music source with FIFO playlist support, browsing, and caching.

use crate::client::RadioParadiseClient;
use crate::models::{Block, Song};
use pmosource::{async_trait, pmodidl, BrowseResult, MusicSource, MusicSourceError, Result};
use pmodidl::{Container, Item, Resource};
use pmoplaylist::{FifoPlaylist, Track};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::SystemTime;
use tokio::sync::RwLock;

#[cfg(feature = "cache")]
use pmocovers::Cache as CoverCache;
#[cfg(feature = "cache")]
use pmoaudiocache::{AudioCache, AudioMetadata};

/// Default image for Radio Paradise (300x300 WebP, embedded in binary)
const DEFAULT_IMAGE: &[u8] = include_bytes!("../assets/default.webp");

/// Default FIFO capacity (number of recent tracks to keep)
const DEFAULT_FIFO_CAPACITY: usize = 50;

/// Radio Paradise music source with full MusicSource trait implementation
///
/// This struct combines a [`RadioParadiseClient`] for API access with a FIFO playlist
/// for dynamic track management, implementing the complete [`MusicSource`] trait.
///
/// # Features
///
/// - **FIFO Playlist**: Dynamic track management with configurable capacity
/// - **API Integration**: Fetches blocks and metadata from Radio Paradise
/// - **URI Resolution**: Resolves track URIs with optional cache support
/// - **Change Tracking**: Tracks update_id and last_change for UPnP notifications
/// - **DIDL-Lite Export**: Converts tracks and blocks to UPnP-compatible formats
///
/// # Examples
///
/// ```no_run
/// use pmoparadise::{RadioParadiseSource, RadioParadiseClient};
/// use pmosource::MusicSource;
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let client = RadioParadiseClient::new().await?;
///     let source = RadioParadiseSource::new(client, "http://localhost:8080", 50);
///
///     println!("Source: {}", source.name());
///     println!("Supports FIFO: {}", source.supports_fifo());
///
///     // Start streaming and the FIFO will be populated
///     Ok(())
/// }
/// ```
#[derive(Clone)]
pub struct RadioParadiseSource {
    inner: Arc<RadioParadiseSourceInner>,
}

struct RadioParadiseSourceInner {
    /// Radio Paradise API client
    client: RadioParadiseClient,

    /// FIFO playlist for dynamic track management
    playlist: FifoPlaylist,

    /// Cache server base URL for URI resolution
    cache_base_url: String,

    /// Track metadata cache (track_id -> (original_uri, cached_pk, block_event))
    track_cache: RwLock<HashMap<String, TrackMetadata>>,

    /// Cover image cache (optional)
    #[cfg(feature = "cache")]
    cover_cache: Option<Arc<CoverCache>>,

    /// Audio cache (optional)
    #[cfg(feature = "cache")]
    audio_cache: Option<Arc<AudioCache>>,
}

#[derive(Debug, Clone)]
struct TrackMetadata {
    original_uri: String,
    cached_pk: Option<String>,
    block: Arc<Block>,
    song_index: usize,
    #[cfg(feature = "cache")]
    cached_audio_pk: Option<String>,
    #[cfg(feature = "cache")]
    cached_cover_pk: Option<String>,
}

impl std::fmt::Debug for RadioParadiseSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RadioParadiseSource")
            .field("cache_base_url", &self.inner.cache_base_url)
            .finish()
    }
}

impl RadioParadiseSource {
    /// Create a new Radio Paradise source
    ///
    /// # Arguments
    ///
    /// * `client` - Radio Paradise API client
    /// * `cache_base_url` - Base URL for the cache server (e.g., "http://localhost:8080")
    /// * `fifo_capacity` - Maximum number of tracks in the FIFO
    pub fn new(
        client: RadioParadiseClient,
        cache_base_url: impl Into<String>,
        fifo_capacity: usize,
    ) -> Self {
        let playlist = FifoPlaylist::new(
            "radio-paradise".to_string(),
            "Radio Paradise".to_string(),
            fifo_capacity,
            DEFAULT_IMAGE,
        );

        Self {
            inner: Arc::new(RadioParadiseSourceInner {
                client,
                playlist,
                cache_base_url: cache_base_url.into(),
                track_cache: RwLock::new(HashMap::new()),
                #[cfg(feature = "cache")]
                cover_cache: None,
                #[cfg(feature = "cache")]
                audio_cache: None,
            }),
        }
    }

    /// Create with default FIFO capacity
    pub fn new_default(client: RadioParadiseClient, cache_base_url: impl Into<String>) -> Self {
        Self::new(client, cache_base_url, DEFAULT_FIFO_CAPACITY)
    }

    /// Create a new Radio Paradise source with caching support
    ///
    /// # Arguments
    ///
    /// * `client` - Radio Paradise API client
    /// * `cache_base_url` - Base URL for the cache server (e.g., "http://localhost:8080")
    /// * `fifo_capacity` - Maximum number of tracks in the FIFO
    /// * `cover_cache` - Optional cover image cache
    /// * `audio_cache` - Optional audio cache
    #[cfg(feature = "cache")]
    pub fn new_with_cache(
        client: RadioParadiseClient,
        cache_base_url: impl Into<String>,
        fifo_capacity: usize,
        cover_cache: Option<Arc<CoverCache>>,
        audio_cache: Option<Arc<AudioCache>>,
    ) -> Self {
        let playlist = FifoPlaylist::new(
            "radio-paradise".to_string(),
            "Radio Paradise".to_string(),
            fifo_capacity,
            DEFAULT_IMAGE,
        );

        Self {
            inner: Arc::new(RadioParadiseSourceInner {
                client,
                playlist,
                cache_base_url: cache_base_url.into(),
                track_cache: RwLock::new(HashMap::new()),
                cover_cache,
                audio_cache,
            }),
        }
    }

    /// Add a track from a Radio Paradise song and block
    ///
    /// This is the main way to populate the FIFO with tracks as they are
    /// received from the Radio Paradise API.
    pub async fn add_song(&self, block: Arc<Block>, song: &Song, song_index: usize) -> Result<()> {
        let track_id = format!("rp-{}-{}", block.event, song_index);

        // Create track for playlist
        let mut track = Track::new(track_id.clone(), song.title.clone(), block.url.clone());

        if !song.artist.is_empty() {
            track = track.with_artist(song.artist.clone());
        }

        if !song.album.is_empty() {
            track = track.with_album(song.album.clone());
        }

        if song.duration > 0 {
            track = track.with_duration((song.duration / 1000) as u32);
        }

        // Cache cover image and add to track
        #[cfg(feature = "cache")]
        let cached_cover_pk = if let Some(ref cover_cache) = self.inner.cover_cache {
            if let Some(ref image_base) = block.image_base {
                if let Some(ref cover) = song.cover {
                    let image_url = format!("{}{}", image_base, cover);

                    // Cache the cover image asynchronously
                    match cover_cache.add_from_url(&image_url).await {
                        Ok(pk) => {
                            // Use the cached cover URL
                            let cached_url = format!("{}/covers/images/{}", self.inner.cache_base_url, pk);
                            track = track.with_image(cached_url);
                            Some(pk)
                        }
                        Err(e) => {
                            tracing::warn!("Failed to cache cover image {}: {}", image_url, e);
                            // Fall back to original URL
                            track = track.with_image(image_url);
                            None
                        }
                    }
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        };

        // If no cache, add original cover image
        #[cfg(not(feature = "cache"))]
        if let Some(ref image_base) = block.image_base {
            if let Some(ref cover) = song.cover {
                let image_url = format!("{}{}", image_base, cover);
                track = track.with_image(image_url);
            }
        }

        // Cache audio asynchronously (in background)
        #[cfg(feature = "cache")]
        let cached_audio_pk = if let Some(ref audio_cache) = self.inner.audio_cache {
            // Prepare metadata for the audio cache
            let metadata = AudioMetadata {
                title: Some(song.title.clone()),
                artist: if !song.artist.is_empty() {
                    Some(song.artist.clone())
                } else {
                    None
                },
                album: if !song.album.is_empty() {
                    Some(song.album.clone())
                } else {
                    None
                },
                duration_secs: if song.duration > 0 {
                    Some((song.duration / 1000) as u64)
                } else {
                    None
                },
                year: None,
                track_number: None,
                track_total: None,
                disc_number: None,
                disc_total: None,
                genre: None,
                sample_rate: None,
                channels: None,
                bitrate: None,
            };

            // Cache the audio asynchronously
            match audio_cache.add_from_url(&block.url, Some(metadata)).await {
                Ok((pk, _)) => {
                    tracing::info!("Successfully cached audio for track {}: {}", track_id, pk);
                    Some(pk)
                }
                Err(e) => {
                    tracing::warn!("Failed to cache audio for track {}: {}", track_id, e);
                    None
                }
            }
        } else {
            None
        };

        // Store metadata
        {
            let mut cache = self.inner.track_cache.write().await;
            cache.insert(
                track_id.clone(),
                TrackMetadata {
                    original_uri: block.url.clone(),
                    cached_pk: None,
                    block: block.clone(),
                    song_index,
                    #[cfg(feature = "cache")]
                    cached_audio_pk,
                    #[cfg(feature = "cache")]
                    cached_cover_pk,
                },
            );
        }

        // Add to FIFO
        self.inner.playlist.append_track(track).await;

        Ok(())
    }

    /// Mark a track as cached
    ///
    /// Call this after successfully caching a track's audio via pmoaudiocache.
    pub async fn cache_track(&self, track_id: &str, cache_pk: String) -> Result<()> {
        let mut cache = self.inner.track_cache.write().await;

        if let Some(metadata) = cache.get_mut(track_id) {
            metadata.cached_pk = Some(cache_pk);
            Ok(())
        } else {
            Err(MusicSourceError::ObjectNotFound(track_id.to_string()))
        }
    }

    /// Convert a pmoplaylist::Track to pmodidl::Item
    fn track_to_item(&self, track: &Track) -> Item {
        let duration_str = track.duration.map(|d| {
            let hours = d / 3600;
            let minutes = (d % 3600) / 60;
            let seconds = d % 60;
            format!("{}:{:02}:{:02}", hours, minutes, seconds)
        });

        let resource = Resource {
            protocol_info: "http-get:*:audio/flac:*".to_string(),
            bits_per_sample: None,
            sample_frequency: None,
            nr_audio_channels: None,
            duration: duration_str,
            url: track.uri.clone(),
        };

        Item {
            id: track.id.clone(),
            parent_id: "radio-paradise".to_string(),
            restricted: Some("1".to_string()),
            title: track.title.clone(),
            creator: track.artist.clone(),
            class: "object.item.audioItem.musicTrack".to_string(),
            artist: track.artist.clone(),
            album: track.album.clone(),
            genre: None,
            album_art: track.image.clone(),
            album_art_pk: None,
            date: None,
            original_track_number: None,
            resources: vec![resource],
            descriptions: vec![],
        }
    }

    /// Get the Radio Paradise client
    pub fn client(&self) -> &RadioParadiseClient {
        &self.inner.client
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
        Ok(self.inner.playlist.as_container().await)
    }

    async fn browse(&self, object_id: &str) -> Result<BrowseResult> {
        // For Radio Paradise, browsing returns all tracks in the FIFO
        if object_id == "radio-paradise" || object_id == "0" {
            let tracks = self.inner.playlist.get_items(0, 1000).await;
            let items: Vec<Item> = tracks.iter().map(|t| self.track_to_item(t)).collect();
            Ok(BrowseResult::Items(items))
        } else {
            Err(MusicSourceError::ObjectNotFound(object_id.to_string()))
        }
    }

    async fn resolve_uri(&self, object_id: &str) -> Result<String> {
        let cache = self.inner.track_cache.read().await;

        if let Some(metadata) = cache.get(object_id) {
            // Priority 1: Use cached audio if available
            #[cfg(feature = "cache")]
            if let Some(ref pk) = metadata.cached_audio_pk {
                return Ok(format!("{}/audio/tracks/{}/stream", self.inner.cache_base_url, pk));
            }

            // Priority 2: Use legacy cached_pk (for backward compatibility)
            if let Some(ref pk) = metadata.cached_pk {
                return Ok(format!("{}/audio/cache/{}", self.inner.cache_base_url, pk));
            }

            // Priority 3: Return original block URI (not cached yet)
            Ok(metadata.original_uri.clone())
        } else {
            Err(MusicSourceError::ObjectNotFound(object_id.to_string()))
        }
    }

    fn supports_fifo(&self) -> bool {
        true
    }

    async fn append_track(&self, track: Item) -> Result<()> {
        // Convert Item back to Track
        let duration = track
            .resources
            .first()
            .and_then(|r| r.duration.as_ref())
            .and_then(|d| {
                let parts: Vec<&str> = d.split(':').collect();
                if parts.len() == 3 {
                    let h: u32 = parts[0].parse().ok()?;
                    let m: u32 = parts[1].parse().ok()?;
                    let s: u32 = parts[2].parse().ok()?;
                    Some(h * 3600 + m * 60 + s)
                } else {
                    None
                }
            });

        let uri = track
            .resources
            .first()
            .map(|r| r.url.clone())
            .unwrap_or_default();

        let mut pmo_track = Track::new(track.id.clone(), track.title.clone(), uri);

        if let Some(artist) = track.artist {
            pmo_track = pmo_track.with_artist(artist);
        }

        if let Some(album) = track.album {
            pmo_track = pmo_track.with_album(album);
        }

        if let Some(dur) = duration {
            pmo_track = pmo_track.with_duration(dur);
        }

        if let Some(img) = track.album_art {
            pmo_track = pmo_track.with_image(img);
        }

        self.inner.playlist.append_track(pmo_track).await;
        Ok(())
    }

    async fn remove_oldest(&self) -> Result<Option<Item>> {
        if let Some(track) = self.inner.playlist.remove_oldest().await {
            // Remove from cache
            {
                let mut cache = self.inner.track_cache.write().await;
                cache.remove(&track.id);
            }

            Ok(Some(self.track_to_item(&track)))
        } else {
            Ok(None)
        }
    }

    async fn update_id(&self) -> u32 {
        self.inner.playlist.update_id().await
    }

    async fn last_change(&self) -> Option<SystemTime> {
        Some(self.inner.playlist.last_change().await)
    }

    async fn get_items(&self, offset: usize, count: usize) -> Result<Vec<Item>> {
        let tracks = self.inner.playlist.get_items(offset, count).await;
        Ok(tracks.iter().map(|t| self.track_to_item(t)).collect())
    }

    async fn search(&self, _query: &str) -> Result<BrowseResult> {
        // Radio Paradise doesn't support search
        Err(MusicSourceError::SearchNotSupported)
    }

    // ============= Extended Features Implementation =============

    fn capabilities(&self) -> pmosource::SourceCapabilities {
        pmosource::SourceCapabilities {
            supports_fifo: true,
            supports_search: false,
            supports_favorites: false,
            supports_playlists: false,
            supports_user_content: false,
            supports_high_res_audio: true,
            max_sample_rate: Some(96_000), // Radio Paradise FLAC is typically 44.1 or 48 kHz, up to 96 kHz
            supports_multiple_formats: true,
            supports_advanced_search: false,
            supports_pagination: false,
        }
    }

    async fn get_available_formats(&self, _object_id: &str) -> Result<Vec<pmosource::AudioFormat>> {
        use pmosource::AudioFormat;

        // Radio Paradise offers 5 quality levels
        Ok(vec![
            AudioFormat {
                format_id: "mp3-128".to_string(),
                mime_type: "audio/mpeg".to_string(),
                sample_rate: Some(44100),
                bit_depth: None,
                bitrate: Some(128),
                channels: Some(2),
            },
            AudioFormat {
                format_id: "aac-64".to_string(),
                mime_type: "audio/aac".to_string(),
                sample_rate: Some(44100),
                bit_depth: None,
                bitrate: Some(64),
                channels: Some(2),
            },
            AudioFormat {
                format_id: "aac-128".to_string(),
                mime_type: "audio/aac".to_string(),
                sample_rate: Some(44100),
                bit_depth: None,
                bitrate: Some(128),
                channels: Some(2),
            },
            AudioFormat {
                format_id: "aac-320".to_string(),
                mime_type: "audio/aac".to_string(),
                sample_rate: Some(44100),
                bit_depth: None,
                bitrate: Some(320),
                channels: Some(2),
            },
            AudioFormat {
                format_id: "flac".to_string(),
                mime_type: "audio/flac".to_string(),
                sample_rate: Some(44100),
                bit_depth: Some(16),
                bitrate: None,
                channels: Some(2),
            },
        ])
    }

    async fn get_cache_status(&self, object_id: &str) -> Result<pmosource::CacheStatus> {
        use pmosource::CacheStatus;

        let cache = self.inner.track_cache.read().await;

        if let Some(metadata) = cache.get(object_id) {
            #[cfg(feature = "cache")]
            {
                if let Some(ref audio_cache) = self.inner.audio_cache {
                    if let Some(ref pk) = metadata.cached_audio_pk {
                        // Check if the cached file exists and get its size
                        if let Ok(Some(info)) = audio_cache.get_info(pk).await {
                            return Ok(CacheStatus::Cached {
                                size_bytes: info.size_bytes,
                            });
                        }
                    }
                }
            }

            // Check legacy cached_pk for backward compatibility
            if metadata.cached_pk.is_some() {
                // We don't have size info for legacy cache
                return Ok(CacheStatus::Cached { size_bytes: 0 });
            }
        }

        Ok(CacheStatus::NotCached)
    }

    async fn cache_item(&self, object_id: &str) -> Result<pmosource::CacheStatus> {
        #[cfg(not(feature = "cache"))]
        {
            let _ = object_id;
            return Err(MusicSourceError::NotSupported("Caching not enabled".to_string()));
        }

        #[cfg(feature = "cache")]
        {
            use pmosource::CacheStatus;

            // Get the track metadata
            let cache = self.inner.track_cache.read().await;
            let metadata = cache
                .get(object_id)
                .ok_or_else(|| MusicSourceError::ObjectNotFound(object_id.to_string()))?
                .clone();
            drop(cache);

            // If already cached, return status
            if metadata.cached_audio_pk.is_some() {
                return self.get_cache_status(object_id).await;
            }

            // Cache it now
            if let Some(ref audio_cache) = self.inner.audio_cache {
                let song = &metadata.block.songs[metadata.song_index];

                let audio_metadata = pmoaudiocache::AudioMetadata {
                    title: Some(song.title.clone()),
                    artist: if !song.artist.is_empty() {
                        Some(song.artist.clone())
                    } else {
                        None
                    },
                    album: if !song.album.is_empty() {
                        Some(song.album.clone())
                    } else {
                        None
                    },
                    duration_secs: if song.duration > 0 {
                        Some((song.duration / 1000) as u64)
                    } else {
                        None
                    },
                    year: None,
                    track_number: None,
                    track_total: None,
                    disc_number: None,
                    disc_total: None,
                    genre: None,
                    sample_rate: None,
                    channels: None,
                    bitrate: None,
                };

                match audio_cache
                    .add_from_url(&metadata.original_uri, Some(audio_metadata))
                    .await
                {
                    Ok((pk, _)) => {
                        // Update the metadata
                        let mut cache = self.inner.track_cache.write().await;
                        if let Some(meta) = cache.get_mut(object_id) {
                            meta.cached_audio_pk = Some(pk);
                        }
                        return self.get_cache_status(object_id).await;
                    }
                    Err(e) => {
                        return Ok(CacheStatus::Failed {
                            error: e.to_string(),
                        });
                    }
                }
            }

            Ok(CacheStatus::NotCached)
        }
    }

    async fn browse_paginated(
        &self,
        object_id: &str,
        offset: usize,
        limit: usize,
    ) -> Result<BrowseResult> {
        // For Radio Paradise, we can efficiently paginate the FIFO
        if object_id == "radio-paradise" || object_id == "0" {
            let tracks = self.inner.playlist.get_items(offset, limit).await;
            let items: Vec<Item> = tracks.iter().map(|t| self.track_to_item(t)).collect();
            Ok(BrowseResult::Items(items))
        } else {
            Err(MusicSourceError::ObjectNotFound(object_id.to_string()))
        }
    }

    async fn get_item_count(&self, object_id: &str) -> Result<usize> {
        if object_id == "radio-paradise" || object_id == "0" {
            Ok(self.inner.playlist.len().await)
        } else {
            Err(MusicSourceError::ObjectNotFound(object_id.to_string()))
        }
    }

    async fn statistics(&self) -> Result<pmosource::SourceStatistics> {
        let mut stats = pmosource::SourceStatistics::default();

        // Total items in FIFO
        stats.total_items = Some(self.inner.playlist.len().await);

        // Cache statistics
        #[cfg(feature = "cache")]
        {
            let cache = self.inner.track_cache.read().await;
            let cached_count = cache.values().filter(|m| m.cached_audio_pk.is_some()).count();
            stats.cached_items = Some(cached_count);

            if let Some(ref audio_cache) = self.inner.audio_cache {
                if let Ok(cache_stats) = audio_cache.statistics().await {
                    stats.cache_size_bytes = Some(cache_stats.total_size_bytes);
                }
            }
        }

        Ok(stats)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_source_info() {
        let client = RadioParadiseClient::with_client(reqwest::Client::new());
        let source = RadioParadiseSource::new_default(client, "http://localhost:8080");

        assert_eq!(source.name(), "Radio Paradise");
        assert_eq!(source.id(), "radio-paradise");
        assert_eq!(source.default_image_mime_type(), "image/webp");
        assert!(source.supports_fifo());
    }

    #[test]
    fn test_default_image_present() {
        assert!(DEFAULT_IMAGE.len() > 0, "Default image should not be empty");

        // Check WebP magic bytes (RIFF...WEBP)
        assert!(DEFAULT_IMAGE.len() >= 12, "Image too small to be valid WebP");
        assert_eq!(&DEFAULT_IMAGE[0..4], b"RIFF", "Missing RIFF header");
        assert_eq!(&DEFAULT_IMAGE[8..12], b"WEBP", "Missing WEBP signature");
    }

    #[tokio::test]
    async fn test_fifo_operations() {
        let client = RadioParadiseClient::with_client(reqwest::Client::new());
        let source = RadioParadiseSource::new_default(client, "http://localhost:8080");

        // Initially empty
        let items = source.get_items(0, 10).await.unwrap();
        assert_eq!(items.len(), 0);

        // Test FIFO support
        assert!(source.supports_fifo());

        // Initial update_id
        let update_id = source.update_id().await;
        assert_eq!(update_id, 0);
    }
}
