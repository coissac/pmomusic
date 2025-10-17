//! Module de gestion du cache d'images avec conversion WebP
//!
//! Ce module étend le cache générique de `pmocache` avec des fonctionnalités
//! spécifiques aux images : conversion WebP et génération de variantes.

use anyhow::Result;
use pmocache::{CacheConfig, FileCache};
use crate::webp;
use std::path::PathBuf;
use std::ops::Deref;

/// Configuration pour le cache de couvertures
pub struct CoversConfig;

impl CacheConfig for CoversConfig {
    fn file_extension() -> &'static str {
        "webp"
    }

    fn table_name() -> &'static str {
        "covers"
    }

    fn cache_type() -> &'static str {
        "image"
    }

    /// Cache name (ex: "covers", "audio", "cache")
    fn cache_name() -> &'static str {
        "covers"
    }
}

/// Cache d'images avec conversion WebP et génération de variantes
///
/// Format des fichiers : `{pk}.{qualificatif}.webp`
/// Exemple : `a1b2c3d4.orig.webp`, `a1b2c3d4.thumb.webp`
///
/// Ce type est un wrapper autour de `pmocache::Cache<CoversConfig>` qui permet
/// d'implémenter le trait `FileCache` avec conversion WebP automatique.
#[derive(Debug)]
pub struct Cache(pmocache::Cache<CoversConfig>);

impl Cache {
    /// Crée un nouveau cache d'images
    pub fn new(dir: &str, limit: usize, base_url: &str) -> Result<Self> {
        Ok(Self(pmocache::Cache::new(dir, limit, base_url)?))
    }
}

/// Permet d'accéder aux méthodes publiques de `pmocache::Cache` directement
impl Deref for Cache {
    type Target = pmocache::Cache<CoversConfig>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// Implémentation de FileCache pour Cache avec conversion WebP automatique
impl FileCache for Cache {
    fn cache_type(&self) -> &str {
        CoversConfig::cache_type()
    }

    fn validate_data(&self, data: &[u8]) -> Result<Vec<u8>> {
        // Convertir l'image en WebP
        let img = image::load_from_memory(data)?;
        webp::encode_webp(&img)
    }

    async fn add_from_url(&self, url: &str, collection: Option<&str>) -> Result<String> {
        let response = reqwest::get(url).await?;
        if !response.status().is_success() {
            return Err(anyhow::anyhow!("Bad status: {}", response.status()));
        }

        let data = response.bytes().await?;
        self.add(url, &data, collection).await
    }

    async fn ensure_from_url(&self, url: &str, collection: Option<&str>) -> Result<String> {
        let pk = pmocache::pk_from_url(url);

        if self.db.get(&pk).is_ok() {
            let file_path = self.file_path(&pk);
            if file_path.exists() {
                return Ok(pk);
            }
        }

        self.add_from_url(url, collection).await
    }

    async fn add(&self, url: &str, data: &[u8], collection: Option<&str>) -> Result<String> {
        // Valider et convertir les données en WebP
        let webp_data = self.validate_data(data)?;

        let pk = pmocache::pk_from_url(url);
        let file_path = self.file_path(&pk);

        if !file_path.exists() {
            tokio::fs::write(&file_path, &webp_data).await?;
        }

        self.db.add(&pk, url, collection)?;
        Ok(pk)
    }

    async fn get(&self, pk: &str) -> Result<PathBuf> {
        self.db.get(pk)?;
        self.db.update_hit(pk)?;

        let file_path = self.file_path(pk);
        if file_path.exists() {
            Ok(file_path)
        } else {
            Err(anyhow::anyhow!("File not found"))
        }
    }

    async fn get_collection(&self, collection: &str) -> Result<Vec<PathBuf>> {
        let entries = self.db.get_by_collection(collection)?;
        let mut paths = Vec::new();

        for entry in entries {
            let path = self.file_path(&entry.pk);
            if path.exists() {
                paths.push(path);
            }
        }

        Ok(paths)
    }

    async fn purge(&self) -> Result<()> {
        let cache_dir = PathBuf::from(self.get_cache_dir());
        let mut entries = tokio::fs::read_dir(&cache_dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            if entry.path().is_file() && entry.path() != cache_dir.join("cache.db") {
                tokio::fs::remove_file(entry.path()).await?;
            }
        }

        self.db
            .purge()
            .map_err(|e| anyhow::anyhow!("Database error: {}", e))
    }

    async fn consolidate(&self) -> Result<()> {
        // Récupérer la liste des entrées à traiter
        let entries = self.db.get_all()?;

        // Supprimer les entrées sans fichiers correspondants
        for entry in entries {
            let file_path = self.file_path(&entry.pk);
            if !file_path.exists() {
                match reqwest::get(&entry.source_url).await {
                    Ok(response) if response.status().is_success() => {
                        let data = response.bytes().await?;
                        self.add(&entry.source_url, &data, entry.collection.as_deref())
                            .await?;
                    }
                    _ => {
                        self.db.delete(&entry.pk)?;
                    }
                }
            }
        }

        // Supprimer les fichiers sans entrées DB correspondantes
        let cache_dir_path = PathBuf::from(self.get_cache_dir());
        let mut dir_entries = tokio::fs::read_dir(&cache_dir_path).await?;
        while let Some(entry) = dir_entries.next_entry().await? {
            let path = entry.path();
            if path.is_file() && path != cache_dir_path.join("cache.db") {
                if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
                    // Format attendu: {pk}.{qualifier}.{EXT}
                    if let Some(pk) = file_name.split('.').next() {
                        if self.db.get(pk).is_err() {
                            tokio::fs::remove_file(path).await?;
                        }
                    }
                }
            }
        }

        Ok(())
    }

    fn get_cache_dir(&self) -> String {
        self.get_cache_dir()
    }

    fn get_base_url(&self) -> &str {
        self.get_base_url()
    }
}
