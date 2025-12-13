//! Music source implementation for Qobuz
//!
//! This module implements the [`pmosource::MusicSource`] trait for Qobuz,
//! providing a complete music catalog browsing and searching experience.

use crate::client::QobuzClient;
use crate::didl::ToDIDL;
use crate::models::Track;
use pmoaudiocache::{AudioMetadata, Cache as AudioCache};
use pmocovers::Cache as CoverCache;
use pmodidl::{Container, Item};
use pmosource::SourceCacheManager;
use pmosource::{async_trait, BrowseResult, MusicSource, MusicSourceError, Result};
use std::sync::Arc;
use std::time::SystemTime;

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
/// use std::sync::Arc;
/// use pmoaudiocache::cache as audio_cache;
/// use pmocovers::cache as cover_cache;
/// use pmoqobuz::{QobuzSource, QobuzClient};
/// use pmosource::MusicSource;
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let client = QobuzClient::from_config().await?;
///     let cover_cache = Arc::new(cover_cache::new_cache("/tmp/qobuz_covers", 256)?);
///     let audio_cache = Arc::new(audio_cache::new_cache("/tmp/qobuz_audio", 64)?);
///     let source = QobuzSource::new(client, cover_cache, audio_cache);
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

    /// Cache manager (centralisé)
    cache_manager: SourceCacheManager,

    /// Update tracking
    update_counter: tokio::sync::RwLock<u32>,
    last_change: tokio::sync::RwLock<SystemTime>,
}

impl std::fmt::Debug for QobuzSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("QobuzSource").finish()
    }
}

impl QobuzSource {
    /// Create a new Qobuz source from the cache registry
    ///
    /// This is the recommended way to create a source when using the UPnP server.
    /// The caches are automatically retrieved from the global registry.
    ///
    /// # Arguments
    ///
    /// * `client` - Authenticated Qobuz API client
    ///
    /// # Errors
    ///
    /// Returns an error if the caches are not initialized in the registry
    #[cfg(feature = "server")]
    pub fn from_registry(client: QobuzClient) -> Result<Self> {
        let cache_manager = SourceCacheManager::from_registry("qobuz".to_string())?;

        Ok(Self {
            inner: Arc::new(QobuzSourceInner {
                client,
                cache_manager,
                update_counter: tokio::sync::RwLock::new(0),
                last_change: tokio::sync::RwLock::new(SystemTime::now()),
            }),
        })
    }

    /// Create a new Qobuz source with explicit caches (for tests)
    ///
    /// # Arguments
    ///
    /// * `client` - Authenticated Qobuz API client
    /// * `cover_cache` - Cover image cache (required)
    /// * `audio_cache` - Audio cache (required)
    pub fn new(
        client: QobuzClient,
        cover_cache: Arc<CoverCache>,
        audio_cache: Arc<AudioCache>,
    ) -> Self {
        let cache_manager = SourceCacheManager::new("qobuz".to_string(), cover_cache, audio_cache);

        Self {
            inner: Arc::new(QobuzSourceInner {
                client,
                cache_manager,
                update_counter: tokio::sync::RwLock::new(0),
                last_change: tokio::sync::RwLock::new(SystemTime::now()),
            }),
        }
    }

    /// Get the Qobuz client
    pub fn client(&self) -> &QobuzClient {
        &self.inner.client
    }

    /// Add a track from Qobuz with caching
    ///
    /// This method downloads and caches both cover art and audio data.
    pub async fn add_track(&self, track: &Track) -> Result<String> {
        let track_id = format!("qobuz://track/{}", track.id);

        // Get streaming URL
        let stream_url = self
            .inner
            .client
            .get_stream_url(&track.id)
            .await
            .map_err(|e| MusicSourceError::UriResolutionError(e.to_string()))?;

        // 1. Cache cover via manager
        let cached_cover_pk = if let Some(ref album) = track.album {
            if let Some(ref image_url) = album.image {
                self.inner.cache_manager.cache_cover(image_url).await.ok()
            } else {
                None
            }
        } else {
            None
        };

        // 2. Prepare rich metadata from Qobuz track
        let metadata = AudioMetadata {
            title: Some(track.title.clone()),
            artist: track.performer.as_ref().map(|p| p.name.clone()),
            album: track.album.as_ref().map(|a| a.title.clone()),
            duration_secs: Some(track.duration as u64),
            year: track.album.as_ref().and_then(|a| {
                a.release_date
                    .as_ref()
                    .and_then(|d| d.split('-').next()?.parse().ok())
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
            bitrate: None,
            conversion: None,
        };

        // 3. Cache audio via manager
        let cached_audio_pk = self
            .inner
            .cache_manager
            .cache_audio(&stream_url, Some(metadata))
            .await
            .ok();

        // 4. Store metadata
        self.inner
            .cache_manager
            .update_metadata(
                track_id.clone(),
                pmosource::TrackMetadata {
                    original_uri: stream_url,
                    cached_audio_pk,
                    cached_cover_pk,
                },
            )
            .await;

        Ok(track_id)
    }

    /// Add track with lazy audio caching (cover eager, audio lazy)
    ///
    /// This method caches cover art immediately (small, needed for UI) but
    /// defers audio download until the track is actually played.
    ///
    /// # Arguments
    ///
    /// * `track` - The Qobuz track to add
    ///
    /// # Returns
    ///
    /// The track ID (e.g., "qobuz://track/12345")
    pub async fn add_track_lazy(&self, track: &Track) -> Result<String> {
        let track_id = format!("qobuz://track/{}", track.id);

        // Get streaming URL
        let stream_url = self
            .inner
            .client
            .get_stream_url(&track.id)
            .await
            .map_err(|e| MusicSourceError::UriResolutionError(e.to_string()))?;

        // 1. Cache cover EAGERLY (small, UI needs it)
        let cached_cover_pk = if let Some(ref album) = track.album {
            if let Some(ref image_url) = album.image {
                self.inner.cache_manager.cache_cover(image_url).await.ok()
            } else {
                None
            }
        } else {
            None
        };

        // 2. Prepare rich metadata from Qobuz track
        let metadata = AudioMetadata {
            title: Some(track.title.clone()),
            artist: track.performer.as_ref().map(|p| p.name.clone()),
            album: track.album.as_ref().map(|a| a.title.clone()),
            duration_secs: Some(track.duration as u64),
            year: track.album.as_ref().and_then(|a| {
                a.release_date
                    .as_ref()
                    .and_then(|d| d.split('-').next()?.parse().ok())
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
            bitrate: None,
            conversion: None,
        };

        // 3. Cache audio LAZILY (KEY CHANGE: use cache_audio_lazy)
        let cached_audio_pk = self
            .inner
            .cache_manager
            .cache_audio_lazy(&stream_url, Some(metadata))
            .await
            .ok();

        // 4. Store metadata
        self.inner
            .cache_manager
            .update_metadata(
                track_id.clone(),
                pmosource::TrackMetadata {
                    original_uri: stream_url,
                    cached_audio_pk,
                    cached_cover_pk,
                },
            )
            .await;

        Ok(track_id)
    }

    /// Load full album into pmoplaylist with lazy audio
    ///
    /// This method fetches all tracks from a Qobuz album and adds them to a playlist
    /// with lazy audio loading. Covers are downloaded eagerly, audio lazily.
    ///
    /// # Arguments
    ///
    /// * `playlist_id` - ID of the target playlist
    /// * `album_id` - Qobuz album ID
    ///
    /// # Returns
    ///
    /// Number of tracks successfully added
    pub async fn add_album_to_playlist(&self, playlist_id: &str, album_id: &str) -> Result<usize> {
        use tracing::{debug, info, warn};

        // 1. Get tracks from Qobuz (goes through rate limiter)
        let tracks = self
            .inner
            .client
            .get_album_tracks(album_id)
            .await
            .map_err(|e| MusicSourceError::BrowseError(e.to_string()))?;

        if tracks.is_empty() {
            return Ok(0);
        }

        info!(
            "Adding album {} ({} tracks) to playlist {} with lazy audio",
            album_id,
            tracks.len(),
            playlist_id
        );

        // 2. Add each track lazily + collect lazy PKs
        let mut lazy_pks = Vec::with_capacity(tracks.len());

        for (i, track) in tracks.iter().enumerate() {
            match self.add_track_lazy(track).await {
                Ok(track_id) => {
                    debug!("Track {}/{}: {} (lazy)", i + 1, tracks.len(), track.title);

                    // Extract lazy PK from cache manager
                    if let Some(metadata) = self.inner.cache_manager.get_metadata(&track_id).await {
                        if let Some(audio_pk) = metadata.cached_audio_pk {
                            lazy_pks.push(audio_pk);
                        }
                    }
                }
                Err(e) => {
                    warn!("Failed to add track {} ({}): {}", i + 1, track.title, e);
                    // Continue with other tracks
                }
            }
        }

        // 3. Batch insert into playlist (single DB transaction)
        let playlist_manager = pmoplaylist::PlaylistManager();
        let writer = playlist_manager
            .get_write_handle(playlist_id.to_string())
            .await
            .map_err(|e| MusicSourceError::PlaylistError(e.to_string()))?;

        writer
            .push_lazy_batch(lazy_pks.clone())
            .await
            .map_err(|e| MusicSourceError::PlaylistError(e.to_string()))?;

        // 4. Enable lazy mode with lookahead of 2 tracks
        playlist_manager.enable_lazy_mode(playlist_id, 2);

        info!(
            "Album {} added: {}/{} tracks",
            album_id,
            lazy_pks.len(),
            tracks.len()
        );

        Ok(lazy_pks.len())
    }

    /// Load Qobuz playlist into pmoplaylist with lazy audio
    ///
    /// This method fetches all tracks from a Qobuz playlist and adds them to a pmoplaylist
    /// with lazy audio loading. Covers are downloaded eagerly, audio lazily.
    ///
    /// # Arguments
    ///
    /// * `playlist_id` - ID of the target pmoplaylist
    /// * `qobuz_playlist_id` - Qobuz playlist ID
    ///
    /// # Returns
    ///
    /// Number of tracks successfully added
    pub async fn add_qobuz_playlist_to_playlist(
        &self,
        playlist_id: &str,
        qobuz_playlist_id: &str,
    ) -> Result<usize> {
        use tracing::{debug, info, warn};

        // 1. Get tracks from Qobuz playlist (goes through rate limiter)
        let tracks = self
            .inner
            .client
            .get_playlist_tracks(qobuz_playlist_id)
            .await
            .map_err(|e| MusicSourceError::BrowseError(e.to_string()))?;

        if tracks.is_empty() {
            return Ok(0);
        }

        info!(
            "Adding Qobuz playlist {} ({} tracks) to pmoplaylist {} with lazy audio",
            qobuz_playlist_id,
            tracks.len(),
            playlist_id
        );

        // 2. Add each track lazily + collect lazy PKs
        let mut lazy_pks = Vec::with_capacity(tracks.len());

        for (i, track) in tracks.iter().enumerate() {
            match self.add_track_lazy(track).await {
                Ok(track_id) => {
                    debug!("Track {}/{}: {} (lazy)", i + 1, tracks.len(), track.title);

                    // Extract lazy PK from cache manager
                    if let Some(metadata) = self.inner.cache_manager.get_metadata(&track_id).await {
                        if let Some(audio_pk) = metadata.cached_audio_pk {
                            lazy_pks.push(audio_pk);
                        }
                    }
                }
                Err(e) => {
                    warn!("Failed to add track {} ({}): {}", i + 1, track.title, e);
                    // Continue with other tracks
                }
            }
        }

        // 3. Batch insert into playlist (single DB transaction)
        let playlist_manager = pmoplaylist::PlaylistManager();
        let writer = playlist_manager
            .get_write_handle(playlist_id.to_string())
            .await
            .map_err(|e| MusicSourceError::PlaylistError(e.to_string()))?;

        writer
            .push_lazy_batch(lazy_pks.clone())
            .await
            .map_err(|e| MusicSourceError::PlaylistError(e.to_string()))?;

        // 4. Enable lazy mode with lookahead of 2 tracks
        playlist_manager.enable_lazy_mode(playlist_id, 2);

        info!(
            "Qobuz playlist {} added: {}/{} tracks",
            qobuz_playlist_id,
            lazy_pks.len(),
            tracks.len()
        );

        Ok(lazy_pks.len())
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
            searchable: Some("1".to_string()),
            title: "Qobuz".to_string(),
            class: "object.container".to_string(),
            containers: vec![
                // Favorites container
                Container {
                    id: "qobuz:favorites".to_string(),
                    parent_id: "qobuz".to_string(),
                    restricted: Some("1".to_string()),
                    child_count: None, // Will be determined when browsed
                    searchable: Some("1".to_string()),
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
        // Try cache manager first
        if let Ok(uri) = self.inner.cache_manager.resolve_uri(object_id).await {
            return Ok(uri);
        }

        // If not cached, extract track ID and get streaming URL from Qobuz
        let track_id = object_id
            .strip_prefix("qobuz://track/")
            .unwrap_or(object_id);

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

    // ============= Extended Features Implementation =============

    fn capabilities(&self) -> pmosource::SourceCapabilities {
        pmosource::SourceCapabilities {
            supports_fifo: false,
            supports_search: true,
            supports_favorites: true,
            supports_playlists: true,
            supports_user_content: false,
            supports_high_res_audio: true,
            max_sample_rate: Some(192_000), // Qobuz supports up to 192kHz
            supports_multiple_formats: true,
            supports_advanced_search: false, // TODO: Qobuz API supports it, not yet implemented
            supports_pagination: true,
        }
    }

    async fn get_available_formats(&self, object_id: &str) -> Result<Vec<pmosource::AudioFormat>> {
        use pmosource::AudioFormat;

        // Extract track ID from object_id
        let track_id = if let Some(id) = object_id.strip_prefix("qobuz://track/") {
            id
        } else {
            object_id
        };

        // Get track details from Qobuz
        let track = self
            .inner
            .client
            .get_track(track_id)
            .await
            .map_err(|e| MusicSourceError::BrowseError(e.to_string()))?;

        // Qobuz provides multiple formats based on subscription
        let mut formats = vec![];

        // MP3 320 (format_id 5) - available to all
        formats.push(AudioFormat {
            format_id: "mp3-320".to_string(),
            mime_type: "audio/mpeg".to_string(),
            sample_rate: Some(44100),
            bit_depth: None,
            bitrate: Some(320),
            channels: Some(2),
        });

        // FLAC 16/44.1 (format_id 6) - CD quality
        formats.push(AudioFormat {
            format_id: "flac-16-44".to_string(),
            mime_type: "audio/flac".to_string(),
            sample_rate: Some(44100),
            bit_depth: Some(16),
            bitrate: None,
            channels: Some(2),
        });

        // Hi-Res formats (if available for this track)
        if let Some(sample_rate) = track.sample_rate {
            if sample_rate > 44100 {
                // FLAC 24-bit Hi-Res
                let bit_depth = track.bit_depth.map(|d| d as u8).or(Some(24));

                formats.push(AudioFormat {
                    format_id: format!("flac-{}-{}", bit_depth.unwrap_or(24), sample_rate / 1000),
                    mime_type: "audio/flac".to_string(),
                    sample_rate: Some(sample_rate),
                    bit_depth,
                    bitrate: None,
                    channels: track.channels,
                });
            }
        }

        Ok(formats)
    }

    async fn get_cache_status(&self, object_id: &str) -> Result<pmosource::CacheStatus> {
        self.inner.cache_manager.get_cache_status(object_id).await
    }

    async fn cache_item(&self, object_id: &str) -> Result<pmosource::CacheStatus> {
        // Extract track ID
        let track_id = object_id
            .strip_prefix("qobuz://track/")
            .unwrap_or(object_id);

        // Get track details from Qobuz
        let track = self
            .inner
            .client
            .get_track(track_id)
            .await
            .map_err(|e| MusicSourceError::BrowseError(e.to_string()))?;

        // Add track to cache (via manager)
        let cached_id = self.add_track(&track).await?;

        // Return the cache status
        self.get_cache_status(&cached_id).await
    }

    async fn add_favorite(&self, object_id: &str) -> Result<()> {
        // Parse object_id to determine type
        let parts: Vec<&str> = object_id.split(':').collect();

        match parts.as_slice() {
            ["qobuz", "album", id] | ["qobuz://album", id] => {
                self.inner
                    .client
                    .add_favorite_album(id)
                    .await
                    .map_err(|e| MusicSourceError::FavoritesError(e.to_string()))?;
            }
            ["qobuz", "track", id] | ["qobuz://track", id] => {
                self.inner
                    .client
                    .add_favorite_track(id)
                    .await
                    .map_err(|e| MusicSourceError::FavoritesError(e.to_string()))?;
            }
            _ => {
                return Err(MusicSourceError::NotSupported(
                    "Favorites only supported for albums and tracks".to_string(),
                ));
            }
        }

        self.increment_update_id().await;
        Ok(())
    }

    async fn remove_favorite(&self, object_id: &str) -> Result<()> {
        // Parse object_id to determine type
        let parts: Vec<&str> = object_id.split(':').collect();

        match parts.as_slice() {
            ["qobuz", "album", id] | ["qobuz://album", id] => {
                self.inner
                    .client
                    .remove_favorite_album(id)
                    .await
                    .map_err(|e| MusicSourceError::FavoritesError(e.to_string()))?;
            }
            ["qobuz", "track", id] | ["qobuz://track", id] => {
                self.inner
                    .client
                    .remove_favorite_track(id)
                    .await
                    .map_err(|e| MusicSourceError::FavoritesError(e.to_string()))?;
            }
            _ => {
                return Err(MusicSourceError::NotSupported(
                    "Favorites only supported for albums and tracks".to_string(),
                ));
            }
        }

        self.increment_update_id().await;
        Ok(())
    }

    async fn is_favorite(&self, object_id: &str) -> Result<bool> {
        // Parse object_id to determine type
        let parts: Vec<&str> = object_id.split(':').collect();

        match parts.as_slice() {
            ["qobuz", "album", id] | ["qobuz://album", id] => {
                let favorites = self
                    .inner
                    .client
                    .get_favorite_albums()
                    .await
                    .map_err(|e| MusicSourceError::FavoritesError(e.to_string()))?;

                Ok(favorites.iter().any(|album| album.id == *id))
            }
            ["qobuz", "track", id] | ["qobuz://track", id] => {
                let favorites = self
                    .inner
                    .client
                    .get_favorite_tracks()
                    .await
                    .map_err(|e| MusicSourceError::FavoritesError(e.to_string()))?;

                Ok(favorites.iter().any(|track| track.id == *id))
            }
            _ => Err(MusicSourceError::NotSupported(
                "Favorites only supported for albums and tracks".to_string(),
            )),
        }
    }

    async fn get_user_playlists(&self) -> Result<Vec<Container>> {
        let playlists = self
            .inner
            .client
            .get_user_playlists()
            .await
            .map_err(|e| MusicSourceError::PlaylistError(e.to_string()))?;

        let containers: Vec<Container> = playlists
            .into_iter()
            .filter_map(|playlist| playlist.to_didl_container("qobuz").ok())
            .collect();

        Ok(containers)
    }

    async fn add_to_playlist(&self, playlist_id: &str, item_id: &str) -> Result<()> {
        // Extract track ID from item_id
        let track_id = if let Some(id) = item_id.strip_prefix("qobuz://track/") {
            id
        } else if let Some(id) = item_id.strip_prefix("qobuz:track:") {
            id
        } else {
            item_id
        };

        self.inner
            .client
            .add_to_playlist(playlist_id, track_id)
            .await
            .map_err(|e| MusicSourceError::PlaylistError(e.to_string()))?;

        self.increment_update_id().await;
        Ok(())
    }

    async fn get_item_count(&self, object_id: &str) -> Result<usize> {
        match self.parse_object_id(object_id) {
            ObjectIdType::Album(album_id) => {
                let album = self
                    .inner
                    .client
                    .get_album(&album_id)
                    .await
                    .map_err(|e| MusicSourceError::BrowseError(e.to_string()))?;

                Ok(album.tracks_count.unwrap_or(0) as usize)
            }
            ObjectIdType::Playlist(playlist_id) => {
                let playlist = self
                    .inner
                    .client
                    .get_playlist(&playlist_id)
                    .await
                    .map_err(|e| MusicSourceError::BrowseError(e.to_string()))?;

                Ok(playlist.tracks_count.unwrap_or(0) as usize)
            }
            _ => {
                // Fall back to default implementation
                let result = self.browse(object_id).await?;
                Ok(result.count())
            }
        }
    }

    async fn browse_paginated(
        &self,
        object_id: &str,
        offset: usize,
        limit: usize,
    ) -> Result<BrowseResult> {
        match self.parse_object_id(object_id) {
            ObjectIdType::Album(album_id) => {
                // Qobuz returns all tracks, so we slice them
                let tracks = self
                    .inner
                    .client
                    .get_album_tracks(&album_id)
                    .await
                    .map_err(|e| MusicSourceError::BrowseError(e.to_string()))?;

                let items: Vec<Item> = tracks
                    .into_iter()
                    .skip(offset)
                    .take(limit)
                    .filter_map(|track| {
                        track
                            .to_didl_item(&format!("qobuz:album:{}", album_id))
                            .ok()
                    })
                    .collect();

                Ok(BrowseResult::Items(items))
            }
            ObjectIdType::Favorites => {
                let albums = self
                    .inner
                    .client
                    .get_favorite_albums()
                    .await
                    .map_err(|e| MusicSourceError::BrowseError(e.to_string()))?;

                let containers: Vec<Container> = albums
                    .into_iter()
                    .skip(offset)
                    .take(limit)
                    .filter_map(|album| album.to_didl_container("qobuz:favorites").ok())
                    .collect();

                Ok(BrowseResult::Containers(containers))
            }
            _ => {
                // Fall back to default implementation
                self.browse(object_id).await
            }
        }
    }

    async fn statistics(&self) -> Result<pmosource::SourceStatistics> {
        let mut stats = pmosource::SourceStatistics::default();

        // Try to get favorite counts
        if let Ok(albums) = self.inner.client.get_favorite_albums().await {
            stats.total_containers = Some(albums.len());
        }

        if let Ok(tracks) = self.inner.client.get_favorite_tracks().await {
            stats.total_items = Some(tracks.len());
        }

        // Get cache statistics from manager
        let cache_stats = self.inner.cache_manager.statistics().await;
        stats.cached_items = Some(cache_stats.cached_tracks);

        Ok(stats)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_image_present() {
        assert!(DEFAULT_IMAGE.len() > 0, "Default image should not be empty");

        // Check WebP magic bytes (RIFF...WEBP)
        assert!(
            DEFAULT_IMAGE.len() >= 12,
            "Image too small to be valid WebP"
        );
        assert_eq!(&DEFAULT_IMAGE[0..4], b"RIFF", "Missing RIFF header");
        assert_eq!(&DEFAULT_IMAGE[8..12], b"WEBP", "Missing WEBP signature");
    }

    // Note: We can't easily test parse_object_id without creating a real client
    // which requires authentication. The parsing logic is simple enough that
    // it's covered by integration tests.
}
