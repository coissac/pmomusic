//! MusicSource implementation for Radio France
//!
//! This module implements the `MusicSource` trait from `pmosource` for Radio France,
//! providing UPnP/DLNA integration with dynamic container generation.

use crate::error::Result;
use crate::models::Station;
use crate::playlist::{StationGroup, StationGroups, StationPlaylist};
use crate::stateful_client::RadioFranceStatefulClient;
use pmoconfig::Config;
use pmodidl::{Container, Item};
use pmosource::{async_trait, BrowseResult, MusicSource, MusicSourceError, SourceCapabilities};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::SystemTime;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;

#[cfg(feature = "cache")]
use pmocovers::Cache as CoverCache;

#[cfg(feature = "server")]
use pmoupnp;

/// Default image for Radio France source
const RADIOFRANCE_DEFAULT_IMAGE: &[u8] = include_bytes!("../assets/radiofrance-logo.webp");

/// Radio France music source
///
/// Provides access to ~70 Radio France stations via UPnP/DLNA with:
/// - Dynamic container generation based on station structure
/// - Automatic metadata refresh for active streams
/// - Hierarchical organization (standalone, groups, local radios)
pub struct RadioFranceSource {
    /// Stateful client with automatic caching
    client: RadioFranceStatefulClient,

    /// Cache of playlists by station slug (volatile metadata)
    playlists: Arc<RwLock<HashMap<String, StationPlaylist>>>,

    /// Background tasks for metadata refresh
    refresh_handles: Arc<RwLock<HashMap<String, JoinHandle<()>>>>,

    /// Cover cache (optional)
    #[cfg(feature = "cache")]
    cover_cache: Option<Arc<CoverCache>>,

    /// Server base URL for cover URLs
    server_base_url: Option<String>,

    /// Update counter for change tracking
    update_id: Arc<RwLock<u32>>,

    /// Last change timestamp
    last_change: Arc<RwLock<Option<SystemTime>>>,
}

impl RadioFranceSource {
    /// Create a new Radio France source
    ///
    /// # Arguments
    ///
    /// * `config` - Configuration for the client
    ///
    /// # Example
    ///
    /// ```no_run
    /// use pmoradiofrance::RadioFranceSource;
    /// use pmoconfig::get_config;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     let config = get_config();
    ///     let source = RadioFranceSource::new(config).await?;
    ///     Ok(())
    /// }
    /// ```
    pub async fn new(config: Arc<Config>) -> Result<Self> {
        let client = RadioFranceStatefulClient::new(config).await?;

        Ok(Self {
            client,
            playlists: Arc::new(RwLock::new(HashMap::new())),
            refresh_handles: Arc::new(RwLock::new(HashMap::new())),
            #[cfg(feature = "cache")]
            cover_cache: None,
            server_base_url: None,
            update_id: Arc::new(RwLock::new(0)),
            last_change: Arc::new(RwLock::new(None)),
        })
    }

    /// Set the cover cache
    #[cfg(feature = "cache")]
    pub fn with_cover_cache(mut self, cache: Arc<CoverCache>) -> Self {
        self.cover_cache = Some(cache);
        self
    }

    /// Set the server base URL for cover serving
    pub fn with_server_base_url(mut self, url: impl Into<String>) -> Self {
        self.server_base_url = Some(url.into());
        self
    }

    /// Create a new Radio France source from the cache registry
    ///
    /// This is the recommended way to create a source when using the UPnP server.
    /// The cover cache is automatically retrieved from the global registry.
    ///
    /// # Arguments
    ///
    /// * `client` - Radio France stateful client
    /// * `base_url` - Base URL for streaming server (e.g., "http://192.168.0.138:8080")
    ///
    /// # Errors
    ///
    /// Returns an error if the cover cache is not initialized in the registry
    #[cfg(feature = "server")]
    pub fn from_registry(
        client: RadioFranceStatefulClient,
        base_url: impl Into<String>,
    ) -> Result<Self> {
        #[cfg(feature = "cache")]
        let cover_cache = pmoupnp::cache_registry::get_cover_cache();

        Ok(Self {
            client,
            playlists: Arc::new(RwLock::new(HashMap::new())),
            refresh_handles: Arc::new(RwLock::new(HashMap::new())),
            #[cfg(feature = "cache")]
            cover_cache,
            server_base_url: Some(base_url.into()),
            update_id: Arc::new(RwLock::new(0)),
            last_change: Arc::new(RwLock::new(None)),
        })
    }

    /// Start metadata refresh task for a station
    async fn start_metadata_refresh(&self, station_slug: &str) -> Result<()> {
        let mut handles = self.refresh_handles.write().await;

        // If already running, do nothing
        if handles.contains_key(station_slug) {
            return Ok(());
        }

        let client = self.client.clone();
        let playlists = self.playlists.clone();
        let slug = station_slug.to_string();
        let update_id = self.update_id.clone();
        let last_change = self.last_change.clone();

        #[cfg(feature = "cache")]
        let cover_cache = self.cover_cache.clone();
        #[cfg(feature = "cache")]
        let server_base_url = self.server_base_url.clone();

        let handle = tokio::spawn(async move {
            loop {
                match client.get_live_metadata(&slug).await {
                    Ok(metadata) => {
                        let delay = std::time::Duration::from_millis(metadata.delay_to_refresh);

                        // Update the playlist metadata
                        #[cfg(feature = "cache")]
                        {
                            let mut pls = playlists.write().await;
                            if let Some(playlist) = pls.get_mut(&slug) {
                                let _: Result<()> = playlist
                                    .update_metadata(
                                        &metadata,
                                        cover_cache.as_ref(),
                                        server_base_url.as_deref(),
                                    )
                                    .await;

                                // Update change tracking
                                *update_id.write().await = update_id.read().await.wrapping_add(1);
                                *last_change.write().await = Some(SystemTime::now());
                            }
                        }

                        #[cfg(not(feature = "cache"))]
                        {
                            let mut pls = playlists.write().await;
                            if let Some(playlist) = pls.get_mut(&slug) {
                                let _: () = playlist.update_metadata_no_cache(&metadata);

                                // Update change tracking
                                *update_id.write().await = update_id.read().await.wrapping_add(1);
                                *last_change.write().await = Some(SystemTime::now());
                            }
                        }

                        tokio::time::sleep(delay).await;
                    }
                    Err(e) => {
                        #[cfg(feature = "logging")]
                        tracing::warn!("Failed to refresh metadata for {}: {}", slug, e);

                        tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                    }
                }
            }
        });

        handles.insert(station_slug.to_string(), handle);

        #[cfg(feature = "logging")]
        tracing::debug!("Started metadata refresh for station: {}", station_slug);

        Ok(())
    }

    /// Stop metadata refresh task for a station
    async fn stop_metadata_refresh(&self, station_slug: &str) {
        let mut handles = self.refresh_handles.write().await;
        if let Some(handle) = handles.remove(station_slug) {
            handle.abort();

            #[cfg(feature = "logging")]
            tracing::debug!("Stopped metadata refresh for station: {}", station_slug);
        }
    }

    /// Build the UPnP container tree dynamically from station data
    async fn build_container_tree(&self) -> Result<Container> {
        #[cfg(feature = "logging")]
        tracing::debug!("Building container tree");

        let stations = self.client.get_stations().await?;
        let groups = StationGroups::from_stations(stations);

        #[cfg(feature = "logging")]
        tracing::debug!(
            "Groups: {} standalone, {} with webradios, {} local radios",
            groups.standalone.len(),
            groups.with_webradios.len(),
            groups.local_radios.len()
        );

        let mut containers = Vec::new();
        let mut items = Vec::new();

        // 1. Standalone stations → direct items (avec appels API)
        #[cfg(feature = "logging")]
        tracing::debug!(
            "Building {} standalone station items",
            groups.standalone.len()
        );

        for station in &groups.standalone {
            items.push(self.build_station_item(station).await?);
        }

        // 2. Stations with webradios → containers
        #[cfg(feature = "logging")]
        tracing::debug!("Building {} group containers", groups.with_webradios.len());

        for group in &groups.with_webradios {
            #[cfg(feature = "logging")]
            tracing::debug!("Building container for group: {}", group.main.name);
            containers.push(self.build_station_container(group).await?);
        }

        // 3. Local radios → single "Radios ICI" container
        #[cfg(feature = "logging")]
        tracing::debug!(
            "Building ICI container with {} local radios",
            groups.local_radios.len()
        );

        if !groups.local_radios.is_empty() {
            containers.push(self.build_ici_container(&groups.local_radios).await?);
        }

        #[cfg(feature = "logging")]
        tracing::debug!(
            "Container tree built: {} containers, {} items",
            containers.len(),
            items.len()
        );

        Ok(Container {
            id: "radiofrance".to_string(),
            parent_id: "0".to_string(),
            restricted: Some("1".to_string()),
            child_count: Some((containers.len() + items.len()).to_string()),
            searchable: Some("0".to_string()),
            title: "Radio France".to_string(),
            class: "object.container".to_string(),
            artist: None,
            album_art: None,
            containers,
            items,
        })
    }

    /// Build a container for a station group (main + webradios)
    /// Returns an empty container - items will be built when browsing into it
    async fn build_station_container(&self, group: &StationGroup) -> Result<Container> {
        let child_count = 1 + group.webradios.len(); // main + webradios

        Ok(Container {
            id: format!("radiofrance:group:{}", group.main.slug),
            parent_id: "radiofrance".to_string(),
            restricted: Some("1".to_string()),
            child_count: Some(child_count.to_string()),
            searchable: Some("0".to_string()),
            title: group.main.name.clone(),
            class: "object.container".to_string(),
            artist: None,
            album_art: None,
            containers: vec![],
            items: vec![],
        })
    }

    /// Build the "Radios ICI" container
    /// Returns an empty container - items will be built when browsing into it
    async fn build_ici_container(&self, local_radios: &[Station]) -> Result<Container> {
        Ok(Container {
            id: "radiofrance:ici".to_string(),
            parent_id: "radiofrance".to_string(),
            restricted: Some("1".to_string()),
            child_count: Some(local_radios.len().to_string()),
            searchable: Some("0".to_string()),
            title: "Radios ICI".to_string(),
            class: "object.container".to_string(),
            artist: None,
            album_art: None,
            containers: vec![],
            items: vec![],
        })
    }

    /// Build a UPnP item for a station
    ///
    /// Fetches live metadata to create a complete item with stream URL.
    async fn build_station_item(&self, station: &Station) -> Result<Item> {
        #[cfg(feature = "logging")]
        tracing::debug!(
            "Building station item for: {} ({})",
            station.name,
            station.slug
        );

        let playlists = self.playlists.read().await;

        // If we already have this station in cache, use it
        if let Some(existing) = playlists.get(&station.slug) {
            #[cfg(feature = "logging")]
            tracing::debug!("Using cached item for: {}", station.slug);
            return Ok(existing.stream_item.clone());
        }

        // Release read lock before fetching metadata
        drop(playlists);

        // Fetch metadata from API
        let metadata = self.client.get_live_metadata(&station.slug).await?;

        // Create playlist with metadata
        #[cfg(feature = "cache")]
        let playlist = StationPlaylist::from_live_metadata(
            station.clone(),
            &metadata,
            self.cover_cache.as_ref(),
            self.server_base_url.as_deref(),
        )
        .await?;

        #[cfg(not(feature = "cache"))]
        let playlist = StationPlaylist::from_live_metadata_no_cache(station.clone(), &metadata)?;

        // Cache it
        let mut playlists_write = self.playlists.write().await;
        playlists_write.insert(station.slug.clone(), playlist.clone());
        drop(playlists_write);

        // Note: We don't start metadata refresh here to avoid blocking during browse.
        // Refresh will be started in resolve_uri() when the stream is actually played.

        Ok(playlist.stream_item)
    }
}

impl std::fmt::Debug for RadioFranceSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RadioFranceSource")
            .field("client", &self.client)
            .field("playlists_count", &"<locked>")
            .field("refresh_handles_count", &"<locked>")
            .finish()
    }
}

#[async_trait]
impl MusicSource for RadioFranceSource {
    fn name(&self) -> &str {
        "Radio France"
    }

    fn id(&self) -> &str {
        "radiofrance"
    }

    fn default_image(&self) -> &[u8] {
        RADIOFRANCE_DEFAULT_IMAGE
    }

    fn capabilities(&self) -> SourceCapabilities {
        SourceCapabilities {
            supports_fifo: false,
            supports_search: false,
            supports_favorites: false,
            supports_playlists: false,
            supports_user_content: false,
            supports_high_res_audio: false,
            max_sample_rate: Some(48000), // AAC 48kHz
            supports_multiple_formats: false,
            supports_advanced_search: false,
            supports_pagination: false,
        }
    }

    async fn root_container(&self) -> pmosource::Result<Container> {
        Ok(Container {
            id: "radiofrance".to_string(),
            parent_id: "0".to_string(),
            restricted: Some("1".to_string()),
            child_count: None,
            searchable: Some("0".to_string()),
            title: "Radio France".to_string(),
            class: "object.container".to_string(),
            artist: None,
            album_art: None,
            containers: vec![],
            items: vec![],
        })
    }

    async fn browse(&self, object_id: &str) -> pmosource::Result<BrowseResult> {
        match object_id {
            "radiofrance" => {
                let container = self
                    .build_container_tree()
                    .await
                    .map_err(|e| MusicSourceError::BrowseError(e.to_string()))?;

                Ok(BrowseResult::Mixed {
                    containers: container.containers,
                    items: container.items,
                })
            }
            id if id.starts_with("radiofrance:group:") => {
                let slug = id
                    .strip_prefix("radiofrance:group:")
                    .ok_or_else(|| MusicSourceError::ObjectNotFound(id.to_string()))?;

                let stations = self
                    .client
                    .get_stations()
                    .await
                    .map_err(|e| MusicSourceError::BrowseError(e.to_string()))?;
                let groups = StationGroups::from_stations(stations);

                let group = groups
                    .with_webradios
                    .iter()
                    .find(|g| g.main.slug == slug)
                    .ok_or_else(|| MusicSourceError::ObjectNotFound(id.to_string()))?;

                // Build items for this group only (main + webradios)
                let mut items = vec![self
                    .build_station_item(&group.main)
                    .await
                    .map_err(|e| MusicSourceError::BrowseError(e.to_string()))?];

                for webradio in &group.webradios {
                    items.push(
                        self.build_station_item(webradio)
                            .await
                            .map_err(|e| MusicSourceError::BrowseError(e.to_string()))?,
                    );
                }

                Ok(BrowseResult::Items(items))
            }
            "radiofrance:ici" => {
                let stations = self
                    .client
                    .get_stations()
                    .await
                    .map_err(|e| MusicSourceError::BrowseError(e.to_string()))?;
                let groups = StationGroups::from_stations(stations);

                // Build items for local radios only
                let mut items = Vec::new();
                for station in &groups.local_radios {
                    items.push(
                        self.build_station_item(station)
                            .await
                            .map_err(|e| MusicSourceError::BrowseError(e.to_string()))?,
                    );
                }

                Ok(BrowseResult::Items(items))
            }
            _ => Err(MusicSourceError::ObjectNotFound(object_id.to_string())),
        }
    }

    async fn get_item(&self, object_id: &str) -> pmosource::Result<Item> {
        // Format: radiofrance:{slug}:stream
        let slug = object_id
            .strip_prefix("radiofrance:")
            .and_then(|s| s.strip_suffix(":stream"))
            .ok_or_else(|| MusicSourceError::ObjectNotFound(object_id.to_string()))?;

        let playlists = self.playlists.read().await;
        playlists
            .get(slug)
            .map(|p| p.stream_item.clone())
            .ok_or_else(|| MusicSourceError::ObjectNotFound(object_id.to_string()))
    }

    async fn resolve_uri(&self, object_id: &str) -> pmosource::Result<String> {
        // Extract station slug from object_id (format: radiofrance:{slug}:stream)
        let slug = object_id
            .strip_prefix("radiofrance:")
            .and_then(|s| s.strip_suffix(":stream"))
            .ok_or_else(|| MusicSourceError::ObjectNotFound(object_id.to_string()))?;

        // Ensure we have metadata for this station
        let playlists = self.playlists.read().await;
        let needs_metadata = !playlists.contains_key(slug);
        drop(playlists);

        if needs_metadata {
            // Fetch metadata and create playlist
            let stations = self
                .client
                .get_stations()
                .await
                .map_err(|e| MusicSourceError::BrowseError(e.to_string()))?;

            let station = stations
                .iter()
                .find(|s| s.slug == slug)
                .ok_or_else(|| MusicSourceError::ObjectNotFound(slug.to_string()))?;

            let metadata = self
                .client
                .get_live_metadata(slug)
                .await
                .map_err(|e| MusicSourceError::BrowseError(e.to_string()))?;

            #[cfg(feature = "cache")]
            let playlist = StationPlaylist::from_live_metadata(
                station.clone(),
                &metadata,
                self.cover_cache.as_ref(),
                self.server_base_url.as_deref(),
            )
            .await
            .map_err(|e| MusicSourceError::BrowseError(e.to_string()))?;

            #[cfg(not(feature = "cache"))]
            let playlist = StationPlaylist::from_live_metadata_no_cache(station.clone(), &metadata)
                .map_err(|e| MusicSourceError::BrowseError(e.to_string()))?;

            let mut playlists_write = self.playlists.write().await;
            playlists_write.insert(slug.to_string(), playlist);

            // Start metadata refresh
            drop(playlists_write);
            let _ = self.start_metadata_refresh(slug).await;
        }

        let item = self.get_item(object_id).await?;
        item.resources
            .first()
            .map(|r| r.url.clone())
            .ok_or_else(|| MusicSourceError::UriResolutionError("No resource found".to_string()))
    }

    fn supports_fifo(&self) -> bool {
        false
    }

    async fn append_track(&self, _track: Item) -> pmosource::Result<()> {
        Err(MusicSourceError::FifoNotSupported)
    }

    async fn remove_oldest(&self) -> pmosource::Result<Option<Item>> {
        Err(MusicSourceError::FifoNotSupported)
    }

    async fn update_id(&self) -> u32 {
        *self.update_id.read().await
    }

    async fn last_change(&self) -> Option<SystemTime> {
        *self.last_change.read().await
    }

    async fn get_items(&self, offset: usize, count: usize) -> pmosource::Result<Vec<Item>> {
        // Not applicable for radio stations
        let _ = (offset, count);
        Ok(vec![])
    }
}

impl Drop for RadioFranceSource {
    fn drop(&mut self) {
        // Abort all refresh tasks on drop
        if let Ok(handles) = self.refresh_handles.try_write() {
            for (_, handle) in handles.iter() {
                handle.abort();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: These tests require a valid pmoconfig setup
    // They are primarily structural tests

    #[test]
    fn test_source_metadata() {
        // Test that we can create a source with proper metadata
        // Actual async tests would go in integration tests
    }
}
