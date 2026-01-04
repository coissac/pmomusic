//! Music source implementation for Qobuz
//!
//! This module implements the [`pmosource::MusicSource`] trait for Qobuz,
//! providing a complete music catalog browsing and searching experience.

use crate::client::QobuzClient;
use crate::didl::ToDIDL;
use crate::lazy_provider::QobuzLazyProvider;
use crate::models::Track;
use pmoaudiocache::{AudioMetadata, Cache as AudioCache};
use pmocovers::Cache as CoverCache;
use pmodidl::{Container, Item};
use pmosource::SourceCacheManager;
use pmosource::{async_trait, BrowseResult, MusicSource, MusicSourceError, Result};
use serde_json::json;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

/// TTL pour les playlists d'albums (7 jours)
const ALBUM_PLAYLIST_TTL: Duration = Duration::from_secs(7 * 24 * 3600);

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
///     let source = QobuzSource::new(client, cover_cache, audio_cache, "http://localhost:8080");
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
    client: Arc<QobuzClient>,

    /// Cache manager (centralisé)
    cache_manager: SourceCacheManager,

    /// Base URL for streaming server (e.g., "http://192.168.0.138:8080")
    base_url: String,

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
    /// * `base_url` - Base URL for streaming server (e.g., "http://192.168.0.138:8080")
    ///
    /// # Errors
    ///
    /// Returns an error if the caches are not initialized in the registry
    #[cfg(feature = "server")]
    pub fn from_registry(client: QobuzClient, base_url: impl Into<String>) -> Result<Self> {
        let cache_manager = SourceCacheManager::from_registry("qobuz".to_string())?;
        let client = Arc::new(client);
        cache_manager.register_lazy_provider(Arc::new(QobuzLazyProvider::new(client.clone())));

        Ok(Self {
            inner: Arc::new(QobuzSourceInner {
                client,
                cache_manager,
                base_url: base_url.into(),
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
    /// * `base_url` - Base URL for streaming server (e.g., "http://192.168.0.138:8080")
    pub fn new(
        client: QobuzClient,
        cover_cache: Arc<CoverCache>,
        audio_cache: Arc<AudioCache>,
        base_url: impl Into<String>,
    ) -> Self {
        let cache_manager = SourceCacheManager::new("qobuz".to_string(), cover_cache, audio_cache);
        let client = Arc::new(client);
        cache_manager.register_lazy_provider(Arc::new(QobuzLazyProvider::new(client.clone())));

        Self {
            inner: Arc::new(QobuzSourceInner {
                client,
                cache_manager,
                base_url: base_url.into(),
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

        if let (Some(ref audio_pk), Some(ref cover_pk)) = (&cached_audio_pk, &cached_cover_pk) {
            let _ =
                self.inner
                    .cache_manager
                    .set_audio_metadata(audio_pk, "cover_pk", json!(cover_pk));
        }

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
    /// `(track_id, lazy_pk)` where `track_id` is the logical Qobuz URI and
    /// `lazy_pk` the cache identifier stored in pmocache.
    pub async fn add_track_lazy(&self, track: &Track) -> Result<(String, String)> {
        let track_id = format!("qobuz://track/{}", track.id);

        let lazy_pk = format!("QOBUZ:{}", track.id);

        // Get streaming URL (for metadata fallback)
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

        // 3. Cache audio LAZILY avec un provider
        let cached_audio_pk = self
            .inner
            .cache_manager
            .cache_audio_lazy_with_provider(
                &lazy_pk,
                Some(metadata.clone()),
                cached_cover_pk.clone(),
            )
            .await
            .map_err(|e| {
                MusicSourceError::CacheError(format!(
                    "Failed to register lazy track {}: {}",
                    track.title, e
                ))
            })?;

        if let Some(ref cover_pk) = cached_cover_pk {
            let _ = self.inner.cache_manager.set_audio_metadata(
                &cached_audio_pk,
                "cover_pk",
                json!(cover_pk),
            );
        }

        // Stocker le track_id Qobuz pour reconstruction DIDL ultérieure
        let _ = self.inner.cache_manager.set_audio_metadata(
            &cached_audio_pk,
            "qobuz_track_id",
            json!(track.id),
        );

        // 4. Store metadata
        self.inner
            .cache_manager
            .update_metadata(
                track_id.clone(),
                pmosource::TrackMetadata {
                    original_uri: stream_url,
                    cached_audio_pk: Some(cached_audio_pk.clone()),
                    cached_cover_pk,
                },
            )
            .await;

        Ok((track_id, cached_audio_pk))
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
                Ok((_track_id, lazy_pk)) => {
                    debug!(
                        "Track {}/{}: {} (lazy pk {})",
                        i + 1,
                        tracks.len(),
                        track.title,
                        &lazy_pk
                    );
                    lazy_pks.push(lazy_pk);
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
            .get_persistent_write_handle(playlist_id.to_string())
            .await
            .map_err(|e| MusicSourceError::PlaylistError(e.to_string()))?;

        writer
            .set_role(pmoplaylist::PlaylistRole::Album)
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
                Ok((_track_id, lazy_pk)) => {
                    debug!(
                        "Track {}/{}: {} (lazy pk {})",
                        i + 1,
                        tracks.len(),
                        track.title,
                        &lazy_pk
                    );
                    lazy_pks.push(lazy_pk);
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
            .get_persistent_write_handle(playlist_id.to_string())
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

    /// Vérifie si une playlist d'album existe et est valide (non expirée ET non vide)
    async fn is_album_playlist_valid(&self, playlist_id: &str) -> Result<bool> {
        let playlist_manager = pmoplaylist::PlaylistManager();

        if !playlist_manager.exists(playlist_id).await {
            return Ok(false);
        }

        // Vérifier l'âge
        match playlist_manager.get_playlist_age(playlist_id).await {
            Ok(Some(age)) if age < ALBUM_PLAYLIST_TTL => {
                // Playlist non expirée, vérifier qu'elle contient des tracks
                match playlist_manager.get_read_handle(playlist_id).await {
                    Ok(reader) => {
                        let count = reader.remaining().await.unwrap_or(0);
                        Ok(count > 0) // Valide seulement si non vide
                    }
                    Err(_) => Ok(false),
                }
            }
            _ => Ok(false),
        }
    }

    /// Adapte les Items d'une playlist pour correspondre au schéma UPnP Qobuz
    async fn adapt_playlist_items_to_qobuz(
        &self,
        items: Vec<Item>,
        album_id: &str,
    ) -> Result<Vec<Item>> {
        use tracing::warn;

        let parent_id = format!("qobuz:album:{}", album_id);

        let mut adapted = Vec::with_capacity(items.len());

        for mut item in items {
            // Extraire cache_pk depuis l'URL du resource
            let cache_pk = if let Some(resource) = item.resources.first() {
                resource.url
                    .strip_prefix("/audio/flac/")
                    .map(|s| s.to_string())
            } else {
                None
            };

            if let Some(pk) = cache_pk {
                // Récupérer track_id depuis metadata
                if let Ok(Some(track_id_value)) = self.inner.cache_manager.get_audio_metadata(&pk, "qobuz_track_id") {
                    if let Some(track_id) = track_id_value.as_str() {
                        item.id = format!("qobuz:track:{}", track_id);
                    } else {
                        warn!("qobuz_track_id not a string for {}", pk);
                    }
                } else {
                    warn!("No qobuz_track_id metadata for {}", pk);
                }

                // Convertir URL relative en URL absolue
                // From: /audio/flac/QOBUZ:123
                // To: http://192.168.0.138:8080/audio/flac/QOBUZ:123
                if let Some(resource) = item.resources.first_mut() {
                    if resource.url.starts_with('/') {
                        resource.url = format!("{}{}", self.inner.base_url, resource.url);
                    }
                }
            }

            item.parent_id = parent_id.clone();
            adapted.push(item);
        }

        Ok(adapted)
    }

    /// Récupère ou crée une playlist lazy pour un album
    async fn get_or_create_album_playlist_items(
        &self,
        album_id: &str,
        limit: usize,
    ) -> Result<Vec<Item>> {
        use tracing::{debug, info};

        let playlist_id = format!("qobuz-album-{}", album_id);
        let playlist_manager = pmoplaylist::PlaylistManager();

        // Vérifier validité (existe ET non expirée)
        let is_valid = self.is_album_playlist_valid(&playlist_id).await?;

        if is_valid {
            debug!("Album playlist {} found and valid", playlist_id);

            let reader = playlist_manager
                .get_read_handle(&playlist_id)
                .await
                .map_err(|e| MusicSourceError::PlaylistError(e.to_string()))?;

            let items = reader
                .to_items(limit)
                .await
                .map_err(|e| MusicSourceError::PlaylistError(e.to_string()))?;

            return self.adapt_playlist_items_to_qobuz(items, album_id).await;
        }

        // Playlist invalide/inexistante : (re)créer
        info!("Album playlist {} creating/refreshing", playlist_id);

        // 1. Métadonnées album
        let album = self
            .inner
            .client
            .get_album(album_id)
            .await
            .map_err(|e| MusicSourceError::BrowseError(e.to_string()))?;

        // 2. Cache cover
        let cover_pk = if let Some(ref image_url) = album.image {
            self.inner
                .cache_manager
                .cache_cover(image_url)
                .await
                .ok()
        } else {
            None
        };

        // 3. Créer ou récupérer playlist
        let writer = if playlist_manager.exists(&playlist_id).await {
            let writer = playlist_manager
                .get_persistent_write_handle(playlist_id.clone())
                .await
                .map_err(|e| MusicSourceError::PlaylistError(e.to_string()))?;

            writer
                .flush()
                .await
                .map_err(|e| MusicSourceError::PlaylistError(e.to_string()))?;

            writer
        } else {
            playlist_manager
                .create_persistent_playlist_with_role(
                    playlist_id.clone(),
                    pmoplaylist::PlaylistRole::Album,
                )
                .await
                .map_err(|e| MusicSourceError::PlaylistError(e.to_string()))?
        };

        // 4. Métadonnées playlist
        writer
            .set_title(album.title.clone())
            .await
            .map_err(|e| MusicSourceError::PlaylistError(e.to_string()))?;

        if let Some(pk) = cover_pk {
            writer
                .set_cover_pk(Some(pk))
                .await
                .map_err(|e| MusicSourceError::PlaylistError(e.to_string()))?;
        }

        // IMPORTANT: Libérer le write lock avant d'appeler add_album_to_playlist
        drop(writer);

        // 5. Ajouter tracks (réutilise add_album_to_playlist existant)
        self.add_album_to_playlist(&playlist_id, album_id).await?;

        // 6. Récupérer items
        let reader = playlist_manager
            .get_read_handle(&playlist_id)
            .await
            .map_err(|e| MusicSourceError::PlaylistError(e.to_string()))?;

        let items = reader
            .to_items(limit)
            .await
            .map_err(|e| MusicSourceError::PlaylistError(e.to_string()))?;

        // 7. Adapter IDs
        self.adapt_playlist_items_to_qobuz(items, album_id).await
    }

    /// Increment update counter (called on catalog changes)
    async fn increment_update_id(&self) {
        let mut counter = self.inner.update_counter.write().await;
        *counter = counter.wrapping_add(1);
        let mut last = self.inner.last_change.write().await;
        *last = SystemTime::now();
    }

    /// Construit le container Discover Catalog
    fn build_discover_catalog_container(&self) -> Container {
        Container {
            id: "qobuz:discover".to_string(),
            parent_id: "qobuz".to_string(),
            restricted: Some("1".to_string()),
            child_count: None,
            searchable: Some("1".to_string()),
            title: "Discover Catalog".to_string(),
            class: "object.container".to_string(),
            containers: vec![],
            items: vec![],
        }
    }

    /// Construit le container Discover Genres
    fn build_discover_genres_container(&self) -> Container {
        Container {
            id: "qobuz:genres".to_string(),
            parent_id: "qobuz".to_string(),
            restricted: Some("1".to_string()),
            child_count: None,
            searchable: Some("1".to_string()),
            title: "Discover Genres".to_string(),
            class: "object.container".to_string(),
            containers: vec![],
            items: vec![],
        }
    }

    /// Construit le container Favourites (My Music)
    fn build_favourites_container(&self) -> Container {
        Container {
            id: "qobuz:favorites".to_string(),
            parent_id: "qobuz".to_string(),
            restricted: Some("1".to_string()),
            child_count: None,
            searchable: Some("1".to_string()),
            title: "My Music".to_string(),
            class: "object.container".to_string(),
            containers: vec![],
            items: vec![],
        }
    }

    /// Construit le container Albums favoris
    fn build_favourite_albums_container(&self) -> Container {
        Container {
            id: "qobuz:favorites:albums".to_string(),
            parent_id: "qobuz:favorites".to_string(),
            restricted: Some("1".to_string()),
            child_count: None,
            searchable: Some("1".to_string()),
            title: "Albums".to_string(),
            class: "object.container".to_string(),
            containers: vec![],
            items: vec![],
        }
    }

    /// Construit le container Tracks favoris
    fn build_favourite_tracks_container(&self) -> Container {
        Container {
            id: "qobuz:favorites:tracks".to_string(),
            parent_id: "qobuz:favorites".to_string(),
            restricted: Some("1".to_string()),
            child_count: None,
            searchable: Some("1".to_string()),
            title: "Tracks".to_string(),
            class: "object.container".to_string(),
            containers: vec![],
            items: vec![],
        }
    }

    /// Construit le container Artists favoris
    fn build_favourite_artists_container(&self) -> Container {
        Container {
            id: "qobuz:favorites:artists".to_string(),
            parent_id: "qobuz:favorites".to_string(),
            restricted: Some("1".to_string()),
            child_count: None,
            searchable: Some("1".to_string()),
            title: "Artists".to_string(),
            class: "object.container".to_string(),
            containers: vec![],
            items: vec![],
        }
    }

    /// Construit le container Playlists favoris
    fn build_favourite_playlists_container(&self) -> Container {
        Container {
            id: "qobuz:favorites:playlists".to_string(),
            parent_id: "qobuz:favorites".to_string(),
            restricted: Some("1".to_string()),
            child_count: None,
            searchable: Some("1".to_string()),
            title: "Playlists".to_string(),
            class: "object.container".to_string(),
            containers: vec![],
            items: vec![],
        }
    }

    /// Browse Favourites - retourne 4 sous-containers
    async fn browse_favourites(&self) -> Result<BrowseResult> {
        let containers = vec![
            self.build_favourite_albums_container(),
            self.build_favourite_tracks_container(),
            self.build_favourite_artists_container(),
            self.build_favourite_playlists_container(),
        ];
        Ok(BrowseResult::Containers(containers))
    }

    /// Browse Favourite Albums
    async fn browse_favourite_albums(&self) -> Result<BrowseResult> {
        let albums = self
            .inner
            .client
            .get_favorite_albums()
            .await
            .map_err(|e| MusicSourceError::BrowseError(e.to_string()))?;

        let containers: Vec<Container> = albums
            .into_iter()
            .filter_map(|album| album.to_didl_container("qobuz:favorites:albums").ok())
            .collect();

        Ok(BrowseResult::Containers(containers))
    }

    /// Browse Favourite Tracks
    async fn browse_favourite_tracks(&self) -> Result<BrowseResult> {
        let tracks = self
            .inner
            .client
            .get_favorite_tracks()
            .await
            .map_err(|e| MusicSourceError::BrowseError(e.to_string()))?;

        let items: Vec<Item> = tracks
            .into_iter()
            .filter_map(|track| track.to_didl_item("qobuz:favorites:tracks").ok())
            .collect();

        Ok(BrowseResult::Items(items))
    }

    /// Browse Favourite Artists
    async fn browse_favourite_artists(&self) -> Result<BrowseResult> {
        let artists = self
            .inner
            .client
            .get_favorite_artists()
            .await
            .map_err(|e| MusicSourceError::BrowseError(e.to_string()))?;

        let containers: Vec<Container> = artists
            .into_iter()
            .filter_map(|artist| {
                // Créer un container pour chaque artiste
                Some(Container {
                    id: format!("qobuz:artist:{}", artist.id),
                    parent_id: "qobuz:favorites:artists".to_string(),
                    restricted: Some("1".to_string()),
                    child_count: None,
                    searchable: Some("1".to_string()),
                    title: artist.name.clone(),
                    class: "object.container".to_string(),
                    containers: vec![],
                    items: vec![],
                })
            })
            .collect();

        Ok(BrowseResult::Containers(containers))
    }

    /// Browse Favourite Playlists
    async fn browse_favourite_playlists(&self) -> Result<BrowseResult> {
        let playlists = self
            .inner
            .client
            .get_user_playlists()
            .await
            .map_err(|e| MusicSourceError::BrowseError(e.to_string()))?;

        let containers: Vec<Container> = playlists
            .into_iter()
            .filter_map(|playlist| playlist.to_didl_container("qobuz:favorites:playlists").ok())
            .collect();

        Ok(BrowseResult::Containers(containers))
    }

    // ===== DISCOVER CATALOG =====

    /// Browse Discover Catalog - retourne 5 containers principaux + 10 playlists par tag
    async fn browse_discover_catalog(&self) -> Result<BrowseResult> {
        use crate::models::PlaylistTag;

        let mut containers = vec![
            self.build_discover_playlists_container(),
            self.build_discover_albums_ideal_container(),
            self.build_discover_albums_qobuzissime_container(),
            self.build_discover_albums_new_container(),
            self.build_discover_artists_container(),
        ];

        // Ajouter les 10 tags de playlists
        for tag in PlaylistTag::all() {
            containers.push(Container {
                id: format!("qobuz:discover:playlists:{}", tag.api_id()),
                parent_id: "qobuz:discover".to_string(),
                restricted: Some("1".to_string()),
                child_count: None,
                searchable: Some("1".to_string()),
                title: tag.display_name().to_string(),
                class: "object.container".to_string(),
                containers: vec![],
                items: vec![],
            });
        }

        Ok(BrowseResult::Containers(containers))
    }

    fn build_discover_playlists_container(&self) -> Container {
        Container {
            id: "qobuz:discover:playlists".to_string(),
            parent_id: "qobuz:discover".to_string(),
            restricted: Some("1".to_string()),
            child_count: None,
            searchable: Some("1".to_string()),
            title: "Playlists".to_string(),
            class: "object.container".to_string(),
            containers: vec![],
            items: vec![],
        }
    }

    fn build_discover_albums_ideal_container(&self) -> Container {
        Container {
            id: "qobuz:discover:albums:ideal".to_string(),
            parent_id: "qobuz:discover".to_string(),
            restricted: Some("1".to_string()),
            child_count: None,
            searchable: Some("1".to_string()),
            title: "Albums (Ideal Discography)".to_string(),
            class: "object.container".to_string(),
            containers: vec![],
            items: vec![],
        }
    }

    fn build_discover_albums_qobuzissime_container(&self) -> Container {
        Container {
            id: "qobuz:discover:albums:qobuzissime".to_string(),
            parent_id: "qobuz:discover".to_string(),
            restricted: Some("1".to_string()),
            child_count: None,
            searchable: Some("1".to_string()),
            title: "Albums (Qobuzissime)".to_string(),
            class: "object.container".to_string(),
            containers: vec![],
            items: vec![],
        }
    }

    fn build_discover_albums_new_container(&self) -> Container {
        Container {
            id: "qobuz:discover:albums:new".to_string(),
            parent_id: "qobuz:discover".to_string(),
            restricted: Some("1".to_string()),
            child_count: None,
            searchable: Some("1".to_string()),
            title: "Albums (New Releases)".to_string(),
            class: "object.container".to_string(),
            containers: vec![],
            items: vec![],
        }
    }

    fn build_discover_artists_container(&self) -> Container {
        Container {
            id: "qobuz:discover:artists".to_string(),
            parent_id: "qobuz:discover".to_string(),
            restricted: Some("1".to_string()),
            child_count: None,
            searchable: Some("1".to_string()),
            title: "Artists".to_string(),
            class: "object.container".to_string(),
            containers: vec![],
            items: vec![],
        }
    }

    /// Browse Discover Playlists (all featured playlists)
    async fn browse_discover_playlists(&self) -> Result<BrowseResult> {
        let playlists = self
            .inner
            .client
            .get_featured_playlists(None, None)
            .await
            .map_err(|e| MusicSourceError::BrowseError(e.to_string()))?;

        let containers: Vec<Container> = playlists
            .into_iter()
            .filter_map(|playlist| playlist.to_didl_container("qobuz:discover:playlists").ok())
            .collect();

        Ok(BrowseResult::Containers(containers))
    }

    /// Browse Discover Albums (Ideal Discography)
    async fn browse_discover_albums_ideal(&self) -> Result<BrowseResult> {
        let albums = self
            .inner
            .client
            .get_featured_albums(None, "ideal-discography")
            .await
            .map_err(|e| MusicSourceError::BrowseError(e.to_string()))?;

        let containers: Vec<Container> = albums
            .into_iter()
            .filter_map(|album| album.to_didl_container("qobuz:discover:albums:ideal").ok())
            .collect();

        Ok(BrowseResult::Containers(containers))
    }

    /// Browse Discover Albums (Qobuzissime)
    async fn browse_discover_albums_qobuzissime(&self) -> Result<BrowseResult> {
        let albums = self
            .inner
            .client
            .get_featured_albums(None, "qobuzissims")
            .await
            .map_err(|e| MusicSourceError::BrowseError(e.to_string()))?;

        let containers: Vec<Container> = albums
            .into_iter()
            .filter_map(|album| album.to_didl_container("qobuz:discover:albums:qobuzissime").ok())
            .collect();

        Ok(BrowseResult::Containers(containers))
    }

    /// Browse Discover Albums (New Releases)
    async fn browse_discover_albums_new(&self) -> Result<BrowseResult> {
        let albums = self
            .inner
            .client
            .get_featured_albums(None, "new-releases")
            .await
            .map_err(|e| MusicSourceError::BrowseError(e.to_string()))?;

        let containers: Vec<Container> = albums
            .into_iter()
            .filter_map(|album| album.to_didl_container("qobuz:discover:albums:new").ok())
            .collect();

        Ok(BrowseResult::Containers(containers))
    }

    /// Browse Discover Artists (Featured Artists)
    async fn browse_discover_artists(&self) -> Result<BrowseResult> {
        let artists = self
            .inner
            .client
            .get_featured_artists(None, None, None)
            .await
            .map_err(|e| MusicSourceError::BrowseError(e.to_string()))?;

        let containers: Vec<Container> = artists
            .into_iter()
            .map(|artist| Container {
                id: format!("qobuz:artist:{}", artist.id),
                parent_id: "qobuz:discover:artists".to_string(),
                restricted: Some("1".to_string()),
                child_count: None,
                searchable: Some("1".to_string()),
                title: artist.name.clone(),
                class: "object.container".to_string(),
                containers: vec![],
                items: vec![],
            })
            .collect();

        Ok(BrowseResult::Containers(containers))
    }

    /// Browse Discover Playlists by tag
    async fn browse_discover_playlists_tag(&self, tag: &str) -> Result<BrowseResult> {
        let playlists = self
            .inner
            .client
            .get_featured_playlists(None, Some(tag))
            .await
            .map_err(|e| MusicSourceError::BrowseError(e.to_string()))?;

        let parent_id = format!("qobuz:discover:playlists:{}", tag);
        let containers: Vec<Container> = playlists
            .into_iter()
            .filter_map(|playlist| playlist.to_didl_container(&parent_id).ok())
            .collect();

        Ok(BrowseResult::Containers(containers))
    }

    // ===== DISCOVER GENRES =====

    /// Browse Discover Genres - liste des genres
    async fn browse_discover_genres(&self) -> Result<BrowseResult> {
        let genres = self
            .inner
            .client
            .get_genres()
            .await
            .map_err(|e| MusicSourceError::BrowseError(e.to_string()))?;

        let containers: Vec<Container> = genres
            .into_iter()
            .filter_map(|genre| {
                // Filtrer les genres sans ID
                genre.id.map(|id| Container {
                    id: format!("qobuz:genre:{}", id),
                    parent_id: "qobuz:genres".to_string(),
                    restricted: Some("1".to_string()),
                    child_count: None,
                    searchable: Some("1".to_string()),
                    title: genre.name.clone(),
                    class: "object.container".to_string(),
                    containers: vec![],
                    items: vec![],
                })
            })
            .collect();

        Ok(BrowseResult::Containers(containers))
    }

    /// Browse un genre spécifique - retourne 6 sous-containers
    async fn browse_genre(&self, genre_id: &str) -> Result<BrowseResult> {
        let containers = vec![
            self.build_genre_new_releases_container(genre_id),
            self.build_genre_ideal_discography_container(genre_id),
            self.build_genre_qobuzissime_container(genre_id),
            self.build_genre_editor_picks_container(genre_id),
            self.build_genre_press_awards_container(genre_id),
            self.build_genre_playlists_container(genre_id),
        ];

        Ok(BrowseResult::Containers(containers))
    }

    fn build_genre_new_releases_container(&self, genre_id: &str) -> Container {
        Container {
            id: format!("qobuz:genre:{}:new-releases", genre_id),
            parent_id: format!("qobuz:genre:{}", genre_id),
            restricted: Some("1".to_string()),
            child_count: None,
            searchable: Some("1".to_string()),
            title: "New Releases".to_string(),
            class: "object.container".to_string(),
            containers: vec![],
            items: vec![],
        }
    }

    fn build_genre_ideal_discography_container(&self, genre_id: &str) -> Container {
        Container {
            id: format!("qobuz:genre:{}:ideal", genre_id),
            parent_id: format!("qobuz:genre:{}", genre_id),
            restricted: Some("1".to_string()),
            child_count: None,
            searchable: Some("1".to_string()),
            title: "Ideal Discography".to_string(),
            class: "object.container".to_string(),
            containers: vec![],
            items: vec![],
        }
    }

    fn build_genre_qobuzissime_container(&self, genre_id: &str) -> Container {
        Container {
            id: format!("qobuz:genre:{}:qobuzissime", genre_id),
            parent_id: format!("qobuz:genre:{}", genre_id),
            restricted: Some("1".to_string()),
            child_count: None,
            searchable: Some("1".to_string()),
            title: "Qobuzissime".to_string(),
            class: "object.container".to_string(),
            containers: vec![],
            items: vec![],
        }
    }

    fn build_genre_editor_picks_container(&self, genre_id: &str) -> Container {
        Container {
            id: format!("qobuz:genre:{}:editor-picks", genre_id),
            parent_id: format!("qobuz:genre:{}", genre_id),
            restricted: Some("1".to_string()),
            child_count: None,
            searchable: Some("1".to_string()),
            title: "Editor Picks".to_string(),
            class: "object.container".to_string(),
            containers: vec![],
            items: vec![],
        }
    }

    fn build_genre_press_awards_container(&self, genre_id: &str) -> Container {
        Container {
            id: format!("qobuz:genre:{}:press-awards", genre_id),
            parent_id: format!("qobuz:genre:{}", genre_id),
            restricted: Some("1".to_string()),
            child_count: None,
            searchable: Some("1".to_string()),
            title: "Press Awards".to_string(),
            class: "object.container".to_string(),
            containers: vec![],
            items: vec![],
        }
    }

    fn build_genre_playlists_container(&self, genre_id: &str) -> Container {
        Container {
            id: format!("qobuz:genre:{}:playlists", genre_id),
            parent_id: format!("qobuz:genre:{}", genre_id),
            restricted: Some("1".to_string()),
            child_count: None,
            searchable: Some("1".to_string()),
            title: "Qobuz Playlists".to_string(),
            class: "object.container".to_string(),
            containers: vec![],
            items: vec![],
        }
    }

    /// Browse Genre New Releases
    async fn browse_genre_new_releases(&self, genre_id: &str) -> Result<BrowseResult> {
        let albums = self
            .inner
            .client
            .get_featured_albums(Some(genre_id), "new-releases")
            .await
            .map_err(|e| MusicSourceError::BrowseError(e.to_string()))?;

        let parent_id = format!("qobuz:genre:{}:new-releases", genre_id);
        let containers: Vec<Container> = albums
            .into_iter()
            .filter_map(|album| album.to_didl_container(&parent_id).ok())
            .collect();

        Ok(BrowseResult::Containers(containers))
    }

    /// Browse Genre Ideal Discography
    async fn browse_genre_ideal_discography(&self, genre_id: &str) -> Result<BrowseResult> {
        let albums = self
            .inner
            .client
            .get_featured_albums(Some(genre_id), "ideal-discography")
            .await
            .map_err(|e| MusicSourceError::BrowseError(e.to_string()))?;

        let parent_id = format!("qobuz:genre:{}:ideal", genre_id);
        let containers: Vec<Container> = albums
            .into_iter()
            .filter_map(|album| album.to_didl_container(&parent_id).ok())
            .collect();

        Ok(BrowseResult::Containers(containers))
    }

    /// Browse Genre Qobuzissime
    async fn browse_genre_qobuzissime(&self, genre_id: &str) -> Result<BrowseResult> {
        let albums = self
            .inner
            .client
            .get_featured_albums(Some(genre_id), "qobuzissims")
            .await
            .map_err(|e| MusicSourceError::BrowseError(e.to_string()))?;

        let parent_id = format!("qobuz:genre:{}:qobuzissime", genre_id);
        let containers: Vec<Container> = albums
            .into_iter()
            .filter_map(|album| album.to_didl_container(&parent_id).ok())
            .collect();

        Ok(BrowseResult::Containers(containers))
    }

    /// Browse Genre Editor Picks
    async fn browse_genre_editor_picks(&self, genre_id: &str) -> Result<BrowseResult> {
        let albums = self
            .inner
            .client
            .get_featured_albums(Some(genre_id), "editor-picks")
            .await
            .map_err(|e| MusicSourceError::BrowseError(e.to_string()))?;

        let parent_id = format!("qobuz:genre:{}:editor-picks", genre_id);
        let containers: Vec<Container> = albums
            .into_iter()
            .filter_map(|album| album.to_didl_container(&parent_id).ok())
            .collect();

        Ok(BrowseResult::Containers(containers))
    }

    /// Browse Genre Press Awards
    async fn browse_genre_press_awards(&self, genre_id: &str) -> Result<BrowseResult> {
        let albums = self
            .inner
            .client
            .get_featured_albums(Some(genre_id), "press-awards")
            .await
            .map_err(|e| MusicSourceError::BrowseError(e.to_string()))?;

        let parent_id = format!("qobuz:genre:{}:press-awards", genre_id);
        let containers: Vec<Container> = albums
            .into_iter()
            .filter_map(|album| album.to_didl_container(&parent_id).ok())
            .collect();

        Ok(BrowseResult::Containers(containers))
    }

    /// Browse Genre Playlists
    async fn browse_genre_playlists(&self, genre_id: &str) -> Result<BrowseResult> {
        let playlists = self
            .inner
            .client
            .get_featured_playlists(Some(genre_id), None)
            .await
            .map_err(|e| MusicSourceError::BrowseError(e.to_string()))?;

        let parent_id = format!("qobuz:genre:{}:playlists", genre_id);
        let containers: Vec<Container> = playlists
            .into_iter()
            .filter_map(|playlist| playlist.to_didl_container(&parent_id).ok())
            .collect();

        Ok(BrowseResult::Containers(containers))
    }

    /// Parse object_id to determine what to browse
    ///
    /// Object IDs follow these patterns:
    /// - "qobuz" or "0" → Root container
    /// - "qobuz:discover" → Discover Catalog
    /// - "qobuz:genres" → Discover Genres
    /// - "qobuz:favorites" → My Music
    /// - "qobuz:album:{id}" → Tracks in album
    /// - "qobuz:playlist:{id}" → Tracks in playlist
    /// - etc.
    fn parse_object_id(&self, object_id: &str) -> ObjectIdType {
        if object_id == "qobuz" || object_id == "0" {
            return ObjectIdType::Root;
        }

        let parts: Vec<&str> = object_id.split(':').collect();
        match parts.as_slice() {
            // Discover Catalog
            ["qobuz", "discover"] => ObjectIdType::DiscoverCatalog,
            ["qobuz", "discover", "playlists"] => ObjectIdType::DiscoverPlaylists,
            ["qobuz", "discover", "albums", "ideal"] => ObjectIdType::DiscoverAlbumsIdeal,
            ["qobuz", "discover", "albums", "qobuzissime"] => ObjectIdType::DiscoverAlbumsQobuzissime,
            ["qobuz", "discover", "albums", "new"] => ObjectIdType::DiscoverAlbumsNew,
            ["qobuz", "discover", "artists"] => ObjectIdType::DiscoverArtists,
            ["qobuz", "discover", "playlists", tag] => ObjectIdType::DiscoverPlaylistsByTag(tag.to_string()),

            // Discover Genres
            ["qobuz", "genres"] => ObjectIdType::DiscoverGenres,
            ["qobuz", "genre", id] => ObjectIdType::GenreRoot(id.to_string()),
            ["qobuz", "genre", id, "new-releases"] => ObjectIdType::GenreNewReleases(id.to_string()),
            ["qobuz", "genre", id, "ideal"] => ObjectIdType::GenreIdealDiscography(id.to_string()),
            ["qobuz", "genre", id, "qobuzissime"] => ObjectIdType::GenreQobuzissime(id.to_string()),
            ["qobuz", "genre", id, "editor-picks"] => ObjectIdType::GenreEditorPicks(id.to_string()),
            ["qobuz", "genre", id, "press-awards"] => ObjectIdType::GenrePressAwards(id.to_string()),
            ["qobuz", "genre", id, "playlists"] => ObjectIdType::GenrePlaylists(id.to_string()),

            // Favourites
            ["qobuz", "favorites"] => ObjectIdType::Favourites,
            ["qobuz", "favorites", "albums"] => ObjectIdType::FavouriteAlbums,
            ["qobuz", "favorites", "tracks"] => ObjectIdType::FavouriteTracks,
            ["qobuz", "favorites", "artists"] => ObjectIdType::FavouriteArtists,
            ["qobuz", "favorites", "playlists"] => ObjectIdType::FavouritePlaylists,

            // Items (existant)
            ["qobuz", "album", id] => ObjectIdType::Album(id.to_string()),
            ["qobuz", "playlist", id] => ObjectIdType::Playlist(id.to_string()),
            ["qobuz", "artist", id] => ObjectIdType::Artist(id.to_string()),
            ["qobuz", "track", id] => ObjectIdType::Track(id.to_string()),

            _ => ObjectIdType::Unknown,
        }
    }
}

#[derive(Debug)]
enum ObjectIdType {
    Root,

    // Discover Catalog
    DiscoverCatalog,
    DiscoverPlaylists,
    DiscoverAlbumsIdeal,
    DiscoverAlbumsQobuzissime,
    DiscoverAlbumsNew,
    DiscoverArtists,
    DiscoverPlaylistsByTag(String), // tag

    // Discover Genres
    DiscoverGenres,
    GenreRoot(String),               // genre_id
    GenreNewReleases(String),        // genre_id
    GenreIdealDiscography(String),   // genre_id
    GenreQobuzissime(String),        // genre_id
    GenreEditorPicks(String),        // genre_id
    GenrePressAwards(String),        // genre_id
    GenrePlaylists(String),          // genre_id

    // Favourites
    Favourites,
    FavouriteAlbums,
    FavouriteTracks,
    FavouriteArtists,
    FavouritePlaylists,

    // Items (existant)
    Album(String),
    Playlist(String),
    Artist(String),
    Track(String),

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
        // Create the root container with 3 main branches
        Ok(Container {
            id: "qobuz".to_string(),
            parent_id: "0".to_string(),
            restricted: Some("1".to_string()),
            child_count: Some("3".to_string()),
            searchable: Some("1".to_string()),
            title: "Qobuz".to_string(),
            class: "object.container".to_string(),
            containers: vec![
                self.build_discover_catalog_container(),
                self.build_discover_genres_container(),
                self.build_favourites_container(),
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

            // Discover Catalog
            ObjectIdType::DiscoverCatalog => self.browse_discover_catalog().await,
            ObjectIdType::DiscoverPlaylists => self.browse_discover_playlists().await,
            ObjectIdType::DiscoverAlbumsIdeal => self.browse_discover_albums_ideal().await,
            ObjectIdType::DiscoverAlbumsQobuzissime => self.browse_discover_albums_qobuzissime().await,
            ObjectIdType::DiscoverAlbumsNew => self.browse_discover_albums_new().await,
            ObjectIdType::DiscoverArtists => self.browse_discover_artists().await,
            ObjectIdType::DiscoverPlaylistsByTag(tag) => self.browse_discover_playlists_tag(&tag).await,

            // Discover Genres
            ObjectIdType::DiscoverGenres => self.browse_discover_genres().await,
            ObjectIdType::GenreRoot(id) => self.browse_genre(&id).await,
            ObjectIdType::GenreNewReleases(id) => self.browse_genre_new_releases(&id).await,
            ObjectIdType::GenreIdealDiscography(id) => self.browse_genre_ideal_discography(&id).await,
            ObjectIdType::GenreQobuzissime(id) => self.browse_genre_qobuzissime(&id).await,
            ObjectIdType::GenreEditorPicks(id) => self.browse_genre_editor_picks(&id).await,
            ObjectIdType::GenrePressAwards(id) => self.browse_genre_press_awards(&id).await,
            ObjectIdType::GenrePlaylists(id) => self.browse_genre_playlists(&id).await,

            // Favourites
            ObjectIdType::Favourites => self.browse_favourites().await,
            ObjectIdType::FavouriteAlbums => self.browse_favourite_albums().await,
            ObjectIdType::FavouriteTracks => self.browse_favourite_tracks().await,
            ObjectIdType::FavouriteArtists => self.browse_favourite_artists().await,
            ObjectIdType::FavouritePlaylists => self.browse_favourite_playlists().await,

            // Items (existant)
            ObjectIdType::Album(album_id) => {
                let items = self
                    .get_or_create_album_playlist_items(&album_id, usize::MAX)
                    .await?;

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

            ObjectIdType::Track(_) => {
                // Track object_ids ne sont pas browsables, retourner une erreur
                Err(MusicSourceError::NotSupported(
                    "Tracks are not browsable containers".to_string(),
                ))
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
                let all_items = self
                    .get_or_create_album_playlist_items(&album_id, usize::MAX)
                    .await?;

                let items: Vec<Item> = all_items
                    .into_iter()
                    .skip(offset)
                    .take(limit)
                    .collect();

                Ok(BrowseResult::Items(items))
            }
            ObjectIdType::FavouriteAlbums => {
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
                    .filter_map(|album| album.to_didl_container("qobuz:favorites:albums").ok())
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
