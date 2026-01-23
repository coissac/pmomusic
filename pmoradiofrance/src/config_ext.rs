//! Extension pour intégrer Radio France dans pmoconfig
//!
//! Ce module fournit le trait `RadioFranceConfigExt` qui permet d'ajouter
//! des méthodes de gestion de la configuration Radio France à pmoconfig::Config.
//!
//! # Fonctionnalités
//!
//! - Activation/désactivation de la source
//! - Cache de la liste des stations (TTL configurable, défaut 7 jours)
//! - Configuration minimale (pas de sur-configuration)
//!
//! # Exemple
//!
//! ```no_run
//! use pmoconfig::get_config;
//! use pmoradiofrance::RadioFranceConfigExt;
//!
//! # fn main() -> anyhow::Result<()> {
//! let config = get_config();
//!
//! // Check if enabled
//! if !config.get_radiofrance_enabled()? {
//!     println!("Radio France is disabled");
//!     return Ok(());
//! }
//!
//! // Get cached stations (or None if cache expired/empty)
//! if let Some(cached) = config.get_radiofrance_cached_stations()? {
//!     println!("Found {} cached stations", cached.stations.len());
//! }
//! # Ok(())
//! # }
//! ```

use crate::models::{CachedStationList, Station};
use anyhow::Result;
use pmoconfig::Config;
use serde_yaml::Value;

/// Default TTL for station list cache (7 days in seconds)
pub const DEFAULT_STATION_CACHE_TTL_SECS: u64 = 7 * 24 * 3600;

/// Trait d'extension pour gérer la configuration Radio France dans pmoconfig
///
/// Ce trait étend `pmoconfig::Config` avec des méthodes spécifiques
/// à la gestion de Radio France, incluant :
///
/// - Activation/désactivation
/// - Cache de la liste des stations
///
/// # Auto-persist des valeurs par défaut
///
/// Les getters persistent automatiquement les valeurs par défaut dans la
/// configuration si elles n'existent pas encore.
pub trait RadioFranceConfigExt {
    // ========================================================================
    // Enable/Disable
    // ========================================================================

    /// Vérifie si Radio France est activé
    ///
    /// # Returns
    ///
    /// `true` si la source est activée (default), `false` sinon.
    fn get_radiofrance_enabled(&self) -> Result<bool>;

    /// Active ou désactive Radio France
    fn set_radiofrance_enabled(&self, enabled: bool) -> Result<()>;

    // ========================================================================
    // Station Cache
    // ========================================================================

    /// Récupère la liste des stations en cache
    ///
    /// # Returns
    ///
    /// - `Some(CachedStationList)` si le cache existe et est valide
    /// - `None` si le cache n'existe pas ou est expiré
    ///
    /// # Cache Validation
    ///
    /// Le cache est considéré invalide si :
    /// - Il n'existe pas
    /// - Son TTL est dépassé (configurable, défaut 7 jours)
    /// - Sa version ne correspond pas à la version actuelle de l'algorithme
    fn get_radiofrance_cached_stations(&self) -> Result<Option<CachedStationList>>;

    /// Enregistre la liste des stations en cache
    ///
    /// # Arguments
    ///
    /// * `stations` - Liste des stations découvertes
    fn set_radiofrance_cached_stations(&self, stations: &[Station]) -> Result<()>;

    /// Récupère le TTL du cache des stations (en secondes)
    ///
    /// # Returns
    ///
    /// Le TTL en secondes (default: 7 jours)
    fn get_radiofrance_station_cache_ttl(&self) -> Result<u64>;

    /// Définit le TTL du cache des stations (en secondes)
    fn set_radiofrance_station_cache_ttl(&self, ttl_secs: u64) -> Result<()>;

    /// Vérifie si le cache des stations est valide
    ///
    /// Raccourci pour `get_radiofrance_cached_stations()?.is_some()`
    fn is_radiofrance_station_cache_valid(&self) -> bool;

    /// Efface le cache des stations (force re-découverte)
    fn clear_radiofrance_station_cache(&self) -> Result<()>;

    // ========================================================================
    // High-level helpers
    // ========================================================================

    /// Récupère les stations, en utilisant le cache si valide
    ///
    /// Cette méthode est un helper qui :
    /// 1. Vérifie le cache
    /// 2. Si valide, retourne les stations du cache
    /// 3. Si invalide, retourne None (l'appelant doit découvrir et mettre en cache)
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use pmoconfig::get_config;
    /// # use pmoradiofrance::{RadioFranceConfigExt, RadioFranceClient};
    /// # #[tokio::main]
    /// # async fn main() -> anyhow::Result<()> {
    /// let config = get_config();
    /// let stations = if let Some(cached) = config.get_radiofrance_stations_cached()? {
    ///     cached
    /// } else {
    ///     let client = RadioFranceClient::new().await?;
    ///     let discovered = client.discover_all_stations().await?;
    ///     config.set_radiofrance_cached_stations(&discovered)?;
    ///     discovered
    /// };
    /// # Ok(())
    /// # }
    /// ```
    fn get_radiofrance_stations_cached(&self) -> Result<Option<Vec<Station>>>;
}

impl RadioFranceConfigExt for Config {
    fn get_radiofrance_enabled(&self) -> Result<bool> {
        match self.get_value(&["sources", "radiofrance", "enabled"]) {
            Ok(Value::Bool(b)) => Ok(b),
            _ => {
                // Default: enabled
                self.set_radiofrance_enabled(true)?;
                Ok(true)
            }
        }
    }

    fn set_radiofrance_enabled(&self, enabled: bool) -> Result<()> {
        self.set_value(&["sources", "radiofrance", "enabled"], Value::Bool(enabled))
    }

    fn get_radiofrance_cached_stations(&self) -> Result<Option<CachedStationList>> {
        let ttl = self.get_radiofrance_station_cache_ttl()?;

        match self.get_value(&["sources", "radiofrance", "station_cache"]) {
            Ok(value) => {
                // Try to deserialize the cached data
                let cached: CachedStationList = serde_yaml::from_value(value)?;

                // Check validity
                if cached.is_valid(ttl) {
                    Ok(Some(cached))
                } else {
                    // Cache expired or version mismatch
                    Ok(None)
                }
            }
            Err(_) => Ok(None),
        }
    }

    fn set_radiofrance_cached_stations(&self, stations: &[Station]) -> Result<()> {
        let cached = CachedStationList::new(stations.to_vec());
        let value = serde_yaml::to_value(&cached)?;
        self.set_value(&["sources", "radiofrance", "station_cache"], value)
    }

    fn get_radiofrance_station_cache_ttl(&self) -> Result<u64> {
        match self.get_value(&["sources", "radiofrance", "station_cache_ttl_secs"]) {
            Ok(Value::Number(n)) => {
                if let Some(ttl) = n.as_u64() {
                    Ok(ttl)
                } else {
                    // Invalid number, use default
                    self.set_radiofrance_station_cache_ttl(DEFAULT_STATION_CACHE_TTL_SECS)?;
                    Ok(DEFAULT_STATION_CACHE_TTL_SECS)
                }
            }
            _ => {
                // Not set, use default and persist
                self.set_radiofrance_station_cache_ttl(DEFAULT_STATION_CACHE_TTL_SECS)?;
                Ok(DEFAULT_STATION_CACHE_TTL_SECS)
            }
        }
    }

    fn set_radiofrance_station_cache_ttl(&self, ttl_secs: u64) -> Result<()> {
        self.set_value(
            &["sources", "radiofrance", "station_cache_ttl_secs"],
            Value::Number(serde_yaml::Number::from(ttl_secs)),
        )
    }

    fn is_radiofrance_station_cache_valid(&self) -> bool {
        self.get_radiofrance_cached_stations()
            .ok()
            .flatten()
            .is_some()
    }

    fn clear_radiofrance_station_cache(&self) -> Result<()> {
        // Set to null to clear
        self.set_value(&["sources", "radiofrance", "station_cache"], Value::Null)
    }

    fn get_radiofrance_stations_cached(&self) -> Result<Option<Vec<Station>>> {
        Ok(self
            .get_radiofrance_cached_stations()?
            .map(|cached| cached.stations))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_ttl() {
        // 7 days in seconds
        assert_eq!(DEFAULT_STATION_CACHE_TTL_SECS, 7 * 24 * 3600);
    }
}
