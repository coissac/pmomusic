//! Music source implementation for Qobuz
//!
//! This module implements the [`pmosource::MusicSource`] trait for Qobuz,
//! providing a complete music catalog browsing and searching experience.

use crate::client::QobuzClient;
use crate::didl::ToDIDL;
use crate::models::{Album, Track};
use pmosource::{async_trait, BrowseResult, MusicSource, MusicSourceError, Result};
use pmodidl::{Container, Item};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::SystemTime;
use tokio::sync::RwLock;

#[cfg(feature = "cache")]
use pmocovers::Cache as CoverCache;
#[cfg(feature = "cache")]
use pmoaudiocache::{AudioCache, AudioMetadata};

/// Default image for Qobuz (300x300 WebP, embedded in binary)
const DEFAULT_IMAGE: &[u8] = include_bytes!("../assets/default.webp");

/// Qobuz music source with full MusicSource trait implementation
///
/// This struct combines a [`QobuzClient`] for API access with browsing and
/// navigation capabilities, implementing the complete [`MusicSource`] trait.
///
/// # Features
///
/// - **Catalog Navigation**: Browse albums, artists, playlists, favorites
/// - **Search**: Full-text search across the Qobuz catalog
/// - **URI Resolution**: Resolves track streaming URIs with authentication
/// - **DIDL-Lite Export**: Converts albums, tracks, and playlists to UPnP formats
/// - **Caching**: Integrated with QobuzClient's cache for performance
///
/// # Architecture
///
/// Unlike streaming sources like Radio Paradise, Qobuz is a catalog-based source:
/// - Root container has multiple sub-containers (Albums, Artists, Favorites, etc.)
/// - No FIFO support (it's a static catalog, not a dynamic stream)
/// - Hierarchical browsing: Root → Category → Albums → Tracks
///
/// # Examples
///
/// ```no_run
/// use pmoqobuz::{QobuzSource, QobuzClient};
/// use pmosource::MusicSource;
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let client = QobuzClient::from_config().await?;
///     let source = QobuzSource::new(client);
///
///     println!("Source: {}", source.name());
///     println!("Supports FIFO: {}", source.supports_fifo());
///
///     // Browse root container
///     let root = source.root_container().await?;
///     println!("Root: {} with {} children", root.title, root.child_count.unwrap_or_default());
///
///     Ok(())
/// }
/// ```
#[derive(Clone)]
pub struct QobuzSource {
    inner: Arc<QobuzSourceInner>,
}

struct QobuzSourceInner {
    /// Qobuz API client
    client: QobuzClient,

    /// Cache server base URL for URI resolution
    cache_base_url: String,

    /// Track metadata cache (track_id -> TrackMetadata)
    track_cache: RwLock<HashMap<String, TrackMetadata>>,

    /// Cover image cache (optional)
    #[cfg(feature = "cache")]
    cover_cache: Option<Arc<CoverCache>>,

    /// Audio cache (optional)
    #[cfg(feature = "cache")]
    audio_cache: Option<Arc<AudioCache>>,

    /// Update tracking
    update_counter: RwLock<u32>,
    last_change: RwLock<SystemTime>,
}

#[derive(Debug, Clone)]
struct TrackMetadata {
    original_uri: String,
    #[cfg(feature = "cache")]
    cached_audio_pk: Option<String>,
    #[cfg(feature = "cache")]
    cached_cover_pk: Option<String>,
}

impl std::fmt::Debug for QobuzSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("QobuzSource").finish()
    }
}

impl QobuzSource {
    /// Create a new Qobuz source
    ///
    /// # Arguments
    ///
    /// * `client` - Authenticated Qobuz API client
    /// * `cache_base_url` - Base URL for the cache server (e.g., "http://localhost:8080")
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use pmoqobuz::{QobuzSource, QobuzClient};
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     let client = QobuzClient::from_config().await?;
    ///     let source = QobuzSource::new(client, "http://localhost:8080");
    ///     Ok(())
    /// }
    /// ```
    pub fn new(client: QobuzClient, cache_base_url: impl Into<String>) -> Self {
        Self {
            inner: Arc::new(QobuzSourceInner {
                client,
                cache_base_url: cache_base_url.into(),
                track_cache: RwLock::new(HashMap::new()),
                #[cfg(feature = "cache")]
                cover_cache: None,
                #[cfg(feature = "cache")]
                audio_cache: None,
                update_counter: RwLock::new(0),
                last_change: RwLock::new(SystemTime::now()),
            }),
        }
    }

    /// Create a new Qobuz source with caching support
    ///
    /// # Arguments
    ///
    /// * `client` - Authenticated Qobuz API client
    /// * `cache_base_url` - Base URL for the cache server (e.g., "http://localhost:8080")
    /// * `cover_cache` - Optional cover image cache
    /// * `audio_cache` - Optional audio cache
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use pmoqobuz::{QobuzSource, QobuzClient};
    /// use pmocovers::Cache as CoverCache;
    /// use pmoaudiocache::AudioCache;
    /// use std::sync::Arc;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     let client = QobuzClient::from_config().await?;
    ///     let cover_cache = Arc::new(CoverCache::new("/tmp/qobuz-covers").await?);
    ///     let audio_cache = Arc::new(AudioCache::new("/tmp/qobuz-audio").await?);
    ///
    ///     let source = QobuzSource::new_with_cache(
    ///         client,
    ///         "http://localhost:8080",
    ///         Some(cover_cache),
    ///         Some(audio_cache),
    ///     );
    ///     Ok(())
    /// }
    /// ```
    #[cfg(feature = "cache")]
    pub fn new_with_cache(
        client: QobuzClient,
        cache_base_url: impl Into<String>,
        cover_cache: Option<Arc<CoverCache>>,
        audio_cache: Option<Arc<AudioCache>>,
    ) -> Self {
        Self {
            inner: Arc::new(QobuzSourceInner {
                client,
                cache_base_url: cache_base_url.into(),
                track_cache: RwLock::new(HashMap::new()),
                cover_cache,
                audio_cache,
                update_counter: RwLock::new(0),
                last_change: RwLock::new(SystemTime::now()),
            }),
        }
    }

    /// Get the Qobuz client
    pub fn client(&self) -> &QobuzClient {
        &self.inner.client
    }

    /// Add a track from Qobuz with optional caching
    ///
    /// This method is used to add a Qobuz track to the internal cache,
    /// downloading and caching both cover art and audio data if caching is enabled.
    ///
    /// # Arguments
    ///
    /// * `track` - The Qobuz track to add
    ///
    /// # Returns
    ///
    /// Returns the track ID that was used for caching.
    pub async fn add_track(&self, track: &Track) -> Result<String> {
        let track_id = format!("qobuz://track/{}", track.id);

        // Get streaming URL
        let stream_url = self
            .inner
            .client
            .get_stream_url(&track.id)
            .await
            .map_err(|e| MusicSourceError::UriResolutionError(e.to_string()))?;

        // Cache cover image
        #[cfg(feature = "cache")]
        let cached_cover_pk = if let Some(ref cover_cache) = self.inner.cover_cache {
            if let Some(ref album) = track.album {
                if let Some(ref image_url) = album.image {
                    match cover_cache.add_from_url(image_url).await {
                        Ok(pk) => {
                            tracing::info!("Successfully cached cover for track {}: {}", track_id, pk);
                            Some(pk)
                        }
                        Err(e) => {
                            tracing::warn!("Failed to cache cover image {}: {}", image_url, e);
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

        // Cache audio asynchronously
        #[cfg(feature = "cache")]
        let cached_audio_pk = if let Some(ref audio_cache) = self.inner.audio_cache {
            // Prepare rich metadata from Qobuz track
            let metadata = AudioMetadata {
                title: Some(track.title.clone()),
                artist: track.performer.as_ref().map(|p| p.name.clone()),
                album: track.album.as_ref().map(|a| a.title.clone()),
                duration_secs: Some(track.duration as u64),
                year: track.album.as_ref().and_then(|a| {
                    a.release_date.as_ref().and_then(|d| {
                        // Parse year from ISO date (e.g., "2023-01-15")
                        d.split('-').next()?.parse().ok()
                    })
                }),
                track_number: Some(track.track_number),
                track_total: track.album.as_ref().and_then(|a| a.tracks_count),
                disc_number: Some(track.media_number),
                disc_total: None,
                genre: track.album.as_ref().and_then(|a| {
                    if !a.genres.is_empty() {
                        Some(a.genres.join(", "))
                    } else {
                        None
                    }
                }),
                sample_rate: track.sample_rate,
                channels: track.channels,
                bitrate: None, // Qobuz doesn't provide bitrate directly
            };

            // Cache the audio asynchronously
            match audio_cache.add_from_url(&stream_url, Some(metadata)).await {
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
                    original_uri: stream_url,
                    #[cfg(feature = "cache")]
                    cached_audio_pk,
                    #[cfg(feature = "cache")]
                    cached_cover_pk,
                },
            );
        }

        Ok(track_id)
    }

    /// Increment update counter (called on catalog changes)
    async fn increment_update_id(&self) {
        let mut counter = self.inner.update_counter.write().await;
        *counter = counter.wrapping_add(1);
        let mut last = self.inner.last_change.write().await;
        *last = SystemTime::now();
    }

    /// Parse object_id to determine what to browse
    ///
    /// Object IDs follow these patterns:
    /// - "qobuz" or "0" → Root container
    /// - "qobuz:favorites" → User's favorite albums
    /// - "qobuz:album:{id}" → Tracks in album
    /// - "qobuz:playlist:{id}" → Tracks in playlist
    fn parse_object_id(&self, object_id: &str) -> ObjectIdType {
        if object_id == "qobuz" || object_id == "0" {
            return ObjectIdType::Root;
        }

        let parts: Vec<&str> = object_id.split(':').collect();
        match parts.as_slice() {
            ["qobuz", "favorites"] => ObjectIdType::Favorites,
            ["qobuz", "album", id] => ObjectIdType::Album(id.to_string()),
            ["qobuz", "playlist", id] => ObjectIdType::Playlist(id.to_string()),
            ["qobuz", "artist", id] => ObjectIdType::Artist(id.to_string()),
            _ => ObjectIdType::Unknown,
        }
    }
}

#[derive(Debug)]
enum ObjectIdType {
    Root,
    Favorites,
    Album(String),
    Playlist(String),
    Artist(String),
    Unknown,
}

#[async_trait]
impl MusicSource for QobuzSource {
    fn name(&self) -> &str {
        "Qobuz"
    }

    fn id(&self) -> &str {
        "qobuz"
    }

    fn default_image(&self) -> &[u8] {
        DEFAULT_IMAGE
    }

    async fn root_container(&self) -> Result<Container> {
        // Create the root container with sub-containers for different categories
        Ok(Container {
            id: "qobuz".to_string(),
            parent_id: "0".to_string(),
            restricted: Some("1".to_string()),
            child_count: Some("2".to_string()), // Favorites + Search (simplified)
            title: "Qobuz".to_string(),
            class: "object.container".to_string(),
            containers: vec![
                // Favorites container
                Container {
                    id: "qobuz:favorites".to_string(),
                    parent_id: "qobuz".to_string(),
                    restricted: Some("1".to_string()),
                    child_count: None, // Will be determined when browsed
                    title: "My Favorites".to_string(),
                    class: "object.container".to_string(),
                    containers: vec![],
                    items: vec![],
                },
            ],
            items: vec![],
        })
    }

    async fn browse(&self, object_id: &str) -> Result<BrowseResult> {
        match self.parse_object_id(object_id) {
            ObjectIdType::Root => {
                // Return the root container's children
                let root = self.root_container().await?;
                Ok(BrowseResult::Containers(root.containers))
            }

            ObjectIdType::Favorites => {
                // Get user's favorite albums
                let albums = self
                    .inner
                    .client
                    .get_favorite_albums()
                    .await
                    .map_err(|e| MusicSourceError::BrowseError(e.to_string()))?;

                let containers: Vec<Container> = albums
                    .into_iter()
                    .filter_map(|album| album.to_didl_container("qobuz:favorites").ok())
                    .collect();

                Ok(BrowseResult::Containers(containers))
            }

            ObjectIdType::Album(album_id) => {
                // Get tracks in album
                let tracks = self
                    .inner
                    .client
                    .get_album_tracks(&album_id)
                    .await
                    .map_err(|e| MusicSourceError::BrowseError(e.to_string()))?;

                let items: Vec<Item> = tracks
                    .into_iter()
                    .filter_map(|track| {
                        track
                            .to_didl_item(&format!("qobuz:album:{}", album_id))
                            .ok()
                    })
                    .collect();

                Ok(BrowseResult::Items(items))
            }

            ObjectIdType::Playlist(playlist_id) => {
                // Get tracks in playlist
                let tracks = self
                    .inner
                    .client
                    .get_playlist_tracks(&playlist_id)
                    .await
                    .map_err(|e| MusicSourceError::BrowseError(e.to_string()))?;

                let items: Vec<Item> = tracks
                    .into_iter()
                    .filter_map(|track| {
                        track
                            .to_didl_item(&format!("qobuz:playlist:{}", playlist_id))
                            .ok()
                    })
                    .collect();

                Ok(BrowseResult::Items(items))
            }

            ObjectIdType::Artist(artist_id) => {
                // Get albums by artist
                let albums = self
                    .inner
                    .client
                    .get_artist_albums(&artist_id)
                    .await
                    .map_err(|e| MusicSourceError::BrowseError(e.to_string()))?;

                let containers: Vec<Container> = albums
                    .into_iter()
                    .filter_map(|album| {
                        album
                            .to_didl_container(&format!("qobuz:artist:{}", artist_id))
                            .ok()
                    })
                    .collect();

                Ok(BrowseResult::Containers(containers))
            }

            ObjectIdType::Unknown => Err(MusicSourceError::ObjectNotFound(object_id.to_string())),
        }
    }

    async fn resolve_uri(&self, object_id: &str) -> Result<String> {
        // Check if we have cached metadata for this track
        let cache = self.inner.track_cache.read().await;

        if let Some(metadata) = cache.get(object_id) {
            // Priority 1: Use cached audio if available
            #[cfg(feature = "cache")]
            if let Some(ref pk) = metadata.cached_audio_pk {
                return Ok(format!("{}/audio/tracks/{}/stream", self.inner.cache_base_url, pk));
            }

            // Priority 2: Return original stream URI (already fetched)
            return Ok(metadata.original_uri.clone());
        }

        // If not in cache, extract track ID and get streaming URL from Qobuz
        // Object IDs for tracks follow pattern: "qobuz://track/{id}"
        let track_id = if let Some(id) = object_id.strip_prefix("qobuz://track/") {
            id
        } else {
            object_id
        };

        // Get streaming URL from Qobuz
        self.inner
            .client
            .get_stream_url(track_id)
            .await
            .map_err(|e| MusicSourceError::UriResolutionError(e.to_string()))
    }

    fn supports_fifo(&self) -> bool {
        // Qobuz is a catalog, not a dynamic stream
        false
    }

    async fn append_track(&self, _track: Item) -> Result<()> {
        Err(MusicSourceError::FifoNotSupported)
    }

    async fn remove_oldest(&self) -> Result<Option<Item>> {
        Err(MusicSourceError::FifoNotSupported)
    }

    async fn update_id(&self) -> u32 {
        *self.inner.update_counter.read().await
    }

    async fn last_change(&self) -> Option<SystemTime> {
        Some(*self.inner.last_change.read().await)
    }

    async fn get_items(&self, offset: usize, count: usize) -> Result<Vec<Item>> {
        // For Qobuz, "get_items" returns favorite tracks with pagination
        let all_tracks = self
            .inner
            .client
            .get_favorite_tracks()
            .await
            .map_err(|e| MusicSourceError::BrowseError(e.to_string()))?;

        let items: Vec<Item> = all_tracks
            .into_iter()
            .skip(offset)
            .take(count)
            .filter_map(|track| track.to_didl_item("qobuz:favorites").ok())
            .collect();

        Ok(items)
    }

    async fn search(&self, query: &str) -> Result<BrowseResult> {
        // Search across Qobuz catalog
        let results = self
            .inner
            .client
            .search(query, None)
            .await
            .map_err(|e| MusicSourceError::BrowseError(e.to_string()))?;

        // Convert albums to containers and tracks to items
        let containers: Vec<Container> = results
            .albums
            .into_iter()
            .filter_map(|album| album.to_didl_container("qobuz").ok())
            .collect();

        let items: Vec<Item> = results
            .tracks
            .into_iter()
            .filter_map(|track| track.to_didl_item("qobuz").ok())
            .collect();

        if !containers.is_empty() || !items.is_empty() {
            Ok(BrowseResult::Mixed { containers, items })
        } else {
            Ok(BrowseResult::Items(vec![]))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_image_present() {
        assert!(DEFAULT_IMAGE.len() > 0, "Default image should not be empty");

        // Check WebP magic bytes (RIFF...WEBP)
        assert!(DEFAULT_IMAGE.len() >= 12, "Image too small to be valid WebP");
        assert_eq!(&DEFAULT_IMAGE[0..4], b"RIFF", "Missing RIFF header");
        assert_eq!(&DEFAULT_IMAGE[8..12], b"WEBP", "Missing WEBP signature");
    }

    // Note: We can't easily test parse_object_id without creating a real client
    // which requires authentication. The parsing logic is simple enough that
    // it's covered by integration tests.
}
