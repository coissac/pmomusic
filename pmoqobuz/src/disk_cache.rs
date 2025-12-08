//! Cache disque simple pour les données volumineuses de l'API Qobuz
//!
//! Ce module gère le cache sur disque des données qui changent rarement :
//! - Favoris (albums, tracks, artistes)
//! - Playlists utilisateur
//! - Bibliothèque
//!
//! Contrairement à pmocache (conçu pour des fichiers binaires avec téléchargement),
//! ce cache est optimisé pour du JSON provenant de l'API.

use anyhow::{anyhow, Result};
use serde::{de::DeserializeOwned, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use tracing::{debug, info};

/// Cache disque pour données JSON de l'API Qobuz
pub struct DiskCache {
    /// Répertoire de cache
    cache_dir: PathBuf,
}

impl DiskCache {
    /// Crée un nouveau cache disque
    ///
    /// # Arguments
    ///
    /// * `cache_dir` - Répertoire où stocker les fichiers cachés
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use pmoqobuz::disk_cache::DiskCache;
    ///
    /// let cache = DiskCache::new(".pmomusic/cache/qobuz")?;
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn new<P: AsRef<Path>>(cache_dir: P) -> Result<Self> {
        let cache_dir = cache_dir.as_ref().to_path_buf();

        // Créer le répertoire s'il n'existe pas
        if !cache_dir.exists() {
            fs::create_dir_all(&cache_dir)?;
            info!("Created cache directory: {}", cache_dir.display());
        }

        Ok(Self { cache_dir })
    }

    /// Construit le chemin d'un fichier de cache
    ///
    /// Format: `{cache_dir}/{key}.json`
    fn cache_path(&self, key: &str) -> PathBuf {
        self.cache_dir.join(format!("{}.json", key))
    }

    /// Sauvegarde des données dans le cache
    ///
    /// # Arguments
    ///
    /// * `key` - Identifiant unique du cache (ex: "favorites_albums_123456")
    /// * `data` - Données à sauvegarder
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use pmoqobuz::disk_cache::DiskCache;
    /// # use pmoqobuz::Album;
    /// # let cache = DiskCache::new(".cache")?;
    /// let albums: Vec<Album> = vec![/* ... */];
    /// cache.save("favorites_albums_123", &albums)?;
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn save<T: Serialize>(&self, key: &str, data: &T) -> Result<()> {
        let path = self.cache_path(key);
        let json = serde_json::to_string_pretty(data)?;

        fs::write(&path, json)?;
        debug!("Saved cache to {}", path.display());

        Ok(())
    }

    /// Charge des données depuis le cache
    ///
    /// # Arguments
    ///
    /// * `key` - Identifiant unique du cache
    ///
    /// # Returns
    ///
    /// Les données désérialisées, ou None si le cache n'existe pas
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use pmoqobuz::disk_cache::DiskCache;
    /// # use pmoqobuz::Album;
    /// # let cache = DiskCache::new(".cache")?;
    /// if let Some(albums) = cache.load::<Vec<Album>>("favorites_albums_123")? {
    ///     println!("Loaded {} albums from cache", albums.len());
    /// }
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn load<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>> {
        let path = self.cache_path(key);

        if !path.exists() {
            debug!("Cache file does not exist: {}", path.display());
            return Ok(None);
        }

        let json = fs::read_to_string(&path)?;
        let data: T = serde_json::from_str(&json)?;

        debug!("Loaded cache from {}", path.display());
        Ok(Some(data))
    }

    /// Charge des données avec vérification du TTL
    ///
    /// # Arguments
    ///
    /// * `key` - Identifiant unique du cache
    /// * `ttl` - Durée de validité maximale
    ///
    /// # Returns
    ///
    /// Les données si le cache existe ET n'est pas expiré, None sinon
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use pmoqobuz::disk_cache::DiskCache;
    /// # use pmoqobuz::Album;
    /// # use std::time::Duration;
    /// # let cache = DiskCache::new(".cache")?;
    /// // Cache valide pendant 1 heure
    /// if let Some(albums) = cache.load_with_ttl::<Vec<Album>>(
    ///     "favorites_albums_123",
    ///     Duration::from_secs(3600)
    /// )? {
    ///     println!("Cache still valid!");
    /// } else {
    ///     println!("Cache expired or missing");
    /// }
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn load_with_ttl<T: DeserializeOwned>(
        &self,
        key: &str,
        ttl: Duration,
    ) -> Result<Option<T>> {
        let path = self.cache_path(key);

        if !path.exists() {
            debug!("Cache file does not exist: {}", path.display());
            return Ok(None);
        }

        // Vérifier l'âge du fichier
        let metadata = fs::metadata(&path)?;
        let modified = metadata.modified()?;
        let age = SystemTime::now()
            .duration_since(modified)
            .unwrap_or(Duration::MAX);

        if age > ttl {
            debug!(
                "Cache expired (age: {}s > ttl: {}s): {}",
                age.as_secs(),
                ttl.as_secs(),
                path.display()
            );
            // Optionnel : supprimer le fichier expiré
            let _ = fs::remove_file(&path);
            return Ok(None);
        }

        debug!(
            "Cache valid (age: {}s < ttl: {}s): {}",
            age.as_secs(),
            ttl.as_secs(),
            path.display()
        );

        let json = fs::read_to_string(&path)?;
        let data: T = serde_json::from_str(&json)?;

        Ok(Some(data))
    }

    /// Invalide (supprime) un cache
    ///
    /// # Arguments
    ///
    /// * `key` - Identifiant unique du cache
    pub fn invalidate(&self, key: &str) -> Result<()> {
        let path = self.cache_path(key);

        if path.exists() {
            fs::remove_file(&path)?;
            debug!("Invalidated cache: {}", path.display());
        }

        Ok(())
    }

    /// Supprime tous les fichiers de cache
    pub fn clear_all(&self) -> Result<()> {
        for entry in fs::read_dir(&self.cache_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.extension().and_then(|s| s.to_str()) == Some("json") {
                fs::remove_file(&path)?;
                debug!("Removed cache file: {}", path.display());
            }
        }

        info!("Cleared all cache files");
        Ok(())
    }

    /// Retourne la taille totale du cache en octets
    pub fn size(&self) -> Result<u64> {
        let mut total = 0u64;

        for entry in fs::read_dir(&self.cache_dir)? {
            let entry = entry?;
            let metadata = entry.metadata()?;

            if metadata.is_file() {
                total += metadata.len();
            }
        }

        Ok(total)
    }

    /// Retourne le nombre de fichiers en cache
    pub fn count(&self) -> Result<usize> {
        let mut count = 0;

        for entry in fs::read_dir(&self.cache_dir)? {
            let entry = entry?;

            if entry.path().extension().and_then(|s| s.to_str()) == Some("json") {
                count += 1;
            }
        }

        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};
    use tempfile::tempdir;

    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    struct TestData {
        id: String,
        value: i32,
    }

    #[test]
    fn test_save_and_load() -> Result<()> {
        let dir = tempdir()?;
        let cache = DiskCache::new(dir.path())?;

        let data = TestData {
            id: "test123".to_string(),
            value: 42,
        };

        // Sauvegarder
        cache.save("test_key", &data)?;

        // Charger
        let loaded: Option<TestData> = cache.load("test_key")?;
        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap(), data);

        Ok(())
    }

    #[test]
    fn test_load_nonexistent() -> Result<()> {
        let dir = tempdir()?;
        let cache = DiskCache::new(dir.path())?;

        let loaded: Option<TestData> = cache.load("nonexistent")?;
        assert!(loaded.is_none());

        Ok(())
    }

    #[test]
    fn test_ttl() -> Result<()> {
        let dir = tempdir()?;
        let cache = DiskCache::new(dir.path())?;

        let data = TestData {
            id: "test123".to_string(),
            value: 42,
        };

        cache.save("test_key", &data)?;

        // Charger immédiatement (< TTL)
        let loaded: Option<TestData> =
            cache.load_with_ttl("test_key", Duration::from_secs(60))?;
        assert!(loaded.is_some());

        // Charger avec TTL expiré
        let loaded: Option<TestData> = cache.load_with_ttl("test_key", Duration::from_secs(0))?;
        assert!(loaded.is_none());

        Ok(())
    }

    #[test]
    fn test_invalidate() -> Result<()> {
        let dir = tempdir()?;
        let cache = DiskCache::new(dir.path())?;

        let data = TestData {
            id: "test123".to_string(),
            value: 42,
        };

        cache.save("test_key", &data)?;
        assert!(cache.load::<TestData>("test_key")?.is_some());

        cache.invalidate("test_key")?;
        assert!(cache.load::<TestData>("test_key")?.is_none());

        Ok(())
    }

    #[test]
    fn test_size_and_count() -> Result<()> {
        let dir = tempdir()?;
        let cache = DiskCache::new(dir.path())?;

        assert_eq!(cache.count()?, 0);
        assert_eq!(cache.size()?, 0);

        let data = TestData {
            id: "test123".to_string(),
            value: 42,
        };

        cache.save("test1", &data)?;
        cache.save("test2", &data)?;

        assert_eq!(cache.count()?, 2);
        assert!(cache.size()? > 0);

        Ok(())
    }
}
