//! Module de gestion du cache générique
//!
//! Ce module fournit une interface générique pour gérer un cache de fichiers
//! avec métadonnées dans une base de données SQLite.

use crate::cache_trait::FileCache;
use crate::db::DB;
use anyhow::{anyhow, Result};
use sha1::{Digest, Sha1};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Trait pour définir les paramètres du cache
pub trait CacheConfig: Send + Sync {
    /// Extension des fichiers (ex: "webp", "flac")
    fn file_extension() -> &'static str;
    /// Nom de la table dans la base de données (ex: "covers", "audio")
    fn table_name() -> &'static str {
        "cached_items"
    }
    /// Type de cache (ex: "audio", "image")
    fn cache_type() -> &'static str {
        "file"
    }
    /// Cache name (ex: "covers", "audio", "cache")
    fn cache_name()  -> &'static str {
        "cache"
    }
    /// Default param extension ("orig") 
    fn default_param() -> &'static str {
        "orig"
    }
}

/// Cache générique pour stocker des fichiers avec métadonnées
///
/// Gère le téléchargement, le stockage et la récupération de fichiers
/// avec une base de données SQLite pour les métadonnées.
///
/// # Paramètres de type
///
/// * `C` - Configuration du cache (implémente `CacheConfig`)
///
/// Note : Ce type est conçu pour être utilisé derrière un `Arc<Cache>`.
/// La synchronisation est gérée par le Mutex interne de la base de données SQLite.
#[derive(Debug)]
pub struct Cache<C: CacheConfig> {
    /// Répertoire de stockage
    dir: PathBuf,
    /// Limite de taille du cache (nombre d'éléments)
    limit: usize,
    /// URL de base pour la génération d'URLs
    base_url: String,
    /// Base de données SQLite
    pub db: Arc<DB>,
    /// Phantom data pour le type de configuration
    _phantom: std::marker::PhantomData<C>,
}

impl<C: CacheConfig> Cache<C> {
    /// Crée un nouveau cache
    ///
    /// # Arguments
    ///
    /// * `dir` - Répertoire de stockage du cache
    /// * `limit` - Limite de taille du cache (nombre d'éléments)
    /// * `base_url` - URL de base pour la génération d'URLs
    pub fn new(dir: &str, limit: usize, base_url: &str) -> Result<Self> {
        let directory = PathBuf::from(dir);
        std::fs::create_dir_all(&directory)?;
        let db = DB::init(&directory.join("cache.db"), C::table_name())?;

        Ok(Self {
            dir: directory,
            limit,
            base_url: base_url.to_string(),
            db: Arc::new(db),
            _phantom: std::marker::PhantomData,
        })
    }

    /// Télécharge un fichier depuis une URL et l'ajoute au cache
    ///
    /// # Arguments
    ///
    /// * `url` - URL du fichier à télécharger
    /// * `collection` - Collection optionnelle à laquelle appartient le fichier
    ///
    /// # Returns
    ///
    /// La clé primaire (pk) du fichier dans le cache
    pub async fn add_from_url(&self, url: &str, collection: Option<&str>) -> Result<String> {
        let response = reqwest::get(url).await?;
        if !response.status().is_success() {
            return Err(anyhow!("Bad status: {}", response.status()));
        }

        let data = response.bytes().await?;
        self.add(url, &data, collection).await
    }

    /// S'assure qu'un fichier est présent dans le cache
    ///
    /// Si le fichier existe déjà, retourne sa clé. Sinon, le télécharge.
    ///
    /// # Arguments
    ///
    /// * `url` - URL du fichier
    /// * `collection` - Collection optionnelle à laquelle appartient le fichier
    pub async fn ensure_from_url(&self, url: &str, collection: Option<&str>) -> Result<String> {
        let pk = pk_from_url(url);

        if self.db.get(&pk).is_ok() {
            let file_path = self.file_path(&pk);
            if file_path.exists() {
                return Ok(pk);
            }
        }

        self.add_from_url(url, collection).await
    }


    /// Récupère le chemin d'un fichier dans le cache
    ///
    /// # Arguments
    ///
    /// * `pk` - Clé primaire du fichier
    pub async fn get(&self, pk: &str) -> Result<PathBuf> {
        self.db.get(pk)?;
        self.db.update_hit(pk)?;

        let file_path = self.file_path(pk);
        if file_path.exists() {
            Ok(file_path)
        } else {
            Err(anyhow!("File not found"))
        }
    }

    /// Récupère tous les fichiers d'une collection
    ///
    /// # Arguments
    ///
    /// * `collection` - Identifiant de la collection
    pub async fn get_collection(&self, collection: &str) -> Result<Vec<PathBuf>> {
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

    /// Supprime tous les fichiers et entrées du cache
    pub async fn purge(&self) -> Result<()> {
        let mut entries = tokio::fs::read_dir(&self.dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            if entry.path().is_file() && entry.path() != self.dir.join("cache.db") {
                tokio::fs::remove_file(entry.path()).await?;
            }
        }

        self.db
            .purge()
            .map_err(|e| anyhow!("Database error: {}", e))
    }

    /// Consolide le cache en supprimant les orphelins et en re-téléchargeant les fichiers manquants
    pub async fn consolidate(&self) -> Result<()> {
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
        let mut dir_entries = tokio::fs::read_dir(&self.dir).await?;
        while let Some(entry) = dir_entries.next_entry().await? {
            let path = entry.path();
            if path.is_file() && path != self.dir.join("cache.db") {
                if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
                    // Format attendu: {pk}.{qualifier}.{EXT}
                    // On extrait le pk (première partie avant le premier point)
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

    /// Retourne le répertoire du cache
    pub fn cache_dir(&self) -> &Path {
        &self.dir
    }

    /// Retourne l'URL de base
    pub fn get_base_url(&self) -> &str {
        &self.base_url
    }

    /// Valide les données avant de les stocker
    /// Par défaut, accepte toutes les données
    pub fn validate_data(&self, data: &[u8]) -> Result<Vec<u8>> {
        Ok(data.to_vec())
    }
}


/// Implémentation du trait FileCache pour Cache
impl<C: CacheConfig> FileCache for Cache<C> {
    fn cache_type(&self) -> &str {
        C::cache_type()
    }

    fn validate_data(&self, data: &[u8]) -> Result<Vec<u8>> {
        // Le cache générique accepte toutes les données
        Ok(data.to_vec())
    }

    async fn add_from_url(&self, url: &str, collection: Option<&str>) -> Result<String> {
        self.add_from_url(url, collection).await
    }

    async fn ensure_from_url(&self, url: &str, collection: Option<&str>) -> Result<String> {
        self.ensure_from_url(url, collection).await
    }

    async fn add(&self, url: &str, data: &[u8], collection: Option<&str>) -> Result<String> {
        // Valider les données avant de les ajouter
        let validated_data = self.validate_data(data)?;

        let pk = pk_from_url(url);
        let file_path = self.file_path(&pk);

        if !file_path.exists() {
            tokio::fs::write(&file_path, &validated_data).await?;
        }

        self.db.add(&pk, url, collection)?;
        Ok(pk)
    }

    async fn get(&self, pk: &str) -> Result<PathBuf> {
        self.get(pk).await
    }

    async fn get_collection(&self, collection: &str) -> Result<Vec<PathBuf>> {
        self.get_collection(collection).await
    }

    async fn purge(&self) -> Result<()> {
        self.purge().await
    }

    async fn consolidate(&self) -> Result<()> {
        self.consolidate().await
    }

    fn get_cache_dir(&self) -> String {
        self.cache_dir()
    }

    fn get_base_url(&self) -> &str {
        self.get_base_url()
    }
}
