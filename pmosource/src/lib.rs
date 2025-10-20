//! # PMOSource
//!
//! Common traits and types for PMOMusic sources.
//!
//! This crate provides the foundational abstractions for different music sources
//! in the PMOMusic ecosystem, such as Radio Paradise, Qobuz, etc.
//!
//! ## Features
//!
//! - **FIFO Support**: Dynamic audio sources using `pmoplaylist` for streaming.
//! - **Container/Item Navigation**: Browse and search using DIDL-Lite format (`pmodidl`).
//! - **Cache Integration**: Automatic URI resolution with `pmoaudiocache` and `pmocovers`.
//! - **Change Tracking**: `update_id` and `last_change` for UPnP notifications.
//! - **Send + Sync**: Ready for async servers.
//! - **Server Extension**: Optional `pmoserver` integration with REST API (feature `server`).
//!
//! ## Usage
//!
//! ### Basic Usage (implementing a source)
//!
//! See the [examples/radio_paradise.rs](../examples/radio_paradise.rs) for a complete implementation.
//!
//! ### Server Integration (feature `server`)
//!
//! ```rust,ignore
//! use pmosource::MusicSourceExt;
//! use pmoserver::ServerBuilder;
//!
//! let mut server = ServerBuilder::new_configured().build();
//!
//! // Initialiser le système de sources
//! server.init_music_sources().await?;
//!
//! // Enregistrer des sources
//! server.register_music_source(Arc::new(my_source)).await;
//! ```

pub mod cache;

use pmodidl::{Container, Item};
use std::fmt::Debug;
use std::time::SystemTime;

/// Standard size for default images (300x300 pixels)
pub const DEFAULT_IMAGE_SIZE: u32 = 300;

/// Error types for music source operations
#[derive(Debug, thiserror::Error)]
pub enum MusicSourceError {
    #[error("Failed to load default image: {0}")]
    ImageLoadError(String),

    #[error("Invalid image format: {0}")]
    InvalidImageFormat(String),

    #[error("Source not available: {0}")]
    SourceUnavailable(String),

    #[error("Object not found: {0}")]
    ObjectNotFound(String),

    #[error("Browse error: {0}")]
    BrowseError(String),

    #[error("Search not supported")]
    SearchNotSupported,

    #[error("FIFO not supported")]
    FifoNotSupported,

    #[error("Cache error: {0}")]
    CacheError(String),

    #[error("URI resolution failed: {0}")]
    UriResolutionError(String),

    #[error("Feature not supported: {0}")]
    NotSupported(String),

    #[error("Favorites operation failed: {0}")]
    FavoritesError(String),

    #[error("Playlist operation failed: {0}")]
    PlaylistError(String),
}

/// Result type for music source operations
pub type Result<T> = std::result::Result<T, MusicSourceError>;

/// Source capabilities describing what features are supported
#[derive(Debug, Clone, Default)]
pub struct SourceCapabilities {
    /// Supports FIFO operations (dynamic playlists)
    pub supports_fifo: bool,
    /// Supports search functionality
    pub supports_search: bool,
    /// Supports user favorites
    pub supports_favorites: bool,
    /// Supports user playlists
    pub supports_playlists: bool,
    /// Supports user-created content
    pub supports_user_content: bool,
    /// Supports high-resolution audio
    pub supports_high_res_audio: bool,
    /// Maximum sample rate supported (Hz)
    pub max_sample_rate: Option<u32>,
    /// Supports multiple audio formats
    pub supports_multiple_formats: bool,
    /// Supports advanced search with filters
    pub supports_advanced_search: bool,
    /// Supports pagination in browse operations
    pub supports_pagination: bool,
}

/// Audio format information
#[derive(Debug, Clone)]
pub struct AudioFormat {
    /// Format identifier (e.g., "flac-24-96", "mp3-320")
    pub format_id: String,
    /// MIME type (e.g., "audio/flac", "audio/mpeg")
    pub mime_type: String,
    /// Sample rate in Hz (e.g., 44100, 96000)
    pub sample_rate: Option<u32>,
    /// Bit depth (e.g., 16, 24)
    pub bit_depth: Option<u8>,
    /// Bitrate in kbps (for lossy formats)
    pub bitrate: Option<u32>,
    /// Number of audio channels (e.g., 2 for stereo)
    pub channels: Option<u8>,
}

impl Default for AudioFormat {
    fn default() -> Self {
        Self {
            format_id: "default".to_string(),
            mime_type: "audio/flac".to_string(),
            sample_rate: Some(44100),
            bit_depth: Some(16),
            bitrate: None,
            channels: Some(2),
        }
    }
}

/// Cache status for an item
#[derive(Debug, Clone)]
pub enum CacheStatus {
    /// Item is not cached
    NotCached,
    /// Item is currently being cached
    Caching { progress: f32 },
    /// Item is fully cached
    Cached { size_bytes: u64 },
    /// Caching failed
    Failed { error: String },
}

/// Search filters for advanced search
#[derive(Debug, Clone, Default)]
pub struct SearchFilters {
    /// Filter by artist name
    pub artist: Option<String>,
    /// Filter by album name
    pub album: Option<String>,
    /// Filter by genre
    pub genre: Option<String>,
    /// Minimum year
    pub year_min: Option<u32>,
    /// Maximum year
    pub year_max: Option<u32>,
    /// Maximum number of results
    pub limit: Option<usize>,
}

/// Source statistics
#[derive(Debug, Clone, Default)]
pub struct SourceStatistics {
    /// Total number of items in the source
    pub total_items: Option<usize>,
    /// Total number of containers in the source
    pub total_containers: Option<usize>,
    /// Number of cached items
    pub cached_items: Option<usize>,
    /// Total cache size in bytes
    pub cache_size_bytes: Option<u64>,
}

/// Result of a browse operation
#[derive(Debug, Clone)]
pub enum BrowseResult {
    /// List of sub-containers only
    Containers(Vec<Container>),

    /// List of items only
    Items(Vec<Item>),

    /// Mixed: both containers and items
    Mixed {
        containers: Vec<Container>,
        items: Vec<Item>,
    },
}

impl BrowseResult {
    /// Returns the total count of objects (containers + items)
    pub fn count(&self) -> usize {
        match self {
            BrowseResult::Containers(c) => c.len(),
            BrowseResult::Items(i) => i.len(),
            BrowseResult::Mixed { containers, items } => containers.len() + items.len(),
        }
    }

    /// Returns all containers
    pub fn containers(&self) -> &[Container] {
        match self {
            BrowseResult::Containers(c) => c,
            BrowseResult::Items(_) => &[],
            BrowseResult::Mixed { containers, .. } => containers,
        }
    }

    /// Returns all items
    pub fn items(&self) -> &[Item] {
        match self {
            BrowseResult::Containers(_) => &[],
            BrowseResult::Items(i) => i,
            BrowseResult::Mixed { items, .. } => items,
        }
    }
}

/// Main trait for music sources
///
/// This trait defines the common interface that all music sources must implement.
/// It provides methods for:
/// - Getting the source name and identification
/// - Retrieving default images/logos
/// - Browsing containers and items (ContentDirectory)
/// - Resolving audio URIs (using caches when available)
/// - Managing FIFO playlists for dynamic sources
/// - Tracking changes via `update_id` and `last_change`
///
/// # Thread Safety
///
/// All implementations must be `Send + Sync` for use in async servers.
///
/// # Examples
///
/// ```rust,no_run
/// use pmosource::{MusicSource, BrowseResult, Result};
/// use pmodidl::{Container, Item};
/// use std::time::SystemTime;
///
/// #[derive(Debug)]
/// struct RadioParadise {
///     // implementation details
/// }
///
/// #[async_trait::async_trait]
/// impl MusicSource for RadioParadise {
///     fn name(&self) -> &str {
///         "Radio Paradise"
///     }
///
///     fn id(&self) -> &str {
///         "radio-paradise"
///     }
///
///     fn default_image(&self) -> &[u8] {
///         // WebP image bytes
///         &[]
///     }
///
///     async fn root_container(&self) -> Result<Container> {
///         Ok(Container {
///             id: "0".to_string(),
///             parent_id: "-1".to_string(),
///             restricted: Some("1".to_string()),
///             child_count: Some("0".to_string()),
///             title: "Radio Paradise".to_string(),
///             class: "object.container".to_string(),
///             containers: vec![],
///             items: vec![],
///         })
///     }
///
///     async fn browse(&self, object_id: &str) -> Result<BrowseResult> {
///         // Browse implementation
///         Ok(BrowseResult::Items(vec![]))
///     }
///
///     async fn resolve_uri(&self, object_id: &str) -> Result<String> {
///         // Return cached URI or original URI
///         Ok("http://example.com/track.mp3".to_string())
///     }
///
///     fn supports_fifo(&self) -> bool {
///         true
///     }
///
///     async fn append_track(&self, track: Item) -> Result<()> {
///         // Add track to FIFO
///         Ok(())
///     }
///
///     async fn remove_oldest(&self) -> Result<Option<Item>> {
///         // Remove oldest track
///         Ok(None)
///     }
///
///     async fn update_id(&self) -> u32 {
///         0
///     }
///
///     async fn last_change(&self) -> Option<SystemTime> {
///         None
///     }
///
///     async fn get_items(&self, offset: usize, count: usize) -> Result<Vec<Item>> {
///         Ok(vec![])
///     }
///
///     async fn search(&self, query: &str) -> Result<BrowseResult> {
///         Err(pmosource::MusicSourceError::SearchNotSupported)
///     }
/// }
/// ```
#[async_trait::async_trait]
pub trait MusicSource: Debug + Send + Sync {
    // ============= Basic Information =============

    /// Returns the human-readable name of the music source
    ///
    /// # Examples
    ///
    /// ```ignore
    /// assert_eq!(source.name(), "Radio Paradise");
    /// ```
    fn name(&self) -> &str;

    /// Returns a unique identifier for the music source
    ///
    /// This is typically a lowercase, hyphenated version of the name
    /// suitable for use in URLs, file names, etc.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// assert_eq!(source.id(), "radio-paradise");
    /// ```
    fn id(&self) -> &str;

    /// Returns the default image/logo for this source as WebP bytes
    ///
    /// The image should be square (300x300 pixels) and in WebP format.
    /// This is embedded in the binary for offline availability.
    ///
    /// # Returns
    ///
    /// A byte slice containing the WebP-encoded image data
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let image_data = source.default_image();
    /// assert!(image_data.len() > 0);
    /// ```
    fn default_image(&self) -> &[u8];

    /// Returns the MIME type of the default image
    ///
    /// By default, this returns "image/webp" since all default images
    /// should be in WebP format.
    fn default_image_mime_type(&self) -> &str {
        "image/webp"
    }

    // ============= ContentDirectory Navigation =============

    /// Returns the root container for this source
    ///
    /// This container is exposed at the top level of the ContentDirectory.
    /// Its `id` should be unique across all sources, typically the source id.
    ///
    /// # Returns
    ///
    /// A `Container` representing the root of this source's hierarchy.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let root = source.root_container().await?;
    /// assert_eq!(root.id, "radio-paradise");
    /// assert_eq!(root.title, "Radio Paradise");
    /// ```
    async fn root_container(&self) -> Result<Container>;

    /// Browse a container or item by its object_id
    ///
    /// Returns the children of the specified container, or an error if the
    /// object doesn't exist or isn't browsable.
    ///
    /// # Arguments
    ///
    /// * `object_id` - The ID of the container to browse
    ///
    /// # Returns
    ///
    /// A `BrowseResult` containing sub-containers and/or items.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let result = source.browse("radio-paradise").await?;
    /// for item in result.items() {
    ///     println!("Track: {}", item.title);
    /// }
    /// ```
    async fn browse(&self, object_id: &str) -> Result<BrowseResult>;

    /// Resolve the actual URI for a track
    ///
    /// This method should return the URI that can be used to stream/download
    /// the audio. If the track is cached (via `pmoaudiocache`), return the
    /// cached URI. Otherwise, return the original URI.
    ///
    /// # Arguments
    ///
    /// * `object_id` - The ID of the track to resolve
    ///
    /// # Returns
    ///
    /// The HTTP URI to access the audio file.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let uri = source.resolve_uri("track-123").await?;
    /// // Returns something like: "http://localhost:8080/cache/audio/abc123"
    /// // or the original URL if not cached
    /// ```
    async fn resolve_uri(&self, object_id: &str) -> Result<String>;

    // ============= FIFO Support =============

    /// Indicates whether this source supports FIFO operations
    ///
    /// Dynamic sources (like radios) typically return `true`, while
    /// static sources (like albums) return `false`.
    ///
    /// # Returns
    ///
    /// `true` if the source supports FIFO operations, `false` otherwise.
    fn supports_fifo(&self) -> bool;

    /// Append a track to the FIFO
    ///
    /// This method is only applicable for sources that support FIFO.
    /// It adds the track to the end of the queue, potentially removing
    /// the oldest track if capacity is reached.
    ///
    /// Updates `update_id` and `last_change`.
    ///
    /// # Arguments
    ///
    /// * `track` - The `Item` to add to the FIFO
    ///
    /// # Errors
    ///
    /// Returns `MusicSourceError::FifoNotSupported` if the source doesn't
    /// support FIFO operations.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let track = Item {
    ///     id: "track-1".to_string(),
    ///     title: "Song Title".to_string(),
    ///     // ... other fields
    /// };
    /// source.append_track(track).await?;
    /// ```
    async fn append_track(&self, track: Item) -> Result<()>;

    /// Remove the oldest track from the FIFO
    ///
    /// This method is only applicable for sources that support FIFO.
    /// Updates `update_id` and `last_change` if a track is removed.
    ///
    /// # Returns
    ///
    /// The removed track, or `None` if the FIFO is empty.
    ///
    /// # Errors
    ///
    /// Returns `MusicSourceError::FifoNotSupported` if the source doesn't
    /// support FIFO operations.
    async fn remove_oldest(&self) -> Result<Option<Item>>;

    // ============= Change Tracking =============

    /// Returns the current update_id
    ///
    /// This counter is incremented each time the source's content changes
    /// (track added, removed, metadata updated, etc.). It's used by UPnP
    /// Control Points to detect changes and refresh their view.
    ///
    /// # Returns
    ///
    /// The current update_id value. Wraps around on overflow.
    async fn update_id(&self) -> u32;

    /// Returns the timestamp of the last change
    ///
    /// This is used to notify MediaRenderers and Control Points about
    /// content updates.
    ///
    /// # Returns
    ///
    /// The `SystemTime` of the last modification, or `None` if never modified.
    async fn last_change(&self) -> Option<SystemTime>;

    // ============= Pagination & Search =============

    /// Get a paginated list of items
    ///
    /// This is useful for browsing large collections without loading
    /// everything into memory.
    ///
    /// # Arguments
    ///
    /// * `offset` - Starting index (0-based)
    /// * `count` - Maximum number of items to return
    ///
    /// # Returns
    ///
    /// A vector of `Item` objects, potentially empty if offset is out of range.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// // Get items 10-19
    /// let items = source.get_items(10, 10).await?;
    /// ```
    async fn get_items(&self, offset: usize, count: usize) -> Result<Vec<Item>>;

    /// Search for tracks matching a query
    ///
    /// This is an optional feature. Sources that don't support search
    /// should return `MusicSourceError::SearchNotSupported`.
    ///
    /// # Arguments
    ///
    /// * `query` - Search query string
    ///
    /// # Returns
    ///
    /// A `BrowseResult` containing matching items/containers.
    ///
    /// # Errors
    ///
    /// Returns `MusicSourceError::SearchNotSupported` if not implemented.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let results = source.search("Pink Floyd").await?;
    /// for item in results.items() {
    ///     println!("Found: {}", item.title);
    /// }
    /// ```
    async fn search(&self, query: &str) -> Result<BrowseResult> {
        let _ = query;
        Err(MusicSourceError::SearchNotSupported)
    }

    // ============= Extended Features =============

    /// Returns the capabilities of this music source
    ///
    /// This allows clients to discover what features are supported without
    /// having to call methods and handle errors.
    ///
    /// # Returns
    ///
    /// A `SourceCapabilities` struct describing supported features.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let caps = source.capabilities();
    /// if caps.supports_search {
    ///     let results = source.search("query").await?;
    /// }
    /// ```
    fn capabilities(&self) -> SourceCapabilities {
        SourceCapabilities {
            supports_fifo: self.supports_fifo(),
            supports_search: false,
            supports_favorites: false,
            supports_playlists: false,
            supports_user_content: false,
            supports_high_res_audio: false,
            max_sample_rate: None,
            supports_multiple_formats: false,
            supports_advanced_search: false,
            supports_pagination: false,
        }
    }

    /// Get available audio formats for a specific track
    ///
    /// Some sources (like Qobuz) offer multiple quality levels and formats.
    /// This method returns all available formats for a given track.
    ///
    /// # Arguments
    ///
    /// * `object_id` - The ID of the track
    ///
    /// # Returns
    ///
    /// A vector of available audio formats, or a single default format.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let formats = source.get_available_formats("track-123").await?;
    /// for format in formats {
    ///     println!("{}: {} Hz, {} bit", format.format_id,
    ///              format.sample_rate.unwrap_or(0),
    ///              format.bit_depth.unwrap_or(0));
    /// }
    /// ```
    async fn get_available_formats(&self, object_id: &str) -> Result<Vec<AudioFormat>> {
        let _ = object_id;
        Ok(vec![AudioFormat::default()])
    }

    /// Get the cache status for a specific item
    ///
    /// Returns information about whether an item is cached, being cached,
    /// or not cached at all.
    ///
    /// # Arguments
    ///
    /// * `object_id` - The ID of the item to check
    ///
    /// # Returns
    ///
    /// The current cache status of the item.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let status = source.get_cache_status("track-123").await?;
    /// match status {
    ///     CacheStatus::Cached { size_bytes } => {
    ///         println!("Cached: {} bytes", size_bytes);
    ///     }
    ///     CacheStatus::NotCached => {
    ///         println!("Not cached");
    ///     }
    ///     _ => {}
    /// }
    /// ```
    async fn get_cache_status(&self, object_id: &str) -> Result<CacheStatus> {
        let _ = object_id;
        Ok(CacheStatus::NotCached)
    }

    /// Request caching of a specific item
    ///
    /// Initiates asynchronous caching of an item (audio and/or cover art).
    /// The operation happens in the background.
    ///
    /// # Arguments
    ///
    /// * `object_id` - The ID of the item to cache
    ///
    /// # Returns
    ///
    /// The initial cache status after the request.
    ///
    /// # Errors
    ///
    /// Returns `MusicSourceError::NotSupported` if caching is not available.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let status = source.cache_item("track-123").await?;
    /// ```
    async fn cache_item(&self, object_id: &str) -> Result<CacheStatus> {
        let _ = object_id;
        Err(MusicSourceError::NotSupported(
            "Caching not supported".to_string(),
        ))
    }

    /// Add an item to favorites
    ///
    /// Marks an item (track, album, artist, etc.) as a favorite.
    ///
    /// # Arguments
    ///
    /// * `object_id` - The ID of the item to favorite
    ///
    /// # Errors
    ///
    /// Returns `MusicSourceError::NotSupported` if favorites are not available.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// source.add_favorite("album-123").await?;
    /// ```
    async fn add_favorite(&self, object_id: &str) -> Result<()> {
        let _ = object_id;
        Err(MusicSourceError::NotSupported(
            "Favorites not supported".to_string(),
        ))
    }

    /// Remove an item from favorites
    ///
    /// Unmarks an item as a favorite.
    ///
    /// # Arguments
    ///
    /// * `object_id` - The ID of the item to unfavorite
    ///
    /// # Errors
    ///
    /// Returns `MusicSourceError::NotSupported` if favorites are not available.
    async fn remove_favorite(&self, object_id: &str) -> Result<()> {
        let _ = object_id;
        Err(MusicSourceError::NotSupported(
            "Favorites not supported".to_string(),
        ))
    }

    /// Check if an item is in favorites
    ///
    /// # Arguments
    ///
    /// * `object_id` - The ID of the item to check
    ///
    /// # Returns
    ///
    /// `true` if the item is favorited, `false` otherwise.
    ///
    /// # Errors
    ///
    /// Returns `MusicSourceError::NotSupported` if favorites are not available.
    async fn is_favorite(&self, object_id: &str) -> Result<bool> {
        let _ = object_id;
        Err(MusicSourceError::NotSupported(
            "Favorites not supported".to_string(),
        ))
    }

    /// Get user playlists
    ///
    /// Returns all playlists created or followed by the user.
    ///
    /// # Returns
    ///
    /// A vector of Container objects representing playlists.
    ///
    /// # Errors
    ///
    /// Returns `MusicSourceError::NotSupported` if playlists are not available.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let playlists = source.get_user_playlists().await?;
    /// for playlist in playlists {
    ///     println!("Playlist: {}", playlist.title);
    /// }
    /// ```
    async fn get_user_playlists(&self) -> Result<Vec<Container>> {
        Err(MusicSourceError::NotSupported(
            "Playlists not supported".to_string(),
        ))
    }

    /// Add an item to a playlist
    ///
    /// # Arguments
    ///
    /// * `playlist_id` - The ID of the playlist
    /// * `item_id` - The ID of the item to add
    ///
    /// # Errors
    ///
    /// Returns `MusicSourceError::NotSupported` if playlists are not available.
    async fn add_to_playlist(&self, playlist_id: &str, item_id: &str) -> Result<()> {
        let _ = (playlist_id, item_id);
        Err(MusicSourceError::NotSupported(
            "Playlists not supported".to_string(),
        ))
    }

    /// Get total item count for a container
    ///
    /// This is more efficient than browsing and counting for large collections.
    ///
    /// # Arguments
    ///
    /// * `object_id` - The ID of the container
    ///
    /// # Returns
    ///
    /// The total number of items in the container.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let count = source.get_item_count("album-123").await?;
    /// println!("Album has {} tracks", count);
    /// ```
    async fn get_item_count(&self, object_id: &str) -> Result<usize> {
        // Default implementation: browse and count
        let result = self.browse(object_id).await?;
        Ok(result.count())
    }

    /// Browse with pagination support
    ///
    /// More efficient than `browse()` for large containers.
    ///
    /// # Arguments
    ///
    /// * `object_id` - The ID of the container to browse
    /// * `offset` - Starting index (0-based)
    /// * `limit` - Maximum number of items to return
    ///
    /// # Returns
    ///
    /// A `BrowseResult` containing the requested subset of items.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// // Get items 10-19
    /// let result = source.browse_paginated("album-123", 10, 10).await?;
    /// ```
    async fn browse_paginated(
        &self,
        object_id: &str,
        offset: usize,
        limit: usize,
    ) -> Result<BrowseResult> {
        // Default implementation: browse all then slice (inefficient)
        let _ = (offset, limit);
        self.browse(object_id).await
    }

    /// Advanced search with filters
    ///
    /// Provides more fine-grained search control than basic `search()`.
    ///
    /// # Arguments
    ///
    /// * `query` - Search query string
    /// * `filters` - Additional search filters
    ///
    /// # Returns
    ///
    /// A `BrowseResult` containing matching items/containers.
    ///
    /// # Errors
    ///
    /// Returns `MusicSourceError::SearchNotSupported` if not implemented.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let filters = SearchFilters {
    ///     artist: Some("Pink Floyd".to_string()),
    ///     year_min: Some(1970),
    ///     year_max: Some(1980),
    ///     ..Default::default()
    /// };
    /// let results = source.search_advanced("Wall", filters).await?;
    /// ```
    async fn search_advanced(&self, query: &str, filters: SearchFilters) -> Result<BrowseResult> {
        // Default: ignore filters and call basic search
        let _ = filters;
        self.search(query).await
    }

    /// Get source statistics
    ///
    /// Returns information about the source such as total items, cache usage, etc.
    ///
    /// # Returns
    ///
    /// A `SourceStatistics` struct with available statistics.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let stats = source.statistics().await?;
    /// if let Some(total) = stats.total_items {
    ///     println!("Total tracks: {}", total);
    /// }
    /// ```
    async fn statistics(&self) -> Result<SourceStatistics> {
        Ok(SourceStatistics::default())
    }
}

// Re-export commonly used types
pub use async_trait::async_trait;
pub use pmodidl;
pub use pmoplaylist;

// Re-export cache types
pub use cache::{CacheStatistics, SourceCacheManager, TrackMetadata};

// Server extension modules (feature-gated)
#[cfg(feature = "server")]
pub mod pmoserver_ext;

#[cfg(feature = "server")]
pub mod api;

#[cfg(feature = "server")]
mod pmoserver_impl;

// Re-export server extension trait
#[cfg(feature = "server")]
pub use pmoserver_ext::MusicSourceExt;

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct TestSource;

    #[async_trait]
    impl MusicSource for TestSource {
        fn name(&self) -> &str {
            "Test Source"
        }

        fn id(&self) -> &str {
            "test-source"
        }

        fn default_image(&self) -> &[u8] {
            &[]
        }

        async fn root_container(&self) -> Result<Container> {
            Ok(Container {
                id: "test-source".to_string(),
                parent_id: "0".to_string(),
                restricted: Some("1".to_string()),
                child_count: Some("0".to_string()),
                title: "Test Source".to_string(),
                class: "object.container".to_string(),
                containers: vec![],
                items: vec![],
            })
        }

        async fn browse(&self, _object_id: &str) -> Result<BrowseResult> {
            Ok(BrowseResult::Items(vec![]))
        }

        async fn resolve_uri(&self, object_id: &str) -> Result<String> {
            Ok(format!("http://example.com/{}", object_id))
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
            0
        }

        async fn last_change(&self) -> Option<SystemTime> {
            None
        }

        async fn get_items(&self, _offset: usize, _count: usize) -> Result<Vec<Item>> {
            Ok(vec![])
        }
    }

    #[tokio::test]
    async fn test_music_source_trait() {
        let source = TestSource;
        assert_eq!(source.name(), "Test Source");
        assert_eq!(source.id(), "test-source");
        assert_eq!(source.default_image_mime_type(), "image/webp");
        assert!(!source.supports_fifo());
    }

    #[tokio::test]
    async fn test_root_container() {
        let source = TestSource;
        let root = source.root_container().await.unwrap();
        assert_eq!(root.id, "test-source");
        assert_eq!(root.title, "Test Source");
    }

    #[tokio::test]
    async fn test_browse_result() {
        let items = vec![];
        let result = BrowseResult::Items(items);
        assert_eq!(result.count(), 0);
        assert_eq!(result.items().len(), 0);
        assert_eq!(result.containers().len(), 0);
    }

    #[tokio::test]
    async fn test_search_not_supported() {
        let source = TestSource;
        let result = source.search("test").await;
        assert!(matches!(result, Err(MusicSourceError::SearchNotSupported)));
    }

    #[tokio::test]
    async fn test_fifo_not_supported() {
        let source = TestSource;

        let item = Item {
            id: "test-1".to_string(),
            parent_id: "0".to_string(),
            restricted: Some("1".to_string()),
            title: "Test".to_string(),
            creator: None,
            class: "object.item.audioItem.musicTrack".to_string(),
            artist: None,
            album: None,
            genre: None,
            album_art: None,
            album_art_pk: None,
            date: None,
            original_track_number: None,
            resources: vec![],
            descriptions: vec![],
        };

        let result = source.append_track(item).await;
        assert!(matches!(result, Err(MusicSourceError::FifoNotSupported)));

        let result = source.remove_oldest().await;
        assert!(matches!(result, Err(MusicSourceError::FifoNotSupported)));
    }
}
