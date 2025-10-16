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
//!
//! ## Usage
//!
//! See the [examples/radio_paradise.rs](../examples/radio_paradise.rs) for a complete implementation.

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
}

/// Result type for music source operations
pub type Result<T> = std::result::Result<T, MusicSourceError>;

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
}

// Re-export commonly used types
pub use async_trait::async_trait;
pub use pmodidl;
pub use pmoplaylist;

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
