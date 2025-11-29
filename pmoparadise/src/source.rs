//! RadioParadiseSource - Implementation of MusicSource for Radio Paradise
//!
//! This module provides a UPnP ContentDirectory source for Radio Paradise,
//! exposing live streams and historical playlists for all 4 channels.

use crate::channels::{ChannelDescriptor, ALL_CHANNELS};
use pmosource::pmodidl::{Container, Item, Resource};
use pmosource::{
    async_trait, AudioFormat, BrowseResult, MusicSource, MusicSourceError, Result,
    SourceCapabilities,
};
use std::sync::Arc;
use std::time::SystemTime;
use tokio::sync::RwLock;


/// Default Radio Paradise image (embedded in binary)
const DEFAULT_IMAGE: &[u8] = include_bytes!("../assets/default.webp");

/// RadioParadiseSource - UPnP ContentDirectory source for Radio Paradise
///
/// Provides access to:
/// - Live FLAC streams for all 4 channels (Main, Mellow, Rock, Eclectic)
/// - Historical playlists (FIFO) for each channel
///
/// # Object ID Schema
///
/// - Root: `radio-paradise`
/// - Channel container: `radio-paradise:channel:{slug}`
/// - Live stream item: `radio-paradise:channel:{slug}:live`
/// - Live playlist container: `radio-paradise:channel:{slug}:liveplaylist`
/// - Live playlist track: `radio-paradise:channel:{slug}:liveplaylist:track:{pk}`
/// - History container: `radio-paradise:channel:{slug}:history`
/// - History track: `radio-paradise:channel:{slug}:history:track:{pk}`
#[derive(Debug, Clone)]
pub struct RadioParadiseSource {
    /// Base URL for streaming server (e.g., "http://localhost:8080")
    base_url: String,
    /// Update counter for change notifications
    update_counter: Arc<RwLock<u32>>,
    /// Last change timestamp
    last_change: Arc<RwLock<SystemTime>>,
    /// Tokens des callbacks enregistrés auprès du PlaylistManager
    callback_tokens: Arc<std::sync::Mutex<Vec<u64>>>,
}

impl RadioParadiseSource {
    /// Create a new RadioParadiseSource
    ///
    /// # Arguments
    ///
    /// * `base_url` - Base URL for streaming server (e.g., "http://localhost:8080")
    ///
    /// # Note
    ///
    /// With the "playlist" feature enabled, this source will use the global PlaylistManager
    /// singleton to access history playlists.
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            update_counter: Arc::new(RwLock::new(0)),
            last_change: Arc::new(RwLock::new(SystemTime::now())),
            callback_tokens: Arc::new(std::sync::Mutex::new(Vec::new())),
        }
    }

    /// Build a live stream URL for a channel
    fn build_live_url(&self, slug: &str) -> String {
        format!("{}/radioparadise/stream/{}/flac", self.base_url, slug)
    }

    /// Build an OGG-FLAC live stream URL for clients that support it
    fn build_live_ogg_url(&self, slug: &str) -> String {
        format!("{}/radioparadise/stream/{}/ogg", self.base_url, slug)
    }

    /// Incrémente l'update_counter et met à jour last_change
    async fn bump_update_counter(&self) {
        {
            let mut c = self.update_counter.write().await;
            *c = c.wrapping_add(1).max(1);
        }
        let mut lc = self.last_change.write().await;
        *lc = SystemTime::now();
    }

    /// Enregistre des callbacks sur les playlists live/historique pour notifier les changements
    pub fn attach_playlist_callbacks(self: &Arc<Self>) {
        use pmoplaylist::PlaylistManager;

        // Préparer les IDs de playlists à surveiller (live + history pour chaque canal)
        let ids: Vec<String> = ALL_CHANNELS
            .iter()
            .flat_map(|ch| {
                vec![
                    Self::live_playlist_id(ch.slug),
                    Self::history_playlist_id(ch.slug),
                ]
            })
            .collect();

        let mgr = PlaylistManager();
        let mut tokens = self.callback_tokens.lock().unwrap();

        for pid in ids {
            let weak = Arc::downgrade(self);
            let token = mgr.register_callback(move |changed_id| {
                if changed_id == pid {
                    if let Some(strong) = weak.upgrade() {
                        tokio::spawn(async move {
                            strong.bump_update_counter().await;
                        });
                    }
                }
            });
            tokens.push(token);
        }
    }

    /// URL de fallback pour l'image par défaut de la source
    fn default_cover_url(&self) -> String {
        format!("{}/api/sources/{}/image", self.base_url, self.id())
    }

    /// Fetch current metadata from the live stream
    async fn fetch_live_metadata(&self, slug: &str) -> Result<Option<Item>> {
        let metadata_url = format!("{}/radioparadise/metadata/{}", self.base_url, slug);

        // Try to fetch metadata via HTTP
        match reqwest::get(&metadata_url).await {
            Ok(response) if response.status().is_success() => {
                match response.json::<serde_json::Value>().await {
                    Ok(json) => {
                        // Parse metadata from JSON and create an Item
                        let title = json["title"].as_str().unwrap_or("Unknown Title").to_string();
                        let artist = json["artist"].as_str().map(|s| s.to_string());
                        let album = json["album"].as_str().map(|s| s.to_string());
                        let year = json["year"].as_u64().map(|y| y as u32);
                        // Préférer l'URL de cache si cover_pk est fourni par le pipeline
                        let cover_pk = json["cover_pk"].as_str().map(|s| s.to_string());
                        let cover_url = cover_pk
                            .as_ref()
                            .map(|pk| format!("{}/covers/jpeg/{}", self.base_url, pk))
                            .or_else(|| json["cover_url"].as_str().map(|s| s.to_string()))
                            .or_else(|| Some(self.default_cover_url()));

                        // Parse duration from JSON (in seconds as a float)
                        let duration = json["duration"]
                            .as_object()
                            .and_then(|d| d.get("secs"))
                            .and_then(|s| s.as_f64())
                            .or_else(|| json["duration"].as_f64())
                            .map(|secs| {
                                let total_secs = secs as u64;
                                format!("{}:{:02}:{:02}",
                                    total_secs / 3600,
                                    (total_secs % 3600) / 60,
                                    total_secs % 60)
                            });

                        // Create the item with current metadata
                        let item = Item {
                            id: format!("radio-paradise:channel:{}:live", slug),
                            parent_id: format!("radio-paradise:channel:{}", slug),
                            restricted: Some("1".to_string()),
                            title,
                            creator: artist.clone(),
                            class: "object.item.audioItem.audioBroadcast".to_string(),
                            artist,
                            album,
                            genre: Some("Radio".to_string()),
                            album_art: cover_url,
                            album_art_pk: cover_pk,
                            date: year.map(|y| y.to_string()),
                            original_track_number: None,
                            resources: vec![Resource {
                                protocol_info: "http-get:*:audio/flac:*".to_string(),
                                bits_per_sample: None,
                                sample_frequency: None,
                                nr_audio_channels: Some("2".to_string()),
                                duration,
                                url: self.build_live_url(slug),
                            }],
                            descriptions: vec![],
                        };

                        Ok(Some(item))
                    }
                    Err(_) => Ok(None),
                }
            }
            _ => Ok(None),
        }
    }

    /// Get the playlist ID for a channel's history
    #[cfg(feature = "playlist")]
    fn history_playlist_id(slug: &str) -> String {
        // Must match the prefix used in ParadiseHistoryBuilder
        format!("radio-paradise-history-{}", slug)
    }

    /// Live playlist id for a channel
    fn live_playlist_id(slug: &str) -> String {
        format!("radio-paradise-live-{}", slug)
    }

    /// Get channel descriptor by slug
    fn get_channel_by_slug(slug: &str) -> Option<&'static ChannelDescriptor> {
        ALL_CHANNELS.iter().find(|ch| ch.slug == slug)
    }

    /// Parse an object ID into its components
    fn parse_object_id(id: &str) -> ObjectIdType {
        let parts: Vec<&str> = id.split(':').collect();
        match parts.as_slice() {
            ["radio-paradise"] => ObjectIdType::Root,
            ["radio-paradise", "channel", slug] => ObjectIdType::Channel {
                slug: (*slug).to_string(),
            },
            ["radio-paradise", "channel", slug, "live"] => ObjectIdType::LiveStream {
                slug: (*slug).to_string(),
            },
            ["radio-paradise", "channel", slug, "liveplaylist"] => ObjectIdType::LivePlaylist {
                slug: (*slug).to_string(),
            },
            ["radio-paradise", "channel", slug, "liveplaylist", "track", pk] => {
                ObjectIdType::LivePlaylistTrack {
                    slug: (*slug).to_string(),
                    pk: (*pk).to_string(),
                }
            }
            ["radio-paradise", "channel", slug, "history"] => ObjectIdType::History {
                slug: (*slug).to_string(),
            },
            ["radio-paradise", "channel", slug, "history", "track", pk] => {
                ObjectIdType::HistoryTrack {
                    slug: (*slug).to_string(),
                    pk: (*pk).to_string(),
                }
            }
            _ => ObjectIdType::Unknown,
        }
    }

    /// Build a channel container
    fn build_channel_container(&self, descriptor: &ChannelDescriptor) -> Container {
        Container {
            id: format!("radio-paradise:channel:{}", descriptor.slug),
            parent_id: "radio-paradise".to_string(),
            restricted: Some("1".to_string()),
            child_count: None,
            searchable: Some("1".to_string()),
            title: descriptor.display_name.to_string(),
            class: "object.container".to_string(),
            containers: vec![],
            items: vec![],
        }
    }

    /// Build the live playlist container for a channel
    fn build_live_playlist_container(&self, descriptor: &ChannelDescriptor) -> Container {
        Container {
            id: format!("radio-paradise:channel:{}:liveplaylist", descriptor.slug),
            parent_id: format!("radio-paradise:channel:{}", descriptor.slug),
            restricted: Some("1".to_string()),
            child_count: None,
            searchable: Some("0".to_string()),
            title: format!("{} - Live Playlist", descriptor.display_name),
            class: "object.container.playlistContainer".to_string(),
            containers: vec![],
            items: vec![],
        }
    }

    /// Build a live stream item for a channel
    fn build_live_stream_item(&self, descriptor: &ChannelDescriptor) -> Item {
        let stream_url = self.build_live_url(descriptor.slug);

        Item {
            id: format!("radio-paradise:channel:{}:live", descriptor.slug),
            parent_id: format!("radio-paradise:channel:{}", descriptor.slug),
            restricted: Some("1".to_string()),
            title: format!("{} - Live Stream", descriptor.display_name),
            creator: Some("Radio Paradise".to_string()),
            class: "object.item.audioItem.audioBroadcast".to_string(),
            artist: Some("Radio Paradise".to_string()),
            album: Some(descriptor.display_name.to_string()),
            genre: Some("Radio".to_string()),
            album_art: Some(self.default_cover_url()),
            album_art_pk: None,
            date: None,
            original_track_number: None,
            resources: vec![
                Resource {
                    protocol_info: "http-get:*:audio/flac:*".to_string(),
                    bits_per_sample: Some("16".to_string()),
                    sample_frequency: Some("44100".to_string()),
                    nr_audio_channels: Some("2".to_string()),
                    duration: None,
                    url: stream_url.clone(),
                },
                Resource {
                    protocol_info: "http-get:*:audio/ogg:*".to_string(),
                    bits_per_sample: Some("16".to_string()),
                    sample_frequency: Some("44100".to_string()),
                    nr_audio_channels: Some("2".to_string()),
                    duration: None,
                    url: self.build_live_ogg_url(descriptor.slug),
                },
            ],
            descriptions: vec![],
        }
    }

    /// Build a history container for a channel
    fn build_history_container(&self, descriptor: &ChannelDescriptor) -> Container {
        Container {
            id: format!("radio-paradise:channel:{}:history", descriptor.slug),
            parent_id: format!("radio-paradise:channel:{}", descriptor.slug),
            restricted: Some("1".to_string()),
            child_count: None,
            searchable: Some("1".to_string()),
            title: format!("{} - History", descriptor.display_name),
            // Expose l'historique comme une playlist jouable
            class: "object.container.playlistContainer".to_string(),
            containers: vec![],
            items: vec![],
        }
    }

    /// Build a history container with accurate child count from playlist
    #[cfg(feature = "playlist")]
    async fn build_history_container_with_count(&self, descriptor: &ChannelDescriptor) -> Container {
        let mut container = self.build_history_container(descriptor);

        // Try to get actual count from playlist
        let playlist_id = Self::history_playlist_id(descriptor.slug);
        let manager = pmoplaylist::PlaylistManager();

        if let Ok(reader) = manager.get_read_handle(&playlist_id).await {
            if let Ok(count) = reader.remaining().await {
                container.child_count = Some(count.to_string());
            }
        }

        container
    }

    /// Get items from history playlist
    #[cfg(feature = "playlist")]
    async fn get_history_items(
        &self,
        slug: &str,
        _offset: usize,
        count: usize,
    ) -> Result<Vec<Item>> {
        let playlist_id = Self::history_playlist_id(slug);

        // Get read handle for the playlist from the singleton
        let manager = pmoplaylist::PlaylistManager();
        let reader = manager.get_read_handle(&playlist_id).await.map_err(|e| {
            MusicSourceError::BrowseError(format!("Failed to get playlist {}: {}", playlist_id, e))
        })?;

        // Get items from playlist (to_items starts from cursor position)
        let mut items = reader.to_items(count).await.map_err(|e| {
            MusicSourceError::BrowseError(format!("Failed to read playlist entries: {}", e))
        })?;

        // Transform item IDs, parent_ids, and resource URLs to match Radio Paradise schema
        // Expected: radio-paradise:channel:{slug}:history:track:{pk}
        // Parent: radio-paradise:channel:{slug}:history
        for item in items.iter_mut() {
            // Extract cache_pk from the resource URL (last segment)
            if let Some(resource) = item.resources.first_mut() {
                if let Some(pk) = resource.url.split('/').last() {
                    // Update item ID and parent ID
                    item.id = format!("radio-paradise:channel:{}:history:track:{}", slug, pk);
                    item.parent_id = format!("radio-paradise:channel:{}:history", slug);

                    // Convert relative URL to absolute URL
                    // From: /audio/flac/pk
                    // To: http://base_url/audio/flac/pk
                    if resource.url.starts_with('/') {
                        resource.url = format!("{}{}", self.base_url, resource.url);
                    }
                }
            }

            // Fix: Ajouter un genre par défaut si absent
            // Certains clients UPnP (comme gupnp-av-cp) requièrent le champ <upnp:genre>
            // pour parser correctement les items de classe musicTrack, même si ce champ
            // est optionnel selon la spec UPnP ContentDirectory.
            if item.genre.is_none() {
                item.genre = Some("Radio Paradise".to_string());
            }

            // Normaliser l'albumArtURI : rendre absolu si chemin relatif, sinon fallback par défaut
            if let Some(art) = item.album_art.as_mut() {
                if art.starts_with('/') {
                    *art = format!("{}{}", self.base_url, art);
                }
            } else {
                item.album_art = Some(self.default_cover_url());
            }
        }

        Ok(items)
    }

    /// Get items from live playlist (current stream queue)
    #[cfg(feature = "playlist")]
    async fn get_live_playlist_items(
        &self,
        slug: &str,
        _offset: usize,
        count: usize,
    ) -> Result<Vec<Item>> {
        let playlist_id = Self::live_playlist_id(slug);

        let manager = pmoplaylist::PlaylistManager();
        let reader = manager.get_read_handle(&playlist_id).await.map_err(|e| {
            MusicSourceError::BrowseError(format!("Failed to get live playlist {}: {}", playlist_id, e))
        })?;

        let mut items = reader.to_items(count).await.map_err(|e| {
            MusicSourceError::BrowseError(format!(
                "Failed to read live playlist entries: {}",
                e
            ))
        })?;

        for item in items.iter_mut() {
            // Ajuster id/parent/url pour coller au schéma Radio Paradise
            if let Some(resource) = item.resources.first_mut() {
                if let Some(pk) = resource.url.split('/').last() {
                    item.id = format!(
                        "radio-paradise:channel:{}:liveplaylist:track:{}",
                        slug, pk
                    );
                    item.parent_id = format!(
                        "radio-paradise:channel:{}:liveplaylist",
                        slug
                    );

                    if resource.url.starts_with('/') {
                        resource.url = format!("{}{}", self.base_url, resource.url);
                    }
                }
            }

            if item.genre.is_none() {
                item.genre = Some("Radio Paradise".to_string());
            }

            if let Some(art) = item.album_art.as_mut() {
                if art.starts_with('/') {
                    *art = format!("{}{}", self.base_url, art);
                }
            } else {
                item.album_art = Some(self.default_cover_url());
            }
        }

        Ok(items)
    }

    /// Get a single item from the live playlist by pk
    #[cfg(feature = "playlist")]
    async fn get_live_playlist_item(&self, slug: &str, pk: &str) -> Result<Item> {
        let items = self.get_live_playlist_items(slug, 0, 1000).await?;
        let expected_id = format!(
            "radio-paradise:channel:{}:liveplaylist:track:{}",
            slug, pk
        );
        for item in items {
            if item.id == expected_id {
                return Ok(item);
            }
        }
        Err(MusicSourceError::ObjectNotFound(format!(
            "Track with pk {} not found in live playlist",
            pk
        )))
    }

}

/// Types of object IDs in the Radio Paradise source
#[derive(Debug, Clone, PartialEq)]
enum ObjectIdType {
    Root,
    Channel { slug: String },
    LiveStream { slug: String },
    LivePlaylist { slug: String },
    LivePlaylistTrack { slug: String, pk: String },
    History { slug: String },
    HistoryTrack { slug: String, pk: String },
    Unknown,
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
        Ok(Container {
            id: "radio-paradise".to_string(),
            parent_id: "0".to_string(),
            restricted: Some("1".to_string()),
            // childCount retiré pour éviter les soucis de compatibilité côté CP
            child_count: None,
            searchable: Some("1".to_string()),
            title: "Radio Paradise".to_string(),
            class: "object.container".to_string(),
            containers: vec![],
            items: vec![],
        })
    }

    async fn browse(&self, object_id: &str) -> Result<BrowseResult> {
        match Self::parse_object_id(object_id) {
            ObjectIdType::Root => {
                // Return the 4 channel containers
                let containers: Vec<Container> = ALL_CHANNELS
                    .iter()
                    .map(|ch| self.build_channel_container(ch))
                    .collect();

                Ok(BrowseResult::Containers(containers))
            }

            ObjectIdType::Channel { slug } => {
                // Return live stream item + history container
                let descriptor = Self::get_channel_by_slug(&slug).ok_or_else(|| {
                    MusicSourceError::ObjectNotFound(format!("Unknown channel: {}", slug))
                })?;

                let live_item = self.build_live_stream_item(descriptor);
                let live_playlist_container = self.build_live_playlist_container(descriptor);

                #[cfg(feature = "playlist")]
                let history_container = self.build_history_container_with_count(descriptor).await;
                #[cfg(not(feature = "playlist"))]
                let history_container = self.build_history_container(descriptor);

                Ok(BrowseResult::Mixed {
                    containers: vec![live_playlist_container, history_container],
                    items: vec![live_item],
                })
            }

            ObjectIdType::History { slug } => {
                // Return history container (for BrowseMetadata) and items (for BrowseDirectChildren)
                // The content_handler will filter out the container when browsing direct children
                let descriptor = Self::get_channel_by_slug(&slug).ok_or_else(|| {
                    MusicSourceError::ObjectNotFound(format!("Unknown channel: {}", slug))
                })?;

                #[cfg(feature = "playlist")]
                {
                    let history_container = self.build_history_container_with_count(descriptor).await;
                    let items = self.get_history_items(&slug, 0, 100).await?;
                    Ok(BrowseResult::Mixed {
                        containers: vec![history_container],
                        items,
                    })
                }

                #[cfg(not(feature = "playlist"))]
                {
                    // If playlist feature is disabled, return just the container
                    let history_container = self.build_history_container(descriptor);
                    Ok(BrowseResult::Containers(vec![history_container]))
                }
            }

            ObjectIdType::LiveStream { slug } => {
                // Return metadata for the live stream item
                let descriptor = Self::get_channel_by_slug(&slug).ok_or_else(|| {
                    MusicSourceError::ObjectNotFound(format!("Unknown channel: {}", slug))
                })?;
                let item = self.build_live_stream_item(descriptor);
                Ok(BrowseResult::Items(vec![item]))
            }

            ObjectIdType::LivePlaylist { slug } => {
                // Playlist du live : container + items
                let descriptor = Self::get_channel_by_slug(&slug).ok_or_else(|| {
                    MusicSourceError::ObjectNotFound(format!("Unknown channel: {}", slug))
                })?;

                #[cfg(feature = "playlist")]
                {
                    let container = self.build_live_playlist_container(descriptor);
                    let items = self.get_live_playlist_items(&slug, 0, 100).await?;
                    Ok(BrowseResult::Mixed {
                        containers: vec![container],
                        items,
                    })
                }

                #[cfg(not(feature = "playlist"))]
                {
                    let container = self.build_live_playlist_container(descriptor);
                    Ok(BrowseResult::Containers(vec![container]))
                }
            }

            ObjectIdType::HistoryTrack { slug, pk } => {
                // Return metadata for the history track item
                let item = self.get_item(object_id).await?;
                Ok(BrowseResult::Items(vec![item]))
            }

            ObjectIdType::LivePlaylistTrack { slug, pk } => {
                // Détails d'un titre du live (playlist live)
                #[cfg(feature = "playlist")]
                {
                    let item = self.get_live_playlist_item(&slug, &pk).await?;
                    Ok(BrowseResult::Items(vec![item]))
                }

                #[cfg(not(feature = "playlist"))]
                {
                    let _ = (slug, pk);
                    Err(MusicSourceError::NotSupported(
                        "Playlist feature not enabled".to_string(),
                    ))
                }
            }

            ObjectIdType::Unknown => Err(MusicSourceError::ObjectNotFound(format!(
                "Unknown object ID: {}",
                object_id
            ))),
        }
    }

    async fn resolve_uri(&self, object_id: &str) -> Result<String> {
        match Self::parse_object_id(object_id) {
            ObjectIdType::LiveStream { slug } => {
                // Return live stream URL
                Ok(self.build_live_url(&slug))
            }

            ObjectIdType::HistoryTrack { pk, .. } => {
                // Return cached audio URL
                Ok(format!("{}/cache/audio/{}", self.base_url, pk))
            }

            ObjectIdType::LivePlaylistTrack { pk, .. } => {
                // Return cached audio URL
                Ok(format!("{}/cache/audio/{}", self.base_url, pk))
            }

            _ => Err(MusicSourceError::ObjectNotFound(format!(
                "Cannot resolve URI for object: {}",
                object_id
            ))),
        }
    }

    fn capabilities(&self) -> SourceCapabilities {
        SourceCapabilities {
            supports_fifo: self.supports_fifo(),
            supports_search: false,
            supports_favorites: false,
            supports_playlists: false,
            supports_user_content: false,
            supports_high_res_audio: true,
            max_sample_rate: Some(44100),
            supports_multiple_formats: true,
            supports_advanced_search: false,
            supports_pagination: false,
        }
    }

    async fn get_available_formats(&self, object_id: &str) -> Result<Vec<AudioFormat>> {
        match Self::parse_object_id(object_id) {
            ObjectIdType::LiveStream { .. } => Ok(vec![
                AudioFormat {
                    format_id: "flac".to_string(),
                    mime_type: "audio/flac".to_string(),
                    sample_rate: Some(44100),
                    bit_depth: Some(16),
                    bitrate: None,
                    channels: Some(2),
                },
                AudioFormat {
                    format_id: "ogg-flac".to_string(),
                    mime_type: "audio/ogg".to_string(),
                    sample_rate: Some(44100),
                    bit_depth: Some(16),
                    bitrate: None,
                    channels: Some(2),
                },
            ]),
            ObjectIdType::HistoryTrack { .. } => Ok(vec![AudioFormat {
                format_id: "flac".to_string(),
                mime_type: "audio/flac".to_string(),
                sample_rate: Some(44100),
                bit_depth: Some(16),
                bitrate: None,
                channels: Some(2),
            }]),
            ObjectIdType::LivePlaylistTrack { .. } => Ok(vec![AudioFormat {
                format_id: "flac".to_string(),
                mime_type: "audio/flac".to_string(),
                sample_rate: Some(44100),
                bit_depth: Some(16),
                bitrate: None,
                channels: Some(2),
            }]),
            _ => Err(MusicSourceError::ObjectNotFound(format!(
                "Cannot list formats for object: {}",
                object_id
            ))),
        }
    }

    async fn get_item(&self, object_id: &str) -> Result<Item> {
        match Self::parse_object_id(object_id) {
            ObjectIdType::LiveStream { slug } => {
                // Try to fetch current metadata from live stream
                if let Ok(Some(item)) = self.fetch_live_metadata(&slug).await {
                    return Ok(item);
                }

                // Fallback to static item if metadata fetch fails
                let descriptor = Self::get_channel_by_slug(&slug).ok_or_else(|| {
                    MusicSourceError::ObjectNotFound(format!("Unknown channel: {}", slug))
                })?;
                Ok(self.build_live_stream_item(descriptor))
            }

            ObjectIdType::HistoryTrack { slug, pk } => {
                // Get from history playlist
                #[cfg(feature = "playlist")]
                {
                    let playlist_id = Self::history_playlist_id(&slug);
                    let manager = pmoplaylist::PlaylistManager();
                    let reader = manager.get_read_handle(&playlist_id).await.map_err(|e| {
                        MusicSourceError::BrowseError(format!(
                            "Failed to get playlist {}: {}",
                            playlist_id, e
                        ))
                    })?;

                    // Try to find the item with this pk
                    let items = reader.to_items(1000).await.map_err(|e| {
                        MusicSourceError::BrowseError(format!(
                            "Failed to read playlist entries: {}",
                            e
                        ))
                    })?;

                    // Ajuster les IDs/parent_id/URL pour coller au schéma Radio Paradise,
                    // comme dans get_history_items.
                    let mut adjusted = Vec::new();
                    for mut item in items {
                        if let Some(resource) = item.resources.first_mut() {
                            if let Some(pk2) = resource.url.split('/').last() {
                                item.id =
                                    format!("radio-paradise:channel:{}:history:track:{}", slug, pk2);
                                item.parent_id =
                                    format!("radio-paradise:channel:{}:history", slug);

                                if resource.url.starts_with('/') {
                                    resource.url =
                                        format!("{}{}", self.base_url, resource.url);
                                }
                            }
                        }
                        if item.genre.is_none() {
                            item.genre = Some("Radio Paradise".to_string());
                        }
                        adjusted.push(item);
                    }

                    // Find the item matching this pk in the item ID
                    let expected_id = format!("radio-paradise:channel:{}:history:track:{}", slug, pk);
                    for item in adjusted {
                        if item.id == expected_id {
                            return Ok(item);
                        }
                    }

                    Err(MusicSourceError::ObjectNotFound(format!(
                        "Track with pk {} not found in history",
                        pk
                    )))
                }

                #[cfg(not(feature = "playlist"))]
                {
                    let _ = (slug, pk);
                    Err(MusicSourceError::NotSupported(
                        "Playlist feature not enabled".to_string(),
                    ))
                }
            }

            ObjectIdType::LivePlaylistTrack { slug, pk } => {
                #[cfg(feature = "playlist")]
                {
                    let playlist_id = Self::live_playlist_id(&slug);
                    let manager = pmoplaylist::PlaylistManager();
                    let reader = manager.get_read_handle(&playlist_id).await.map_err(|e| {
                        MusicSourceError::BrowseError(format!(
                            "Failed to get live playlist {}: {}",
                            playlist_id, e
                        ))
                    })?;

                    let items = reader.to_items(1000).await.map_err(|e| {
                        MusicSourceError::BrowseError(format!(
                            "Failed to read live playlist entries: {}",
                            e
                        ))
                    })?;

                    for mut item in items {
                        if let Some(resource) = item.resources.first_mut() {
                            if let Some(pk2) = resource.url.split('/').last() {
                                item.id = format!(
                                    "radio-paradise:channel:{}:liveplaylist:track:{}",
                                    slug, pk2
                                );
                                item.parent_id = format!(
                                    "radio-paradise:channel:{}:liveplaylist",
                                    slug
                                );

                                if resource.url.starts_with('/') {
                                    resource.url = format!("{}{}", self.base_url, resource.url);
                                }
                            }
                        }

                        if item.genre.is_none() {
                            item.genre = Some("Radio Paradise".to_string());
                        }

                        if let Some(art) = item.album_art.as_mut() {
                            if art.starts_with('/') {
                                *art = format!("{}{}", self.base_url, art);
                            }
                        } else {
                            item.album_art = Some(self.default_cover_url());
                        }

                        let expected_id = format!(
                            "radio-paradise:channel:{}:liveplaylist:track:{}",
                            slug, pk
                        );
                        if item.id == expected_id {
                            return Ok(item);
                        }
                    }

                    Err(MusicSourceError::ObjectNotFound(format!(
                        "Track with pk {} not found in live playlist",
                        pk
                    )))
                }

                #[cfg(not(feature = "playlist"))]
                {
                    let _ = (slug, pk);
                    Err(MusicSourceError::NotSupported(
                        "Playlist feature not enabled".to_string(),
                    ))
                }
            }

            _ => Err(MusicSourceError::ObjectNotFound(format!(
                "Cannot get item for object: {}",
                object_id
            ))),
        }
    }

    fn supports_fifo(&self) -> bool {
        // History playlists are FIFO
        cfg!(feature = "playlist")
    }

    async fn append_track(&self, _track: Item) -> Result<()> {
        // Tracks are added automatically by FlacCacheSink
        Err(MusicSourceError::NotSupported(
            "Tracks are automatically added to history by the streaming system".to_string(),
        ))
    }

    async fn remove_oldest(&self) -> Result<Option<Item>> {
        // Managed automatically by playlist FIFO
        Ok(None)
    }

    async fn update_id(&self) -> u32 {
        *self.update_counter.read().await
    }

    async fn last_change(&self) -> Option<SystemTime> {
        Some(*self.last_change.read().await)
    }

    async fn get_items(&self, offset: usize, count: usize) -> Result<Vec<Item>> {
        // For Radio Paradise, we don't have a global FIFO
        // Each channel has its own history
        // Return empty for now - clients should browse specific channel histories
        let _ = (offset, count);
        Ok(vec![])
    }
}
