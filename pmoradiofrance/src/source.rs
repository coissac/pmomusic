//! MusicSource implementation for Radio France
//!
//! This module implements the `MusicSource` trait from `pmosource` for Radio France,
//! providing UPnP/DLNA integration with dynamic container generation.

use crate::client::RadioFranceClient;
use crate::error::Result;
use crate::metadata_cache::MetadataCache;
use crate::playlist::StationGroups;
use pmoconfig::Config;
use pmodidl::{Container, Item};
use pmosource::{async_trait, BrowseResult, MusicSource, MusicSourceError, SourceCapabilities};
use std::sync::Arc;
use std::time::SystemTime;
use tokio::sync::RwLock;

#[cfg(feature = "server")]
use pmoupnp;

/// Default image for Radio France source (embedded in binary)
pub const RADIOFRANCE_DEFAULT_IMAGE: &[u8] = include_bytes!("../assets/radiofrance-logo.webp");

/// Radio France music source
///
/// Provides access to ~70 Radio France stations via UPnP/DLNA with:
/// - Cache de métadonnées avec TTL et système d'événements
/// - Construction dynamique de containers DIDL
/// - Notifications GENA pour les playlists
pub struct RadioFranceSource {
    /// Cache de métadonnées centralisé
    metadata_cache: Arc<MetadataCache>,

    /// Server base URL
    server_base_url: String,

    /// Update counter for change tracking
    update_id: Arc<RwLock<u32>>,

    /// Last change timestamp
    last_change: Arc<RwLock<Option<SystemTime>>>,

    /// Callback for notifying container updates (UPnP GENA)
    container_notifier: Option<Arc<dyn Fn(&[String]) + Send + Sync + 'static>>,
}

impl RadioFranceSource {
    /// Create a new Radio France source
    ///
    /// # Arguments
    ///
    /// * `config` - Configuration for cache and discovery
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
    #[cfg(feature = "cache")]
    pub async fn new(config: Arc<Config>) -> Result<Self> {
        let client = RadioFranceClient::new().await?;
        let cover_cache = pmoupnp::cache_registry::get_cover_cache()
            .ok_or_else(|| crate::error::Error::Other("Cover cache not initialized".to_string()))?;

        // TODO: Récupérer server_base_url depuis config
        let server_base_url = "http://localhost:8080".to_string();

        let metadata_cache = Arc::new(MetadataCache::new(
            client,
            cover_cache,
            server_base_url.clone(),
            config,
        ));

        let update_id = Arc::new(RwLock::new(0));
        let last_change = Arc::new(RwLock::new(None));

        let source = Self {
            metadata_cache,
            server_base_url,
            update_id,
            last_change,
            container_notifier: None,
        };

        Ok(source)
    }

    /// Set the container notifier for UPnP GENA events
    pub fn with_container_notifier(
        mut self,
        notifier: Arc<dyn Fn(&[String]) + Send + Sync + 'static>,
    ) -> Self {
        // S'abonner aux événements du cache de métadonnées
        let container_notifier = Arc::new(notifier.clone());
        let update_id = self.update_id.clone();
        let last_change = self.last_change.clone();

        self.metadata_cache.subscribe(Arc::new(move |slug: &str| {
            let slug = slug.to_string();
            let update_id = update_id.clone();
            let last_change = last_change.clone();
            let container_notifier = container_notifier.clone();

            tokio::spawn(async move {
                *update_id.write().await += 1;
                *last_change.write().await = Some(SystemTime::now());

                // Notifier le container de playlist (pas l'item)
                container_notifier(&[format!("radiofrance:{}", slug)]);
            });
        }));

        self.container_notifier = Some(notifier);
        self
    }

    /// Create a new Radio France source from the cache registry
    ///
    /// This is the recommended way to create a source when using the UPnP server.
    ///
    /// # Arguments
    ///
    /// * `config` - Configuration
    /// * `base_url` - Base URL for streaming server
    #[cfg(feature = "server")]
    pub async fn from_registry(config: Arc<Config>, base_url: impl Into<String>) -> Result<Self> {
        let client = RadioFranceClient::new().await?;
        let cover_cache = pmoupnp::cache_registry::get_cover_cache()
            .ok_or_else(|| crate::error::Error::Other("Cover cache not initialized".to_string()))?;
        let server_base_url = base_url.into();

        let metadata_cache = Arc::new(MetadataCache::new(
            client,
            cover_cache,
            server_base_url.clone(),
            config,
        ));

        let update_id = Arc::new(RwLock::new(0));
        let last_change = Arc::new(RwLock::new(None));

        Ok(Self {
            metadata_cache,
            server_base_url,
            update_id,
            last_change,
            container_notifier: None,
        })
    }
}

impl RadioFranceSource {
    /// Get the list of all Radio France stations
    pub async fn get_stations(&self) -> Result<Vec<crate::models::Station>> {
        self.metadata_cache.get_stations().await
    }

    /// Get live metadata for a station
    pub async fn get_live_metadata(&self, slug: &str) -> Result<crate::models::LiveResponse> {
        self.metadata_cache.get_live_metadata(slug).await
    }

    /// Get the HiFi stream URL for a station
    pub async fn get_stream_url(&self, slug: &str) -> Result<String> {
        self.metadata_cache.get_stream_url(slug).await
    }
}

impl std::fmt::Debug for RadioFranceSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RadioFranceSource")
            .field("server_base_url", &self.server_base_url)
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
                // Niveau 0: retourne le Container avec les groupes
                let stations = self
                    .metadata_cache
                    .get_stations()
                    .await
                    .map_err(|e| MusicSourceError::BrowseError(e.to_string()))?;

                let groups = StationGroups::from_stations(stations);
                let container = groups
                    .to_didl(&self.metadata_cache, &self.server_base_url)
                    .await
                    .map_err(|e| MusicSourceError::BrowseError(e.to_string()))?;

                Ok(BrowseResult::Containers(container.containers))
            }
            id if id.starts_with("radiofrance:group:") || id == "radiofrance:ici" => {
                // Niveau 1: browse d'un groupe
                let slug = id
                    .strip_prefix("radiofrance:group:")
                    .or_else(|| {
                        if id == "radiofrance:ici" {
                            Some("ici")
                        } else {
                            None
                        }
                    })
                    .ok_or_else(|| MusicSourceError::ObjectNotFound(id.to_string()))?;

                let stations = self
                    .metadata_cache
                    .get_stations()
                    .await
                    .map_err(|e| MusicSourceError::BrowseError(e.to_string()))?;

                let groups = StationGroups::from_stations(stations);
                let group = groups
                    .groups
                    .iter()
                    .find(|g| g.slug() == slug)
                    .ok_or_else(|| MusicSourceError::ObjectNotFound(id.to_string()))?;

                let container = group
                    .to_didl(&self.metadata_cache, &self.server_base_url)
                    .await
                    .map_err(|e| MusicSourceError::BrowseError(e.to_string()))?;

                // Si c'est une playlist (1 station), retourner le container lui-même
                // Sinon retourner ses sous-containers
                if container.class == "object.container.playlistContainer" {
                    Ok(BrowseResult::Containers(vec![container]))
                } else {
                    Ok(BrowseResult::Containers(container.containers))
                }
            }
            id if id.starts_with("radiofrance:") && !id.contains(":stream") => {
                // Niveau 2: browse d'une station (playlist)
                let slug = id
                    .strip_prefix("radiofrance:")
                    .ok_or_else(|| MusicSourceError::ObjectNotFound(id.to_string()))?;

                let stations = self
                    .metadata_cache
                    .get_stations()
                    .await
                    .map_err(|e| MusicSourceError::BrowseError(e.to_string()))?;

                let station = stations
                    .iter()
                    .find(|s| s.slug == slug)
                    .ok_or_else(|| MusicSourceError::ObjectNotFound(id.to_string()))?;

                let container = station
                    .to_didl(&self.metadata_cache, &self.server_base_url)
                    .await
                    .map_err(|e| MusicSourceError::BrowseError(e.to_string()))?;

                // Retourner les items de la playlist
                Ok(BrowseResult::Items(container.items))
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

        let stations = self
            .metadata_cache
            .get_stations()
            .await
            .map_err(|e| MusicSourceError::BrowseError(e.to_string()))?;

        let station = stations
            .iter()
            .find(|s| s.slug == slug)
            .ok_or_else(|| MusicSourceError::ObjectNotFound(object_id.to_string()))?;

        let container = station
            .to_didl(&self.metadata_cache, &self.server_base_url)
            .await
            .map_err(|e| MusicSourceError::BrowseError(e.to_string()))?;

        container
            .items
            .into_iter()
            .next()
            .ok_or_else(|| MusicSourceError::ObjectNotFound(object_id.to_string()))
    }

    async fn resolve_uri(&self, object_id: &str) -> pmosource::Result<String> {
        // Extract station slug from object_id (format: radiofrance:{slug}:stream)
        let _slug = object_id
            .strip_prefix("radiofrance:")
            .and_then(|s| s.strip_suffix(":stream"))
            .ok_or_else(|| MusicSourceError::ObjectNotFound(object_id.to_string()))?;

        // Get the item to extract the stream URL
        let item = self.get_item(object_id).await?;
        item.resources
            .first()
            .map(|r| r.url.clone())
            .ok_or_else(|| MusicSourceError::ObjectNotFound(object_id.to_string()))
    }

    fn supports_fifo(&self) -> bool {
        false
    }

    async fn append_track(&self, _item: Item) -> pmosource::Result<()> {
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

    async fn get_items(&self, _offset: usize, _count: usize) -> pmosource::Result<Vec<Item>> {
        Ok(vec![])
    }
}
