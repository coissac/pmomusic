//! Music source implementation for Qobuz
//!
//! This module implements the [`pmosource::MusicSource`] trait for Qobuz,
//! providing a complete music catalog browsing and searching experience.

use crate::client::QobuzClient;
use crate::didl::ToDIDL;
use crate::models::{Album, Track};
use pmosource::{async_trait, BrowseResult, MusicSource, MusicSourceError, Result};
use pmodidl::{Container, Item};
use std::sync::Arc;
use std::time::SystemTime;
use tokio::sync::RwLock;

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

    /// Update tracking
    update_counter: RwLock<u32>,
    last_change: RwLock<SystemTime>,
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
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use pmoqobuz::{QobuzSource, QobuzClient};
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     let client = QobuzClient::from_config().await?;
    ///     let source = QobuzSource::new(client);
    ///     Ok(())
    /// }
    /// ```
    pub fn new(client: QobuzClient) -> Self {
        Self {
            inner: Arc::new(QobuzSourceInner {
                client,
                update_counter: RwLock::new(0),
                last_change: RwLock::new(SystemTime::now()),
            }),
        }
    }

    /// Get the Qobuz client
    pub fn client(&self) -> &QobuzClient {
        &self.inner.client
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
        // Extract track ID from object_id
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
