//! Module de gestion du cache d'images avec conversion WebP
//!
//! Ce module étend le cache générique de `pmocache` avec des fonctionnalités
//! spécifiques aux images : conversion WebP et génération de variantes.

use std::path::PathBuf;
use std::sync::Arc;
use anyhow::{anyhow, Result};
use pmocache::{Cache as GenericCache, CacheConfig};
use crate::webp;
use crate::db::DB;

/// Cache d'images avec conversion WebP et génération de variantes
///
/// Gère le téléchargement, la conversion en WebP, le stockage et la génération
/// de variantes de tailles pour les images de couvertures.
#[derive(Debug)]
pub struct Cache {
    cache: GenericCache,
    pub(crate) dir: PathBuf,
    pub(crate) limit: usize,
    pub db: Arc<DB>,
}

impl Cache {
    /// Crée un nouveau cache d'images
    ///
    /// # Arguments
    ///
    /// * `dir` - Répertoire de stockage du cache
    /// * `limit` - Limite de taille du cache (nombre d'images)
    ///
    /// # Exemple
    ///
    /// ```rust,no_run
    /// use pmocovers::Cache;
    ///
    /// let cache = Cache::new("./cache", 1000).unwrap();
    /// ```
    pub fn new(dir: &str, limit: usize) -> Result<Self> {
        let config = CacheConfig::new(dir, limit, "covers", "orig.webp");
        let cache = GenericCache::new(config)?;

        Ok(Self {
            dir: PathBuf::from(dir),
            limit,
            db: Arc::clone(&cache.db),
            cache,
        })
    }

    /// Télécharge une image depuis une URL et l'ajoute au cache
    ///
    /// # Arguments
    ///
    /// * `url` - URL de l'image à télécharger
    ///
    /// # Returns
    ///
    /// La clé primaire (pk) de l'image dans le cache
    pub async fn add_from_url(&self, url: &str) -> Result<String> {
        let response = reqwest::get(url).await?;
        if !response.status().is_success() {
            return Err(anyhow!("Bad status: {}", response.status()));
        }

        let data = response.bytes().await?;
        self.add(url, &data).await
    }

    /// S'assure qu'une image est présente dans le cache
    ///
    /// Si l'image existe déjà, retourne sa clé. Sinon, la télécharge.
    ///
    /// # Arguments
    ///
    /// * `url` - URL de l'image
    pub async fn ensure_from_url(&self, url: &str) -> Result<String> {
        self.cache.ensure_from_url(url, None).await
    }

    /// Ajoute une image au cache avec conversion en WebP
    ///
    /// # Arguments
    ///
    /// * `url` - URL source de l'image
    /// * `data` - Données brutes de l'image
    pub async fn add(&self, url: &str, data: &[u8]) -> Result<String> {
        let pk = pmocache::pk_from_url(url);
        let orig_path = self.dir.join(format!("{}.orig.webp", pk));

        // Vérifier si le fichier existe déjà
        if !orig_path.exists() {
            // Convertir l'image en WebP
            let img = image::load_from_memory(data)?;
            let webp_data = webp::encode_webp(&img)?;
            tokio::fs::write(&orig_path, webp_data).await?;
        }

        // Ajouter à la DB (sans collection pour les covers)
        self.db.add(&pk, url, None)?;
        Ok(pk)
    }

    /// Récupère le chemin d'une image dans le cache
    ///
    /// # Arguments
    ///
    /// * `pk` - Clé primaire de l'image
    pub async fn get(&self, pk: &str) -> Result<PathBuf> {
        self.cache.get(pk).await
    }

    /// Supprime tous les fichiers et entrées du cache
    pub async fn purge(&self) -> Result<()> {
        self.cache.purge().await
    }

    /// Consolide le cache en supprimant les orphelins et en re-téléchargeant les images manquantes
    pub async fn consolidate(&self) -> Result<()> {
        // Récupérer la liste des entrées à traiter
        let entries = self.db.get_all()?;

        // Supprimer les entrées sans fichiers correspondants
        for entry in entries {
            let orig_path = self.dir.join(format!("{}.orig.webp", entry.pk));
            if !orig_path.exists() {
                match reqwest::get(&entry.source_url).await {
                    Ok(response) if response.status().is_success() => {
                        let data = response.bytes().await?;
                        self.add(&entry.source_url, &data).await?;
                    }
                    _ => {
                        self.db.delete(&entry.pk)?;
                    }
                }
            }
        }

        // Supprimer les fichiers sans entrées DB correspondantes
        let mut dir_entries = tokio::fs::read_dir(&self.dir).await?;
        while let Some(entry) = dir_entries.next_entry().await? {
            let path = entry.path();
            if path.is_file() && path != self.dir.join("cache.db") {
                if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
                    if file_name.ends_with(".orig.webp") {
                        let pk = file_name.trim_end_matches(".orig.webp");
                        if self.db.get(pk).is_err() {
                            tokio::fs::remove_file(path).await?;
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Retourne le répertoire du cache
    pub fn cache_dir(&self) -> String {
        self.dir.to_string_lossy().to_string()
    }
}
