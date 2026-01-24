//! MusicSource implementation for Radio France
//!
//! This module implements the `MusicSource` trait from `pmosource` for Radio France,
//! providing UPnP/DLNA integration with dynamic container generation.

use crate::error::Result;
use crate::models::{Station, StationType};
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

/// Default image for Radio France source (embedded in binary)
pub const RADIOFRANCE_DEFAULT_IMAGE: &[u8] = include_bytes!("../assets/radiofrance-logo.webp");

/// Radio France music source
///
/// Provides access to ~70 Radio France stations via UPnP/DLNA with:
/// - Dynamic container generation based on station structure
/// - Automatic metadata refresh for active streams
/// - Hierarchical organization (standalone, groups, local radios)
pub struct RadioFranceSource {
    /// Stateful client with automatic caching
    pub(crate) client: RadioFranceStatefulClient,

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

    /// Callback for notifying container updates (UPnP GENA)
    container_notifier: Option<Arc<dyn Fn(&[String]) + Send + Sync + 'static>>,
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

        let source = Self {
            client,
            refresh_handles: Arc::new(RwLock::new(HashMap::new())),
            #[cfg(feature = "cache")]
            cover_cache: None,
            server_base_url: None,
            update_id: Arc::new(RwLock::new(0)),
            last_change: Arc::new(RwLock::new(None)),
            container_notifier: None,
        };

        // S'abonner aux événements du cache pour les notifications GENA
        let container_notifier = source.container_notifier.clone();
        let update_id = source.update_id.clone();
        let last_change = source.last_change.clone();

        source
            .client
            .subscribe_to_updates(Arc::new(move |slug: &str| {
                let slug = slug.to_string();
                let update_id = update_id.clone();
                let last_change = last_change.clone();
                let container_notifier = container_notifier.clone();

                // Spawn async task car le callback n'est pas async
                tokio::spawn(async move {
                    *update_id.write().await += 1;
                    *last_change.write().await = Some(SystemTime::now());

                    if let Some(ref notifier) = container_notifier {
                        // IMPORTANT : Notifier le container de playlist (pas l'item)
                        // Le Control Point est abonné à "radiofrance:fip" (la playlist)
                        // et non à "radiofrance:fip:stream" (l'item)
                        notifier(&[format!("radiofrance:{}", slug)]);
                    }
                });
            }));

        Ok(source)
    }

    /// Set the container notifier for UPnP GENA events
    pub fn with_container_notifier(
        mut self,
        notifier: Arc<dyn Fn(&[String]) + Send + Sync + 'static>,
    ) -> Self {
        self.container_notifier = Some(notifier);
        self
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

        let source = Self {
            client,
            refresh_handles: Arc::new(RwLock::new(HashMap::new())),
            #[cfg(feature = "cache")]
            cover_cache,
            server_base_url: Some(base_url.into()),
            update_id: Arc::new(RwLock::new(0)),
            last_change: Arc::new(RwLock::new(None)),
            container_notifier: None,
        };

        // S'abonner aux événements du cache pour les notifications GENA
        let container_notifier = source.container_notifier.clone();
        let update_id = source.update_id.clone();
        let last_change = source.last_change.clone();

        source
            .client
            .subscribe_to_updates(Arc::new(move |slug: &str| {
                let slug = slug.to_string();
                let update_id = update_id.clone();
                let last_change = last_change.clone();
                let container_notifier = container_notifier.clone();

                tokio::spawn(async move {
                    *update_id.write().await += 1;
                    *last_change.write().await = Some(SystemTime::now());

                    if let Some(ref notifier) = container_notifier {
                        notifier(&[format!("radiofrance:{}", slug)]);
                    }
                });
            }));

        Ok(source)
    }

    /// Start metadata refresh task for a station
    ///
    /// Appelée par le proxy du stream audio. Cette méthode lance une tâche
    /// qui appelle `get_live_metadata()` périodiquement (toutes les secondes).
    /// Le cache avec TTL gère le refresh réel, et les événements GENA sont
    /// déclenchés automatiquement par le système d'abonnement.
    pub async fn start_metadata_refresh(&self, station_slug: &str) -> Result<()> {
        let mut handles = self.refresh_handles.write().await;

        // If already running, do nothing
        if handles.contains_key(station_slug) {
            return Ok(());
        }

        let client = self.client.clone();
        let slug = station_slug.to_string();

        let handle = tokio::spawn(async move {
            loop {
                // Appeler simplement get_live_metadata
                // Si le cache est valide, retour immédiat
                // Si expiré, fetch API + mise à jour cache + notification GENA
                let _ = client.get_live_metadata(&slug).await;

                // Attendre 1 seconde avant le prochain check
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
        });

        handles.insert(station_slug.to_string(), handle);

        #[cfg(feature = "logging")]
        tracing::debug!(
            "Started metadata refresh polling for station: {}",
            station_slug
        );

        Ok(())
    }

    /// Stop metadata refresh task for a station
    pub async fn stop_metadata_refresh(&self, station_slug: &str) {
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

        // 1. Standalone stations → playlist containers (plus des items directs)
        #[cfg(feature = "logging")]
        tracing::debug!(
            "Building {} standalone station playlist containers",
            groups.standalone.len()
        );

        for station in &groups.standalone {
            containers.push(self.build_station_playlist(station).await?);
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
            "Container tree built: {} containers (all playlists)",
            containers.len()
        );

        Ok(Container {
            id: "radiofrance".to_string(),
            parent_id: "0".to_string(),
            restricted: Some("1".to_string()),
            child_count: Some(containers.len().to_string()),
            searchable: Some("0".to_string()),
            title: "Radio France".to_string(),
            class: "object.container".to_string(),
            artist: None,
            album_art: None,
            containers,
            items: vec![], // Plus d'items directs - tout est dans des playlists
        })
    }

    /// Build a container for a station group (main + webradios)
    /// Returns an empty container - items will be built when browsing into it
    async fn build_station_container(&self, group: &StationGroup) -> Result<Container> {
        let child_count = 1 + group.webradios.len(); // main + webradios

        // Utiliser le logo par défaut si server_base_url est configuré
        let album_art = self.server_base_url.as_ref().map(|base| {
            format!(
                "{}/api/radiofrance/default-logo",
                base.trim_end_matches('/')
            )
        });

        Ok(Container {
            id: format!("radiofrance:group:{}", group.main.slug),
            parent_id: "radiofrance".to_string(),
            restricted: Some("1".to_string()),
            child_count: Some(child_count.to_string()),
            searchable: Some("0".to_string()),
            title: group.main.name.clone(),
            class: "object.container".to_string(),
            artist: None,
            album_art,
            containers: vec![],
            items: vec![],
        })
    }

    /// Build the "Radios ICI" container
    /// Returns an empty container - items will be built when browsing into it
    async fn build_ici_container(&self, local_radios: &[Station]) -> Result<Container> {
        // Utiliser le logo par défaut si server_base_url est configuré
        let album_art = self.server_base_url.as_ref().map(|base| {
            format!(
                "{}/api/radiofrance/default-logo",
                base.trim_end_matches('/')
            )
        });

        Ok(Container {
            id: "radiofrance:ici".to_string(),
            parent_id: "radiofrance".to_string(),
            restricted: Some("1".to_string()),
            child_count: Some(local_radios.len().to_string()),
            searchable: Some("0".to_string()),
            title: "Radios ICI".to_string(),
            class: "object.container".to_string(),
            artist: None,
            album_art,
            containers: vec![],
            items: vec![],
        })
    }

    /// Construit le container de playlist avec son unique item (métadonnées cohérentes)
    ///
    /// Cette méthode crée un container de type `playlistContainer` contenant un seul item.
    /// Un seul appel au cache garantit la cohérence des métadonnées entre le container et l'item.
    async fn build_station_playlist(&self, station: &Station) -> Result<Container> {
        #[cfg(feature = "logging")]
        tracing::debug!(
            "Building station playlist for: {} ({})",
            station.name,
            station.slug
        );

        // UN SEUL appel au cache - garantit cohérence container/item
        let metadata = self.client.get_live_metadata(&station.slug).await?;

        // Build l'item de stream avec pmoDidl
        #[cfg(feature = "cache")]
        let mut item = StationPlaylist::build_item_from_metadata(
            station,
            &metadata,
            self.cover_cache.as_ref(),
            self.server_base_url.as_deref(),
        )
        .await?;

        #[cfg(not(feature = "cache"))]
        let mut item = StationPlaylist::build_item_from_metadata_sync(
            station,
            &metadata,
            self.server_base_url.as_deref(),
        )?;

        // Parent_id de l'item = le container de playlist
        let playlist_id = format!("radiofrance:{}", station.slug);
        item.parent_id = playlist_id.clone();

        // Construire le container avec les MÊMES métadonnées que l'item
        let container = Container {
            id: playlist_id,
            parent_id: self.get_parent_id_for_station(station),
            restricted: Some("1".to_string()),
            child_count: Some("1".to_string()), // Toujours 1 item
            searchable: Some("0".to_string()),
            // Métadonnées identiques à l'item
            title: item.title.clone(),
            artist: item.artist.clone(),
            album_art: item.album_art.clone(),
            class: "object.container.playlistContainer".to_string(),
            containers: vec![],
            items: vec![item], // L'item est inclus dans le container
        };

        #[cfg(feature = "logging")]
        tracing::debug!(
            "Built playlist container for {}: {} items",
            station.slug,
            container.items.len()
        );

        Ok(container)
    }

    /// Détermine le parent_id selon le type de station
    fn get_parent_id_for_station(&self, station: &Station) -> String {
        match &station.station_type {
            StationType::Webradio { parent_station } => {
                format!("radiofrance:group:{}", parent_station)
            }
            StationType::LocalRadio { .. } => "radiofrance:ici".to_string(),
            StationType::Main => "radiofrance".to_string(),
        }
    }
}

impl std::fmt::Debug for RadioFranceSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RadioFranceSource")
            .field("client", &self.client)
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

                // Retourne uniquement des containers (playlists + groupes)
                Ok(BrowseResult::Containers(container.containers))
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

                // Build playlist containers for this group (main + webradios)
                // Paralléliser les fetches pour éviter les timeouts
                let mut futures = vec![self.build_station_playlist(&group.main)];
                for webradio in &group.webradios {
                    futures.push(self.build_station_playlist(webradio));
                }

                let results = futures::future::join_all(futures).await;

                let mut containers = Vec::new();
                for result in results {
                    let container =
                        result.map_err(|e| MusicSourceError::BrowseError(e.to_string()))?;
                    containers.push(container);
                }

                Ok(BrowseResult::Containers(containers))
            }
            "radiofrance:ici" => {
                let stations = self
                    .client
                    .get_stations()
                    .await
                    .map_err(|e| MusicSourceError::BrowseError(e.to_string()))?;
                let groups = StationGroups::from_stations(stations);

                // Build playlist containers for local radios only
                let mut containers = Vec::new();
                for station in &groups.local_radios {
                    let container = self
                        .build_station_playlist(station)
                        .await
                        .map_err(|e| MusicSourceError::BrowseError(e.to_string()))?;

                    containers.push(container);
                }

                Ok(BrowseResult::Containers(containers))
            }
            id if id.starts_with("radiofrance:") && !id.contains(":stream") => {
                // Browse d'un container de playlist (ex: radiofrance:fip)
                // Le container contient déjà son item, on retourne juste le container
                let slug = id
                    .strip_prefix("radiofrance:")
                    .ok_or_else(|| MusicSourceError::ObjectNotFound(id.to_string()))?;

                // Trouver la station correspondante
                let stations = self
                    .client
                    .get_stations()
                    .await
                    .map_err(|e| MusicSourceError::BrowseError(e.to_string()))?;

                let station = stations
                    .iter()
                    .find(|s| s.slug == slug)
                    .ok_or_else(|| MusicSourceError::ObjectNotFound(id.to_string()))?;

                let container = self
                    .build_station_playlist(station)
                    .await
                    .map_err(|e| MusicSourceError::BrowseError(e.to_string()))?;

                // Retourner le container lui-même (qui contient l'item)
                Ok(BrowseResult::Containers(vec![container]))
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

        // Trouver la station correspondante
        let stations = self
            .client
            .get_stations()
            .await
            .map_err(|e| MusicSourceError::BrowseError(e.to_string()))?;

        let station = stations
            .iter()
            .find(|s| s.slug == slug)
            .ok_or_else(|| MusicSourceError::ObjectNotFound(object_id.to_string()))?;

        // Construire le container de playlist et extraire l'item
        let container = self
            .build_station_playlist(station)
            .await
            .map_err(|e| MusicSourceError::BrowseError(e.to_string()))?;

        // Extraire l'unique item du container
        container
            .items
            .into_iter()
            .next()
            .ok_or_else(|| MusicSourceError::ObjectNotFound(object_id.to_string()))
    }

    async fn resolve_uri(&self, object_id: &str) -> pmosource::Result<String> {
        // Extract station slug from object_id (format: radiofrance:{slug}:stream)
        let slug = object_id
            .strip_prefix("radiofrance:")
            .and_then(|s| s.strip_suffix(":stream"))
            .ok_or_else(|| MusicSourceError::ObjectNotFound(object_id.to_string()))?;

        // Start metadata refresh for this station (if not already running)
        let _ = self.start_metadata_refresh(slug).await;

        // Get the item to extract the stream URL
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
