//! Music source implementation for Radio Paradise
//!
//! This module implements the [`pmosource::MusicSource`] trait for Radio Paradise,
//! providing a complete music source with FIFO playlist support, browsing, and caching.

use crate::client::RadioParadiseClient;
use crate::models::{Block, Song};
use pmosource::{async_trait, pmodidl, BrowseResult, MusicSource, MusicSourceError, Result};
use pmosource::SourceCacheManager;
use pmoaudiocache::{AudioMetadata, Cache as AudioCache};
use pmocovers::Cache as CoverCache;
use pmodidl::{Container, Item, Resource};
use pmoplaylist::{FifoPlaylist, Track};
use std::sync::Arc;
use std::time::SystemTime;

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

    /// Cache manager (centralisé)
    cache_manager: SourceCacheManager,

    /// Blocks cache pour retrouver les métadonnées originales
    /// (track_id -> (block, song_index))
    blocks: tokio::sync::RwLock<std::collections::HashMap<String, (Arc<Block>, usize)>>,
}

impl std::fmt::Debug for RadioParadiseSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RadioParadiseSource")
            .finish()
    }
}

impl RadioParadiseSource {
    /// Create a new Radio Paradise source from the cache registry
    ///
    /// This is the recommended way to create a source when using the UPnP server.
    /// The caches are automatically retrieved from the global registry.
    ///
    /// # Arguments
    ///
    /// * `client` - Radio Paradise API client
    /// * `fifo_capacity` - Maximum number of tracks in the FIFO
    ///
    /// # Errors
    ///
    /// Returns an error if the caches are not initialized in the registry
    #[cfg(feature = "server")]
    pub fn from_registry(client: RadioParadiseClient, fifo_capacity: usize) -> Result<Self> {
        let playlist = FifoPlaylist::new(
            "radio-paradise".to_string(),
            "Radio Paradise".to_string(),
            fifo_capacity,
            DEFAULT_IMAGE,
        );

        let cache_manager = SourceCacheManager::from_registry("radio-paradise".to_string())?;

        Ok(Self {
            inner: Arc::new(RadioParadiseSourceInner {
                client,
                playlist,
                cache_manager,
                blocks: tokio::sync::RwLock::new(std::collections::HashMap::new()),
            }),
        })
    }

    /// Create with default FIFO capacity from the cache registry
    #[cfg(feature = "server")]
    pub fn from_registry_default(client: RadioParadiseClient) -> Result<Self> {
        Self::from_registry(client, DEFAULT_FIFO_CAPACITY)
    }

    /// Create a new Radio Paradise source with explicit caches (for tests)
    ///
    /// # Arguments
    ///
    /// * `client` - Radio Paradise API client
    /// * `fifo_capacity` - Maximum number of tracks in the FIFO
    /// * `cover_cache` - Cover image cache (required)
    /// * `audio_cache` - Audio cache (required)
    pub fn new(
        client: RadioParadiseClient,
        fifo_capacity: usize,
        cover_cache: Arc<CoverCache>,
        audio_cache: Arc<AudioCache>,
    ) -> Self {
        let playlist = FifoPlaylist::new(
            "radio-paradise".to_string(),
            "Radio Paradise".to_string(),
            fifo_capacity,
            DEFAULT_IMAGE,
        );

        let cache_manager = SourceCacheManager::new(
            "radio-paradise".to_string(),
            cover_cache,
            audio_cache,
        );

        Self {
            inner: Arc::new(RadioParadiseSourceInner {
                client,
                playlist,
                cache_manager,
                blocks: tokio::sync::RwLock::new(std::collections::HashMap::new()),
            }),
        }
    }

    /// Create with default FIFO capacity (for tests)
    pub fn new_default(
        client: RadioParadiseClient,
        cover_cache: Arc<CoverCache>,
        audio_cache: Arc<AudioCache>,
    ) -> Self {
        Self::new(client, DEFAULT_FIFO_CAPACITY, cover_cache, audio_cache)
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

        // 1. Cache cover via le manager
        let cached_cover_pk = if let Some(ref image_base) = block.image_base {
            if let Some(ref cover) = song.cover {
                let image_url = format!("{}{}", image_base, cover);

                match self.inner.cache_manager.cache_cover(&image_url).await {
                    Ok(pk) => {
                        // Use the cached cover URL
                        match self.inner.cache_manager.cover_url(&pk, None) {
                            Ok(cached_url) => {
                                track = track.with_image(cached_url);
                                Some(pk)
                            }
                            Err(e) => {
                                tracing::warn!("Failed to build cover URL for {}: {}", pk, e);
                                track = track.with_image(image_url);
                                Some(pk)
                            }
                        }
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
        };

        // 2. Cache audio via le manager (métadonnées pour compatibilité)
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

        let cached_audio_pk = match self.inner.cache_manager.cache_audio(&block.url, Some(metadata)).await {
            Ok(pk) => {
                tracing::info!("Successfully cached audio for track {}: {}", track_id, pk);
                Some(pk)
            }
            Err(e) => {
                tracing::warn!("Failed to cache audio for track {}: {}", track_id, e);
                None
            }
        };

        // 3. Store metadata in the cache manager
        self.inner.cache_manager.update_metadata(
            track_id.clone(),
            pmosource::TrackMetadata {
                original_uri: block.url.clone(),
                cached_audio_pk,
                cached_cover_pk,
            }
        ).await;

        // 4. Store block for later retrieval
        {
            let mut blocks = self.inner.blocks.write().await;
            blocks.insert(track_id.clone(), (block.clone(), song_index));
        }

        // 5. Add to FIFO
        self.inner.playlist.append_track(track).await;

        Ok(())
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
            // Si la FIFO est vide, la peupler avec les morceaux actuels
            if self.inner.playlist.len().await == 0 {
                tracing::info!("FIFO is empty, fetching current Radio Paradise tracks...");

                match self.inner.client.now_playing().await {
                    Ok(now_playing) => {
                        let block = Arc::new(now_playing.block);

                        // Ajouter tous les morceaux du bloc actuel
                        for (song_index, song) in block.songs_ordered() {
                            if let Err(e) = self.add_song(block.clone(), song, song_index).await {
                                tracing::warn!("Failed to add song '{}': {}", song.title, e);
                            } else {
                                tracing::debug!("Added song: {} - {}", song.artist, song.title);
                            }
                        }

                        tracing::info!("✅ Added {} tracks to Radio Paradise FIFO", block.song_count());
                    }
                    Err(e) => {
                        tracing::warn!("Failed to fetch current tracks: {}", e);
                    }
                }
            }

            let tracks = self.inner.playlist.get_items(0, 1000).await;
            let items: Vec<Item> = tracks.iter().map(|t| self.track_to_item(t)).collect();
            Ok(BrowseResult::Items(items))
        } else {
            Err(MusicSourceError::ObjectNotFound(object_id.to_string()))
        }
    }

    async fn resolve_uri(&self, object_id: &str) -> Result<String> {
        // Delegate to cache manager
        self.inner.cache_manager.resolve_uri(object_id).await
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
            // Remove from caches
            self.inner.cache_manager.remove_track(&track.id).await;
            let mut blocks = self.inner.blocks.write().await;
            blocks.remove(&track.id);

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
        // Delegate to cache manager
        self.inner.cache_manager.get_cache_status(object_id).await
    }

    async fn cache_item(&self, object_id: &str) -> Result<pmosource::CacheStatus> {
        use pmosource::CacheStatus;

        // Check if already cached
        let status = self.inner.cache_manager.get_cache_status(object_id).await?;
        if matches!(status, CacheStatus::Cached { .. }) {
            return Ok(status);
        }

        // Get metadata and block info
        let metadata = self.inner.cache_manager.get_metadata(object_id).await
            .ok_or_else(|| MusicSourceError::ObjectNotFound(object_id.to_string()))?;

        let blocks = self.inner.blocks.read().await;
        let (block, song_index) = blocks.get(object_id)
            .ok_or_else(|| MusicSourceError::ObjectNotFound(object_id.to_string()))?;
        let song = block.get_song(*song_index)
            .ok_or_else(|| MusicSourceError::ObjectNotFound(format!("Song {} not found", object_id)))?;

        // Prepare metadata
        let audio_metadata = AudioMetadata {
            title: Some(song.title.clone()),
            artist: if !song.artist.is_empty() { Some(song.artist.clone()) } else { None },
            album: if !song.album.is_empty() { Some(song.album.clone()) } else { None },
            duration_secs: if song.duration > 0 { Some((song.duration / 1000) as u64) } else { None },
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

        // Cache via manager
        match self.inner.cache_manager.cache_audio(&metadata.original_uri, Some(audio_metadata)).await {
            Ok(pk) => {
                // Update metadata with new pk
                let mut updated = metadata;
                updated.cached_audio_pk = Some(pk);
                self.inner.cache_manager.update_metadata(object_id.to_string(), updated).await;
                self.get_cache_status(object_id).await
            }
            Err(e) => Ok(CacheStatus::Failed { error: e.to_string() }),
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
        let cache_stats = self.inner.cache_manager.statistics().await;

        Ok(pmosource::SourceStatistics {
            total_items: Some(self.inner.playlist.len().await),
            cached_items: Some(cache_stats.cached_tracks),
            ..Default::default()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helper to create test caches (requires actual directories in tests)
    async fn create_test_caches() -> (Arc<CoverCache>, Arc<AudioCache>) {
        let temp_dir = std::env::temp_dir();
        let cover_dir = temp_dir.join("test_covers");
        let audio_dir = temp_dir.join("test_audio");

        std::fs::create_dir_all(&cover_dir).ok();
        std::fs::create_dir_all(&audio_dir).ok();

        let cover_cache = Arc::new(
            pmocovers::Cache::new(cover_dir.to_str().unwrap(), 100)
                .unwrap()
        );
        let audio_cache = Arc::new(
            pmoaudiocache::new_cache(audio_dir.to_str().unwrap(), 100)
                .unwrap()
        );

        (cover_cache, audio_cache)
    }

    #[tokio::test]
    async fn test_source_info() {
        let client = RadioParadiseClient::with_client(reqwest::Client::new());
        let (cover_cache, audio_cache) = create_test_caches().await;
        let source = RadioParadiseSource::new_default(
            client,
            cover_cache,
            audio_cache
        );

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
        let (cover_cache, audio_cache) = create_test_caches().await;
        let source = RadioParadiseSource::new_default(
            client,
            cover_cache,
            audio_cache
        );

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
