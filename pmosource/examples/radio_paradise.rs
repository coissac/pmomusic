//! # Radio Paradise Example
//!
//! This example demonstrates how to implement a concrete `MusicSource` using
//! Radio Paradise as a streaming radio source with FIFO support.
//!
//! ## Features
//!
//! - **FIFO Playlist**: Uses `pmoplaylist::FifoPlaylist` for dynamic track management
//! - **Cache Integration**: Resolves URIs via `pmoaudiocache` and `pmocovers` (when enabled)
//! - **DIDL-Lite Export**: Generates proper UPnP-compatible containers and items
//! - **Change Tracking**: Tracks `update_id` and `last_change` for notifications
//!
//! ## Usage
//!
//! ```bash
//! cargo run --example radio_paradise
//! ```

use pmodidl::{Container, Item, Resource};
use pmoplaylist::{FifoPlaylist, Track};
use pmosource::{async_trait, BrowseResult, MusicSource, MusicSourceError, Result};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Default image for Radio Paradise (embedded WebP)
const RADIO_PARADISE_IMAGE: &[u8] = include_bytes!("../assets/radio-paradise.webp");

/// Default capacity for the FIFO (number of recent tracks to keep)
const DEFAULT_FIFO_CAPACITY: usize = 50;

/// Radio Paradise music source
///
/// This is a concrete implementation of `MusicSource` for Radio Paradise,
/// demonstrating how to:
/// - Use `pmoplaylist::FifoPlaylist` for dynamic track management
/// - Integrate with caches for URI resolution
/// - Implement ContentDirectory browsing
/// - Track changes via `update_id` and `last_change`
#[derive(Clone)]
pub struct RadioParadise {
    inner: Arc<RadioParadiseInner>,
}

struct RadioParadiseInner {
    /// FIFO playlist managed by pmoplaylist
    playlist: FifoPlaylist,

    /// Cache server base URL (for URI resolution)
    cache_base_url: String,

    /// Track metadata cache (object_id -> original_uri, cached_pk)
    track_cache: RwLock<HashMap<String, (String, Option<String>)>>,
}

// Manual Debug implementation since FifoPlaylist doesn't derive Debug
impl std::fmt::Debug for RadioParadise {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RadioParadise")
            .field("cache_base_url", &self.inner.cache_base_url)
            .finish()
    }
}

impl RadioParadise {
    /// Create a new Radio Paradise source
    ///
    /// # Arguments
    ///
    /// * `cache_base_url` - Base URL for the cache server (e.g., "http://localhost:8080")
    /// * `fifo_capacity` - Maximum number of tracks in the FIFO
    ///
    /// # Examples
    ///
    /// ```
    /// use pmosource::RadioParadise;
    ///
    /// let source = RadioParadise::new("http://localhost:8080", 50);
    /// ```
    pub fn new(cache_base_url: impl Into<String>, fifo_capacity: usize) -> Self {
        let playlist = FifoPlaylist::new(
            "radio-paradise".to_string(),
            "Radio Paradise".to_string(),
            fifo_capacity,
            RADIO_PARADISE_IMAGE,
        );

        Self {
            inner: Arc::new(RadioParadiseInner {
                playlist,
                cache_base_url: cache_base_url.into(),
                track_cache: RwLock::new(HashMap::new()),
            }),
        }
    }

    /// Create with default settings
    pub fn new_default(cache_base_url: impl Into<String>) -> Self {
        Self::new(cache_base_url, DEFAULT_FIFO_CAPACITY)
    }

    /// Add a track to the Radio Paradise FIFO from raw data
    ///
    /// This simulates receiving a new track from the Radio Paradise API.
    ///
    /// # Arguments
    ///
    /// * `id` - Unique track ID
    /// * `title` - Track title
    /// * `artist` - Artist name
    /// * `album` - Album name
    /// * `uri` - Original streaming URI
    /// * `image_url` - URL for cover art (optional)
    /// * `duration` - Track duration in seconds (optional)
    pub async fn add_track(
        &self,
        id: String,
        title: String,
        artist: Option<String>,
        album: Option<String>,
        uri: String,
        image_url: Option<String>,
        duration: Option<u32>,
    ) -> Result<()> {
        // Store the original URI for later resolution
        {
            let mut cache = self.inner.track_cache.write().await;
            cache.insert(id.clone(), (uri.clone(), None));
        }

        // Create a Track for pmoplaylist
        let mut track = Track::new(id, title, uri);

        if let Some(artist) = artist {
            track = track.with_artist(artist);
        }

        if let Some(album) = album {
            track = track.with_album(album);
        }

        if let Some(duration) = duration {
            track = track.with_duration(duration);
        }

        if let Some(image) = image_url {
            track = track.with_image(image);
        }

        // Add to the FIFO (automatically handles capacity)
        self.inner.playlist.append_track(track).await;

        Ok(())
    }

    /// Simulate caching a track
    ///
    /// In a real implementation, this would interact with `pmoaudiocache`
    /// to download and cache the track, then store the cache key.
    ///
    /// # Arguments
    ///
    /// * `track_id` - The track ID to cache
    /// * `cache_pk` - The cache primary key returned by pmoaudiocache
    pub async fn cache_track(&self, track_id: &str, cache_pk: String) -> Result<()> {
        let mut cache = self.inner.track_cache.write().await;

        if let Some((_original_uri, cached_pk)) = cache.get_mut(track_id) {
            *cached_pk = Some(cache_pk);
            Ok(())
        } else {
            Err(MusicSourceError::ObjectNotFound(track_id.to_string()))
        }
    }

    /// Convert pmoplaylist::Track to pmodidl::Item
    fn track_to_item(&self, track: &Track) -> Item {
        // Format duration
        let duration_str = track.duration.map(|d| {
            let hours = d / 3600;
            let minutes = (d % 3600) / 60;
            let seconds = d % 60;
            format!("{}:{:02}:{:02}", hours, minutes, seconds)
        });

        // Create resource
        let resource = Resource {
            protocol_info: "http-get:*:audio/*:*".to_string(),
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
}

#[async_trait]
impl MusicSource for RadioParadise {
    fn name(&self) -> &str {
        "Radio Paradise"
    }

    fn id(&self) -> &str {
        "radio-paradise"
    }

    fn default_image(&self) -> &[u8] {
        RADIO_PARADISE_IMAGE
    }

    async fn root_container(&self) -> Result<Container> {
        Ok(self.inner.playlist.as_container().await)
    }

    async fn browse(&self, object_id: &str) -> Result<BrowseResult> {
        // For Radio Paradise, browsing the root returns all tracks in the FIFO
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

        if let Some((original_uri, cached_pk)) = cache.get(object_id) {
            // If cached, return the cached URI
            if let Some(pk) = cached_pk {
                Ok(format!("{}/audio/cache/{}", self.inner.cache_base_url, pk))
            } else {
                // Not cached yet, return original URI
                Ok(original_uri.clone())
            }
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

        let mut pmo_track = Track::new(track.id.clone(), track.title.clone(), uri.clone());

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

        // Store in cache
        {
            let mut cache = self.inner.track_cache.write().await;
            cache.insert(track.id.clone(), (uri, None));
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

    async fn last_change(&self) -> Option<std::time::SystemTime> {
        Some(self.inner.playlist.last_change().await)
    }

    async fn get_items(&self, offset: usize, count: usize) -> Result<Vec<Item>> {
        let tracks = self.inner.playlist.get_items(offset, count).await;
        Ok(tracks.iter().map(|t| self.track_to_item(t)).collect())
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("Radio Paradise Source Example");
    println!("==============================\n");

    // Create the source
    let source = RadioParadise::new_default("http://localhost:8080");

    println!("Source: {}", source.name());
    println!("ID: {}", source.id());
    println!("Supports FIFO: {}", source.supports_fifo());
    println!(
        "Default image size: {} bytes\n",
        source.default_image().len()
    );

    // Add some sample tracks
    println!("Adding sample tracks...");

    source
        .add_track(
            "rp-001".to_string(),
            "Wish You Were Here".to_string(),
            Some("Pink Floyd".to_string()),
            Some("Wish You Were Here".to_string()),
            "http://stream.radioparadise.com/rp-001.mp3".to_string(),
            Some("http://img.radioparadise.com/covers/l/001.jpg".to_string()),
            Some(334),
        )
        .await?;

    source
        .add_track(
            "rp-002".to_string(),
            "Bohemian Rhapsody".to_string(),
            Some("Queen".to_string()),
            Some("A Night at the Opera".to_string()),
            "http://stream.radioparadise.com/rp-002.mp3".to_string(),
            Some("http://img.radioparadise.com/covers/l/002.jpg".to_string()),
            Some(354),
        )
        .await?;

    source
        .add_track(
            "rp-003".to_string(),
            "Hotel California".to_string(),
            Some("Eagles".to_string()),
            Some("Hotel California".to_string()),
            "http://stream.radioparadise.com/rp-003.mp3".to_string(),
            Some("http://img.radioparadise.com/covers/l/003.jpg".to_string()),
            Some(391),
        )
        .await?;

    println!("Added 3 tracks\n");

    // Get root container
    println!("Root Container:");
    let root = source.root_container().await?;
    println!("  ID: {}", root.id);
    println!("  Title: {}", root.title);
    println!("  Child Count: {:?}\n", root.child_count);

    // Browse the source
    println!("Browsing tracks:");
    let result = source.browse("radio-paradise").await?;
    for item in result.items() {
        println!(
            "  - {} by {} ({})",
            item.title,
            item.artist.as_deref().unwrap_or("Unknown"),
            item.album.as_deref().unwrap_or("Unknown Album")
        );
    }
    println!();

    // Resolve URIs
    println!("Resolving URIs:");
    for item in result.items() {
        let uri = source.resolve_uri(&item.id).await?;
        println!("  {}: {}", item.id, uri);
    }
    println!();

    // Track changes
    println!("Change Tracking:");
    println!("  Update ID: {}", source.update_id().await);
    println!("  Last Change: {:?}\n", source.last_change().await.unwrap());

    // Simulate caching a track
    println!("Simulating cache for rp-001...");
    source
        .cache_track("rp-001", "cached-abc123".to_string())
        .await?;

    let cached_uri = source.resolve_uri("rp-001").await?;
    println!("  Cached URI: {}\n", cached_uri);

    // Pagination
    println!("Pagination (get items 1-2):");
    let items = source.get_items(1, 2).await?;
    for item in items {
        println!("  - {}", item.title);
    }
    println!();

    // Remove oldest track
    println!("Removing oldest track...");
    if let Some(removed) = source.remove_oldest().await? {
        println!("  Removed: {}", removed.title);
    }
    println!("  New Update ID: {}\n", source.update_id().await);

    // Browse again to see the change
    println!("Browsing after removal:");
    let result = source.browse("radio-paradise").await?;
    println!("  Tracks remaining: {}", result.count());
    for item in result.items() {
        println!("    - {}", item.title);
    }

    Ok(())
}
