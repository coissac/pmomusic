//! Module de gestion du cache générique
//!
//! Ce module fournit une interface générique pour gérer un cache de fichiers
//! avec métadonnées dans une base de données SQLite.

use crate::cache_trait::{pk_from_url, FileCache};
use crate::db::DB;
use crate::download::{download_with_transformer, Download, StreamTransformer};
use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing;

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
    fn cache_name() -> &'static str {
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
/// La synchronisation est gérée par le Mutex interne de la base de données SQLite
/// et par le RwLock pour la map des downloads.
pub struct Cache<C: CacheConfig> {
    /// Répertoire de stockage
    dir: PathBuf,
    /// Limite de taille du cache (nombre d'éléments)
    limit: usize,
    /// Base de données SQLite
    pub db: Arc<DB>,
    /// Map des downloads en cours (pk -> Download)
    downloads: Arc<RwLock<HashMap<String, Arc<Download>>>>,
    /// Factory pour créer des transformers (optionnel)
    transformer_factory: Option<Arc<dyn Fn() -> StreamTransformer + Send + Sync>>,
    /// Phantom data pour le type de configuration
    _phantom: std::marker::PhantomData<C>,
}

impl<C: CacheConfig> Cache<C> {
    /// Crée un nouveau cache sans transformer
    ///
    /// # Arguments
    ///
    /// * `dir` - Répertoire de stockage du cache
    /// * `limit` - Limite de taille du cache (nombre d'éléments)
    pub fn new(dir: &str, limit: usize) -> Result<Self> {
        Self::with_transformer(dir, limit, None)
    }

    /// Crée un nouveau cache avec un transformer optionnel
    ///
    /// # Arguments
    ///
    /// * `dir` - Répertoire de stockage du cache
    /// * `limit` - Limite de taille du cache (nombre d'éléments)
    /// * `transformer_factory` - Factory pour créer des transformers à chaque téléchargement
    ///
    /// # Exemple
    ///
    /// ```rust,no_run
    /// use pmocache::{Cache, CacheConfig, StreamTransformer};
    /// use std::sync::Arc;
    ///
    /// struct MyConfig;
    /// impl CacheConfig for MyConfig {
    ///     fn file_extension() -> &'static str { "dat" }
    /// }
    ///
    /// let transformer_factory = Arc::new(|| {
    ///     // Créer un transformer qui convertit les données
    ///     Box::new(|response, file, progress| {
    ///         Box::pin(async move {
    ///             // Transformation personnalisée
    ///             Ok(())
    ///         })
    ///     }) as StreamTransformer
    /// });
    ///
    /// let cache = Cache::<MyConfig>::with_transformer(
    ///     "./cache",
    ///     1000,
    ///     Some(transformer_factory)
    /// ).unwrap();
    /// ```
    pub fn with_transformer(
        dir: &str,
        limit: usize,
        transformer_factory: Option<Arc<dyn Fn() -> StreamTransformer + Send + Sync>>,
    ) -> Result<Self> {
        let directory = PathBuf::from(dir);
        std::fs::create_dir_all(&directory)?;
        let db = DB::init(&directory.join("cache.db"), C::table_name())?;

        Ok(Self {
            dir: directory,
            limit,
            db: Arc::new(db),
            downloads: Arc::new(RwLock::new(HashMap::new())),
            transformer_factory,
            _phantom: std::marker::PhantomData,
        })
    }

    /// Télécharge un fichier depuis une URL et l'ajoute au cache
    ///
    /// Utilise le module download pour gérer le téléchargement asynchrone.
    /// Le download est tracké dans la map jusqu'à sa fin.
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
        let pk = pk_from_url(url);
        let file_path = self.file_path(&pk);

        // Vérifier si déjà en cours de téléchargement
        {
            let downloads = self.downloads.read().await;
            if downloads.contains_key(&pk) {
                // Download déjà en cours, retourner la clé
                return Ok(pk);
            }
        }

        // Lancer le téléchargement avec transformer
        let transformer = self.transformer_factory.as_ref().map(|f| f());
        let download = download_with_transformer(&file_path, url, transformer);

        // Stocker dans la map des downloads en cours
        {
            let mut downloads = self.downloads.write().await;
            downloads.insert(pk.clone(), download.clone());
        }

        // Ajouter immédiatement à la DB
        self.db.add(&pk, url, collection)?;

        // Appliquer la politique d'éviction LRU si nécessaire
        // Cela garantit que le cache respecte toujours la limite configurée
        if let Err(e) = self.enforce_limit().await {
            tracing::warn!("Error enforcing cache limit: {}", e);
        }

        // Lancer une tâche de nettoyage en background
        let downloads_clone = self.downloads.clone();
        let pk_clone = pk.clone();
        tokio::spawn(async move {
            // Attendre la fin du téléchargement
            let _ = download.wait_until_finished().await;
            // Retirer de la map
            downloads_clone.write().await.remove(&pk_clone);
        });

        Ok(pk)
    }

    /// Ajoute un fichier local au cache
    ///
    /// Le fichier est copié dans le cache via une URL file://
    ///
    /// # Arguments
    ///
    /// * `path` - Chemin du fichier local
    /// * `collection` - Collection optionnelle à laquelle appartient le fichier
    ///
    /// # Returns
    ///
    /// La clé primaire (pk) du fichier dans le cache
    pub async fn add_from_file(&self, path: &str, collection: Option<&str>) -> Result<String> {
        let canonical_path = std::fs::canonicalize(path)?;
        let file_url = format!("file://{}", canonical_path.display());
        self.add_from_url(&file_url, collection).await
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
                // Re-télécharger le fichier manquant
                match self
                    .add_from_url(&entry.source_url, entry.collection.as_deref())
                    .await
                {
                    Ok(_) => {}
                    Err(_) => {
                        // Si le téléchargement échoue, supprimer l'entrée DB
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

    /// Récupère l'objet Download pour un pk donné (si en cours)
    ///
    /// # Arguments
    ///
    /// * `pk` - Clé primaire du fichier
    ///
    /// # Returns
    ///
    /// Some(Download) si le téléchargement est en cours, None sinon
    pub async fn get_download(&self, pk: &str) -> Option<Arc<Download>> {
        let downloads = self.downloads.read().await;
        downloads.get(pk).cloned()
    }

    /// Retourne la taille actuelle téléchargée (source)
    ///
    /// Si le download est en cours, retourne la taille téléchargée.
    /// Sinon, retourne la taille du fichier sur disque.
    ///
    /// # Arguments
    ///
    /// * `pk` - Clé primaire du fichier
    pub async fn current_size(&self, pk: &str) -> Option<u64> {
        if let Some(download) = self.get_download(pk).await {
            Some(download.current_size().await)
        } else {
            // Fichier terminé, lire la taille du fichier
            let file_path = self.file_path(pk);
            if file_path.exists() {
                std::fs::metadata(file_path).ok().map(|m| m.len())
            } else {
                None
            }
        }
    }

    /// Retourne la taille des données transformées
    ///
    /// Si le download est en cours, retourne la taille transformée.
    /// Sinon, retourne la taille du fichier sur disque.
    ///
    /// # Arguments
    ///
    /// * `pk` - Clé primaire du fichier
    pub async fn transformed_size(&self, pk: &str) -> Option<u64> {
        if let Some(download) = self.get_download(pk).await {
            Some(download.transformed_size().await)
        } else {
            // Fichier terminé, lire la taille du fichier
            let file_path = self.file_path(pk);
            if file_path.exists() {
                std::fs::metadata(file_path).ok().map(|m| m.len())
            } else {
                None
            }
        }
    }

    /// Retourne la taille attendue du fichier (si disponible)
    ///
    /// # Arguments
    ///
    /// * `pk` - Clé primaire du fichier
    pub async fn expected_size(&self, pk: &str) -> Option<u64> {
        if let Some(download) = self.get_download(pk).await {
            download.expected_size().await
        } else {
            // Fichier terminé, la taille finale est la taille du fichier
            self.transformed_size(pk).await
        }
    }

    /// Indique si le téléchargement est terminé
    ///
    /// # Arguments
    ///
    /// * `pk` - Clé primaire du fichier
    pub async fn is_finished(&self, pk: &str) -> bool {
        if let Some(download) = self.get_download(pk).await {
            download.finished().await
        } else {
            // Pas dans la map = terminé (ou n'existe pas)
            self.file_path(pk).exists()
        }
    }

    /// Attend qu'un fichier atteigne au moins une taille minimale
    ///
    /// # Arguments
    ///
    /// * `pk` - Clé primaire du fichier
    /// * `min_size` - Taille minimale attendue en bytes
    pub async fn wait_until_min_size(&self, pk: &str, min_size: u64) -> Result<()> {
        if let Some(download) = self.get_download(pk).await {
            download
                .wait_until_min_size(min_size)
                .await
                .map_err(|e| anyhow!("Download error: {}", e))
        } else {
            // Déjà terminé ou n'existe pas
            if self.file_path(pk).exists() {
                Ok(())
            } else {
                Err(anyhow!("File not found"))
            }
        }
    }

    /// Attend que le téléchargement soit complètement terminé
    ///
    /// # Arguments
    ///
    /// * `pk` - Clé primaire du fichier
    pub async fn wait_until_finished(&self, pk: &str) -> Result<()> {
        if let Some(download) = self.get_download(pk).await {
            download
                .wait_until_finished()
                .await
                .map_err(|e| anyhow!("Download error: {}", e))
        } else {
            // Déjà terminé ou n'existe pas
            if self.file_path(pk).exists() {
                Ok(())
            } else {
                Err(anyhow!("File not found"))
            }
        }
    }

    /// Retourne le répertoire du cache
    pub fn cache_dir(&self) -> &Path {
        &self.dir
    }

    /// Construit le chemin complet d'un fichier dans le cache avec le param par défaut
    ///
    /// Format: `{pk}.{default_param}.{extension}`
    pub fn file_path(&self, pk: &str) -> PathBuf {
        self.file_path_with_qualifier(pk, C::default_param())
    }

    /// Construit le chemin d'un fichier dans le cache avec un qualificatif
    ///
    /// Format: `{pk}.{qualifier}.{extension}`
    pub fn file_path_with_qualifier(&self, pk: &str, qualifier: &str) -> PathBuf {
        self.dir
            .join(format!("{}.{}.{}", pk, qualifier, C::file_extension()))
    }

    /// Valide les données avant de les stocker
    /// Par défaut, accepte toutes les données
    pub fn validate_data(&self, data: &[u8]) -> Result<Vec<u8>> {
        Ok(data.to_vec())
    }

    /// Applique la politique d'éviction LRU (Least Recently Used)
    ///
    /// Si le nombre d'entrées dépasse la limite configurée, supprime
    /// les entrées les plus anciennes (moins récemment utilisées).
    ///
    /// Cette méthode :
    /// 1. Compte le nombre total d'entrées
    /// 2. Si > limit, récupère les N entrées les plus anciennes
    /// 3. Supprime ces entrées de la DB et leurs fichiers du disque
    ///
    /// # Returns
    ///
    /// Le nombre d'entrées supprimées
    pub async fn enforce_limit(&self) -> Result<usize> {
        let count = self.db.count()?;

        if count <= self.limit {
            return Ok(0);
        }

        let to_remove = count - self.limit;
        let old_entries = self.db.get_oldest(to_remove)?;

        let mut removed = 0;
        for entry in old_entries {
            // Supprimer tous les fichiers avec ce pk (toutes variantes)
            if let Ok(mut dir_entries) = tokio::fs::read_dir(&self.dir).await {
                while let Ok(Some(dir_entry)) = dir_entries.next_entry().await {
                    if let Some(filename) = dir_entry.file_name().to_str() {
                        // Format: {pk}.{param}.{ext}
                        if filename.starts_with(&entry.pk)
                            && filename.starts_with(&format!("{}.", entry.pk))
                        {
                            let _ = tokio::fs::remove_file(dir_entry.path()).await;
                        }
                    }
                }
            }

            // Supprimer de la base de données
            if let Err(e) = self.db.delete(&entry.pk) {
                tracing::warn!("Error deleting entry {} from DB: {}", entry.pk, e);
            } else {
                removed += 1;
            }
        }

        if removed > 0 {
            tracing::info!(
                "LRU eviction: removed {} old entries (cache size: {} -> {})",
                removed,
                count,
                count - removed
            );
        }

        Ok(removed)
    }
}

/// Implémentation du trait FileCache pour Cache
impl<C: CacheConfig> FileCache<C> for Cache<C> {
    fn get_cache_dir(&self) -> &Path {
        self.cache_dir()
    }

    fn get_database(&self) -> Arc<DB> {
        self.db.clone()
    }

    fn validate_data(&self, data: &[u8]) -> Result<Vec<u8>> {
        // Le cache générique accepte toutes les données
        Ok(data.to_vec())
    }

    async fn add_from_url(&self, url: &str, collection: Option<&str>) -> Result<String> {
        self.add_from_url(url, collection).await
    }

    async fn add_from_file(&self, path: &str, collection: Option<&str>) -> Result<String> {
        self.add_from_file(path, collection).await
    }

    async fn ensure_from_url(&self, url: &str, collection: Option<&str>) -> Result<String> {
        self.ensure_from_url(url, collection).await
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
}
