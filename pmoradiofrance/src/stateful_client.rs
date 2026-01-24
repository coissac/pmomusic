//! Stateful client for Radio France with automatic caching
//!
//! This module provides a higher-level client that automatically manages
//! station discovery caching through pmoconfig, providing a simpler API
//! for integration into PMOMusic.
//!
//! # Example
//!
//! ```no_run
//! use pmoradiofrance::RadioFranceStatefulClient;
//! use pmoconfig::get_config;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let config = get_config();
//!     let client = RadioFranceStatefulClient::new(config).await?;
//!
//!     // Get stations (automatically cached with 7-day TTL)
//!     let stations = client.get_stations().await?;
//!
//!     // Get live metadata (handles caching internally)
//!     let metadata = client.get_live_metadata("franceculture").await?;
//!
//!     Ok(())
//! }
//! ```

use crate::client::RadioFranceClient;
use crate::config_ext::RadioFranceConfigExt;
use crate::error::{Error, Result};
use crate::models::{LiveResponse, Station};
use pmoconfig::Config;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

/// Type de callback pour les notifications de mise à jour de métadonnées
pub type MetadataUpdateCallback = Arc<dyn Fn(&str) + Send + Sync>;

/// Cache entry for live metadata
#[derive(Debug, Clone)]
struct LiveMetadataCache {
    /// Cached metadata
    metadata: LiveResponse,
    /// When the cache should be invalidated (based on delayToRefresh)
    valid_until: SystemTime,
}

impl LiveMetadataCache {
    /// Create a new cache entry
    fn new(metadata: LiveResponse) -> Self {
        let delay = Duration::from_millis(metadata.delay_to_refresh);
        let valid_until = SystemTime::now() + delay;

        Self {
            metadata,
            valid_until,
        }
    }

    /// Check if the cache is still valid
    fn is_valid(&self) -> bool {
        SystemTime::now() < self.valid_until
    }

    /// Get the remaining time until the cache expires
    #[cfg(feature = "logging")]
    fn remaining_ttl(&self) -> Duration {
        self.valid_until
            .duration_since(SystemTime::now())
            .unwrap_or(Duration::ZERO)
    }
}

/// Stateful Radio France client with automatic caching
///
/// This client wraps `RadioFranceClient` and adds:
/// - Automatic station list caching via pmoconfig
/// - Live metadata caching (in-memory, respecting delayToRefresh)
/// - Simple high-level API for PMOMusic integration
///
/// # Caching Strategy
///
/// - **Station List**: Cached in pmoconfig with 7-day TTL (configurable)
/// - **Live Metadata**: Cached in-memory per station, TTL from API's delayToRefresh
///
/// # Thread Safety
///
/// This client is thread-safe (Clone + Send + Sync) and can be shared
/// across async tasks.
#[derive(Clone)]
pub struct RadioFranceStatefulClient {
    /// Underlying HTTP client
    client: RadioFranceClient,
    /// Configuration handle (Arc for sharing)
    config: Arc<Config>,
    /// In-memory cache for live metadata (thread-safe)
    metadata_cache: Arc<std::sync::RwLock<std::collections::HashMap<String, LiveMetadataCache>>>,
    /// Liste des callbacks abonnés aux mises à jour de métadonnées
    update_callbacks: Arc<std::sync::RwLock<Vec<MetadataUpdateCallback>>>,
}

impl RadioFranceStatefulClient {
    /// Create a new stateful client
    ///
    /// # Arguments
    ///
    /// * `config` - Configuration handle for caching station lists
    ///
    /// # Example
    ///
    /// ```no_run
    /// use pmoconfig::get_config;
    /// use pmoradiofrance::RadioFranceStatefulClient;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     let config = get_config();
    ///     let client = RadioFranceStatefulClient::new(config).await?;
    ///     Ok(())
    /// }
    /// ```
    pub async fn new(config: Arc<Config>) -> Result<Self> {
        let client = RadioFranceClient::new().await?;
        Ok(Self {
            client,
            config,
            metadata_cache: Arc::new(std::sync::RwLock::new(std::collections::HashMap::new())),
            update_callbacks: Arc::new(std::sync::RwLock::new(Vec::new())),
        })
    }

    /// Create a client from global configuration
    ///
    /// This is a convenience method that reads the configuration from
    /// the global config singleton.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use pmoradiofrance::RadioFranceStatefulClient;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     let client = RadioFranceStatefulClient::from_config().await?;
    ///     Ok(())
    /// }
    /// ```
    pub async fn from_config() -> Result<Self> {
        let config = pmoconfig::get_config();
        Self::new(config).await
    }

    /// Create a client with a custom RadioFranceClient
    pub fn with_client(client: RadioFranceClient, config: Arc<Config>) -> Self {
        Self {
            client,
            config,
            metadata_cache: Arc::new(std::sync::RwLock::new(std::collections::HashMap::new())),
            update_callbacks: Arc::new(std::sync::RwLock::new(Vec::new())),
        }
    }

    /// Get the underlying HTTP client
    pub fn client(&self) -> &RadioFranceClient {
        &self.client
    }

    /// Get the configuration
    pub fn config(&self) -> &Arc<Config> {
        &self.config
    }

    // ========================================================================
    // Event System (metadata update notifications)
    // ========================================================================

    /// S'abonner aux mises à jour de métadonnées
    ///
    /// Le callback sera appelé avec le slug de la station chaque fois que
    /// ses métadonnées sont rafraîchies depuis l'API.
    ///
    /// # Arguments
    ///
    /// * `callback` - Fonction appelée avec le slug de la station mise à jour
    pub fn subscribe_to_updates(&self, callback: MetadataUpdateCallback) {
        let mut callbacks = self.update_callbacks.write().unwrap();
        callbacks.push(callback);
    }

    /// Notifier tous les abonnés d'une mise à jour de métadonnées
    ///
    /// Cette méthode est appelée en interne lorsque les métadonnées
    /// d'une station sont rafraîchies depuis l'API.
    fn notify_update(&self, slug: &str) {
        let callbacks = self.update_callbacks.read().unwrap();
        for callback in callbacks.iter() {
            callback(slug);
        }
    }

    // ========================================================================
    // Station Discovery (with automatic caching)
    // ========================================================================

    /// Get all stations, using cache if valid
    ///
    /// This method automatically:
    /// 1. Checks if Radio France is enabled in config
    /// 2. Tries to use cached station list
    /// 3. If cache miss/expired, discovers and caches stations
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Radio France is disabled in config
    /// - Discovery fails and no valid cache exists
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use pmoradiofrance::RadioFranceStatefulClient;
    /// # use pmoconfig::get_config;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let config = get_config();
    /// # let client = RadioFranceStatefulClient::new(config).await?;
    /// let stations = client.get_stations().await?;
    /// for station in stations {
    ///     println!("{} - {}", station.name, station.slug);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn get_stations(&self) -> Result<Vec<Station>> {
        // Check if Radio France is enabled
        if !self.config.get_radiofrance_enabled()? {
            return Err(Error::other("Radio France is disabled in configuration"));
        }

        // Try to get from cache
        if let Some(stations) = self.config.get_radiofrance_stations_cached()? {
            #[cfg(feature = "logging")]
            tracing::debug!("Using {} cached stations", stations.len());
            return Ok(stations);
        }

        // Cache miss - discover and cache with timeout
        #[cfg(feature = "logging")]
        tracing::info!("Station cache miss - discovering stations");

        let stations = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            self.client.discover_all_stations(),
        )
        .await
        .map_err(|_| Error::other("Timeout while discovering Radio France stations (10s)"))??;

        // Cache the results
        self.config.set_radiofrance_cached_stations(&stations)?;

        #[cfg(feature = "logging")]
        tracing::info!("Discovered and cached {} stations", stations.len());

        Ok(stations)
    }

    /// Force refresh of the station list (bypass cache)
    ///
    /// Use this to force re-discovery, for example after a manual
    /// cache invalidation or to get the latest station list.
    pub async fn refresh_stations(&self) -> Result<Vec<Station>> {
        #[cfg(feature = "logging")]
        tracing::info!("Force refreshing station list");

        let stations = self.client.discover_all_stations().await?;
        self.config.set_radiofrance_cached_stations(&stations)?;

        #[cfg(feature = "logging")]
        tracing::info!("Refreshed {} stations", stations.len());

        Ok(stations)
    }

    /// Clear the station cache
    ///
    /// Forces next `get_stations()` call to re-discover stations.
    pub fn clear_station_cache(&self) -> Result<()> {
        Ok(self.config.clear_radiofrance_station_cache()?)
    }

    // ========================================================================
    // Live Metadata (with intelligent caching)
    // ========================================================================

    /// Get live metadata for a station, using cache if valid
    ///
    /// This method automatically:
    /// 1. Checks in-memory cache
    /// 2. If cache valid (based on delayToRefresh), returns cached data
    /// 3. If cache expired, fetches fresh data and updates cache
    ///
    /// # Arguments
    ///
    /// * `station` - Station slug (e.g., "franceculture", "fip_rock")
    ///
    /// # Caching Behavior
    ///
    /// The cache TTL is determined by the API's `delayToRefresh` field,
    /// which respects Radio France's recommended polling interval.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use pmoradiofrance::RadioFranceStatefulClient;
    /// # use pmoconfig::get_config;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let config = get_config();
    /// # let client = RadioFranceStatefulClient::new(config).await?;
    /// let metadata = client.get_live_metadata("franceculture").await?;
    /// println!("Now: {} - {}",
    ///     metadata.now.first_line.title_or_default(),
    ///     metadata.now.second_line.title_or_default()
    /// );
    /// # Ok(())
    /// # }
    /// ```
    pub async fn get_live_metadata(&self, station: &str) -> Result<LiveResponse> {
        // Check cache first
        {
            let cache = self.metadata_cache.read().unwrap();
            if let Some(entry) = cache.get(station) {
                if entry.is_valid() {
                    #[cfg(feature = "logging")]
                    tracing::debug!(
                        "Using cached metadata for {} (TTL: {:?})",
                        station,
                        entry.remaining_ttl()
                    );
                    return Ok(entry.metadata.clone());
                }
            }
        }

        // Cache miss or expired - fetch fresh data with timeout
        #[cfg(feature = "logging")]
        tracing::debug!("Fetching live metadata for {}", station);

        let metadata = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            self.client.live_metadata(station),
        )
        .await
        .map_err(|_| {
            Error::other(format!(
                "Timeout while fetching metadata for {} (5s)",
                station
            ))
        })??;

        // Update cache
        {
            let mut cache = self.metadata_cache.write().unwrap();
            cache.insert(
                station.to_string(),
                LiveMetadataCache::new(metadata.clone()),
            );
        }

        // Notify subscribers
        self.notify_update(station);

        #[cfg(feature = "logging")]
        tracing::debug!(
            "Cached metadata for {} (TTL: {} ms)",
            station,
            metadata.delay_to_refresh
        );

        Ok(metadata)
    }

    /// Clear the metadata cache for a specific station
    pub fn clear_metadata_cache(&self, station: &str) {
        let mut cache = self.metadata_cache.write().unwrap();
        cache.remove(station);

        #[cfg(feature = "logging")]
        tracing::debug!("Cleared metadata cache for {}", station);
    }

    /// Clear all metadata caches
    pub fn clear_all_metadata_caches(&self) {
        let mut cache = self.metadata_cache.write().unwrap();
        cache.clear();

        #[cfg(feature = "logging")]
        tracing::debug!("Cleared all metadata caches");
    }

    // ========================================================================
    // Convenience Methods
    // ========================================================================

    /// Get the HiFi stream URL for a station
    ///
    /// Convenience wrapper around `get_live_metadata()` that extracts
    /// the best HiFi stream URL.
    pub async fn get_stream_url(&self, station: &str) -> Result<String> {
        self.client.get_hifi_stream_url(station).await
    }

    /// Check if Radio France is enabled in configuration
    pub fn is_enabled(&self) -> Result<bool> {
        Ok(self.config.get_radiofrance_enabled()?)
    }

    /// Enable Radio France in configuration
    pub fn set_enabled(&self, enabled: bool) -> Result<()> {
        Ok(self.config.set_radiofrance_enabled(enabled)?)
    }

    /// Get the station cache TTL in seconds
    pub fn get_station_cache_ttl(&self) -> Result<u64> {
        Ok(self.config.get_radiofrance_station_cache_ttl()?)
    }

    /// Set the station cache TTL in seconds
    pub fn set_station_cache_ttl(&self, ttl_secs: u64) -> Result<()> {
        Ok(self.config.set_radiofrance_station_cache_ttl(ttl_secs)?)
    }

    /// Get cache statistics
    ///
    /// Returns (number of cached stations, number of cached metadata entries)
    pub fn cache_stats(&self) -> (usize, usize) {
        let station_count = self
            .config
            .get_radiofrance_stations_cached()
            .ok()
            .flatten()
            .map(|s| s.len())
            .unwrap_or(0);

        let metadata_count = self.metadata_cache.read().unwrap().len();

        (station_count, metadata_count)
    }
}

impl std::fmt::Debug for RadioFranceStatefulClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (station_cache, metadata_cache) = self.cache_stats();
        f.debug_struct("RadioFranceStatefulClient")
            .field("client", &self.client)
            .field("cached_stations", &station_cache)
            .field("cached_metadata_entries", &metadata_cache)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: Real integration tests would require pmoconfig setup
    // These are just structural tests

    #[test]
    fn test_live_metadata_cache_validity() {
        let response = LiveResponse {
            station_name: "test".to_string(),
            delay_to_refresh: 5000, // 5 seconds
            migrated: true,
            now: crate::models::ShowMetadata {
                print_prog_music: false,
                start_time: None,
                end_time: None,
                producer: None,
                first_line: Default::default(),
                second_line: Default::default(),
                third_line: None,
                intro: None,
                react_available: false,
                visual_background: None,
                song: None,
                media: Default::default(),
                visuals: None,
                local_radios: None,
            },
            next: None,
        };

        let cache = LiveMetadataCache::new(response);
        assert!(cache.is_valid());

        // Verify the cache expires in the future
        assert!(cache.valid_until > SystemTime::now());
    }
}
