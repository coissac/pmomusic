//! Module de gestion du cache générique
//!
//! Ce module fournit une interface générique pour gérer un cache de fichiers
//! avec métadonnées dans une base de données SQLite.

use std::path::PathBuf;
use std::sync::Arc;
use anyhow::{anyhow, Result};
use sha1::{Sha1, Digest};
use tokio::sync::Mutex;
use crate::db::DB;

/// Configuration du cache
#[derive(Debug, Clone)]
pub struct CacheConfig {
    /// Répertoire de stockage
    pub dir: PathBuf,
    /// Limite de taille du cache (nombre d'éléments)
    pub limit: usize,
    /// Nom de la table dans la base de données
    pub table_name: String,
    /// Extension des fichiers dans le cache
    pub file_extension: String,
}

impl CacheConfig {
    /// Crée une nouvelle configuration de cache
    pub fn new(dir: &str, limit: usize, table_name: &str, file_extension: &str) -> Self {
        Self {
            dir: PathBuf::from(dir),
            limit,
            table_name: table_name.to_string(),
            file_extension: file_extension.to_string(),
        }
    }
}

/// Cache générique pour stocker des fichiers avec métadonnées
///
/// Gère le téléchargement, le stockage et la récupération de fichiers
/// avec une base de données SQLite pour les métadonnées.
///
/// Note : Ce type est conçu pour être utilisé derrière un `Arc<Cache>`.
/// Les méthodes prennent `&self` et utilisent des `Arc` et `Mutex` internes
/// pour la synchronisation.
#[derive(Debug)]
pub struct Cache {
    pub(crate) config: CacheConfig,
    pub db: Arc<DB>,
    mu: Arc<Mutex<()>>,
}

impl Cache {
    /// Crée un nouveau cache avec la configuration spécifiée
    ///
    /// # Arguments
    ///
    /// * `config` - Configuration du cache
    ///
    /// # Exemple
    ///
    /// ```rust,no_run
    /// use pmocache::cache::{Cache, CacheConfig};
    ///
    /// let config = CacheConfig::new("./cache", 1000, "my_cache", "webp");
    /// let cache = Cache::new(config).unwrap();
    /// ```
    pub fn new(config: CacheConfig) -> Result<Self> {
        std::fs::create_dir_all(&config.dir)?;
        let db = DB::init(
            &config.dir.join("cache.db"),
            &config.table_name
        )?;

        Ok(Self {
            config,
            db: Arc::new(db),
            mu: Arc::new(Mutex::new(())),
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

    /// Ajoute des données au cache
    ///
    /// Cette méthode doit être surchargée par les implémentations spécifiques
    /// pour gérer la conversion et le stockage des données.
    ///
    /// # Arguments
    ///
    /// * `url` - URL source du fichier
    /// * `data` - Données brutes à stocker
    /// * `collection` - Collection optionnelle à laquelle appartient le fichier
    pub async fn add(&self, url: &str, data: &[u8], collection: Option<&str>) -> Result<String> {
        let pk = pk_from_url(url);
        let file_path = self.file_path(&pk);

        let _lock = self.mu.lock().await;

        if !file_path.exists() {
            // Par défaut, on stocke les données telles quelles
            tokio::fs::write(&file_path, data).await?;
        }

        self.db.add(&pk, url, collection)?;
        Ok(pk)
    }

    /// Récupère le chemin d'un fichier dans le cache
    ///
    /// # Arguments
    ///
    /// * `pk` - Clé primaire du fichier
    pub async fn get(&self, pk: &str) -> Result<PathBuf> {
        let _lock = self.mu.lock().await;

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
        let _lock = self.mu.lock().await;

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
        let _lock = self.mu.lock().await;

        let mut entries = tokio::fs::read_dir(&self.config.dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            if entry.path().is_file() && entry.path() != self.config.dir.join("cache.db") {
                tokio::fs::remove_file(entry.path()).await?;
            }
        }

        self.db.purge().map_err(|e| anyhow!("Database error: {}", e))
    }

    /// Consolide le cache en supprimant les orphelins et en re-téléchargeant les fichiers manquants
    pub async fn consolidate(&self) -> Result<()> {
        // Récupérer la liste des entrées à traiter
        let entries = {
            let _lock = self.mu.lock().await;
            self.db.get_all()?
        };

        // Supprimer les entrées sans fichiers correspondants
        for entry in entries {
            let file_path = self.file_path(&entry.pk);
            if !file_path.exists() {
                match reqwest::get(&entry.source_url).await {
                    Ok(response) if response.status().is_success() => {
                        let data = response.bytes().await?;
                        self.add(&entry.source_url, &data, entry.collection.as_deref()).await?;
                    }
                    _ => {
                        let _lock = self.mu.lock().await;
                        self.db.delete(&entry.pk)?;
                    }
                }
            }
        }

        // Supprimer les fichiers sans entrées DB correspondantes
        let _lock = self.mu.lock().await;
        let mut dir_entries = tokio::fs::read_dir(&self.config.dir).await?;
        while let Some(entry) = dir_entries.next_entry().await? {
            let path = entry.path();
            if path.is_file() && path != self.config.dir.join("cache.db") {
                if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
                    let pk = file_name.trim_end_matches(&format!(".{}", self.config.file_extension));
                    if self.db.get(pk).is_err() {
                        tokio::fs::remove_file(path).await?;
                    }
                }
            }
        }

        Ok(())
    }

    /// Retourne le répertoire du cache
    pub fn cache_dir(&self) -> String {
        self.config.dir.to_string_lossy().to_string()
    }

    /// Construit le chemin complet d'un fichier dans le cache
    fn file_path(&self, pk: &str) -> PathBuf {
        self.config.dir.join(format!("{}.{}", pk, self.config.file_extension))
    }
}

/// Génère une clé primaire à partir d'une URL
///
/// Utilise SHA1 pour hasher l'URL et retourne les 8 premiers octets en hexadécimal.
pub fn pk_from_url(url: &str) -> String {
    let mut hasher = Sha1::new();
    hasher.update(url.as_bytes());
    let result = hasher.finalize();
    hex::encode(&result[..8])
}
