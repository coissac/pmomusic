//! Module de gestion du cache générique
//!
//! Ce module fournit une interface générique pour gérer un cache de fichiers
//! avec métadonnées dans une base de données SQLite.

use crate::cache_trait::FileCache;
use crate::db::DB;
use crate::download::{
    download_with_transformer, ingest_with_transformer, Download, StreamTransformer,
};
use anyhow::{anyhow, bail, Result};
use serde_json::{Number, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::sync::RwLock;
use tracing;

/// Taille minimale de prébuffering par défaut (512 KB = ~5 secondes de FLAC)
pub const DEFAULT_PREBUFFER_SIZE: u64 = 512 * 1024;

/// Paramètres statiques d'un cache spécialisé.
pub trait CacheConfig: Send + Sync {
    /// Extension des fichiers générés (ex: `"webp"`, `"flac"`).
    fn file_extension() -> &'static str;
    /// Type logique exposé (ex: `"audio"`, `"image"`). Sert notamment pour les routes HTTP.
    fn cache_type() -> &'static str {
        "file"
    }
    /// Nom du cache (ex: `"covers"`, `"audio"`). Utilisé pour composer les chemins d'accès.
    fn cache_name() -> &'static str {
        "cache"
    }
    /// Qualifier par défaut associé au fichier original (ex: `"orig"`).
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
    /// Taille minimale de prébuffering en octets (0 = désactivé)
    min_prebuffer_size: u64,
    /// Phantom data pour le type de configuration
    _phantom: std::marker::PhantomData<C>,
}

impl<C: CacheConfig> Cache<C> {
    /// Retourne le chemin du fichier marker de complétion
    fn get_completion_marker_path(&self, pk: &str) -> PathBuf {
        self.get_file_path(pk)
            .with_extension(format!("{}.complete", C::file_extension()))
    }

    /// Vérifie si un fichier est en cache et complet
    ///
    /// # Returns
    ///
    /// - `Ok(true)` si le fichier est en cache et complet (fichier .complete existe)
    /// - `Ok(false)` si le fichier n'est pas en cache ou incomplet (et supprime les fichiers incomplets)
    /// - `Err` en cas d'erreur
    async fn check_cached_and_complete(&self, pk: &str) -> Result<bool> {
        if self.db.get(pk, false).is_ok() {
            let file_path = self.get_file_path(pk);
            let completion_marker = self.get_completion_marker_path(pk);

            if file_path.exists() {
                // Vérifier si le fichier marker de complétion existe
                if completion_marker.exists() {
                    tracing::debug!("File with pk {} is complete (marker exists)", pk);
                    return Ok(true);
                } else {
                    tracing::warn!(
                        "File with pk {} in cache has no completion marker, will re-download/re-ingest",
                        pk
                    );
                    // Supprimer le fichier incomplet
                    let _ = std::fs::remove_file(&file_path);
                    return Ok(false);
                }
            }
        }
        Ok(false)
    }

    /// Vérifie si un download est en cours et attend le prébuffering si nécessaire
    ///
    /// # Returns
    ///
    /// - `Ok(Some(pk))` si un download est en cours (et prébuffering terminé)
    /// - `Ok(None)` si aucun download en cours
    /// - `Err` en cas d'erreur de prébuffering
    async fn check_ongoing_download(&self, pk: &str) -> Result<Option<String>> {
        let download_handle = {
            let downloads = self.downloads.read().await;
            downloads.get(pk).cloned()
        };

        if let Some(download) = download_handle {
            tracing::debug!(
                "Download already in progress for pk {}, waiting for prebuffering",
                pk
            );

            if self.min_prebuffer_size > 0 {
                download
                    .wait_until_min_size(self.min_prebuffer_size)
                    .await
                    .map_err(|e| anyhow!("Prebuffering failed: {}", e))?;
                tracing::debug!("Prebuffering complete for pk {}", pk);
            }

            return Ok(Some(pk.to_string()));
        }

        Ok(None)
    }

    /// Finalise l'ajout d'un fichier au cache
    ///
    /// Cette fonction helper gère le prébuffering et le nettoyage en background
    async fn finalize_download(&self, pk: &str, download: Arc<Download>) -> Result<String> {
        // Attendre le prébuffering (pour le cache progressif)
        if self.min_prebuffer_size > 0 {
            download
                .wait_until_min_size(self.min_prebuffer_size)
                .await
                .map_err(|e| anyhow!("Prebuffering failed: {}", e))?;
            tracing::debug!(
                "Prebuffering complete for pk {} ({} bytes)",
                pk,
                self.min_prebuffer_size
            );
        }

        // Lancer une tâche de nettoyage et marquage de complétion en background
        let downloads_clone = self.downloads.clone();
        let pk_clone = pk.to_string();
        let completion_marker = self.get_completion_marker_path(pk);

        tokio::spawn(async move {
            let result = download.wait_until_finished().await;
            downloads_clone.write().await.remove(&pk_clone);

            // Créer le fichier marker de complétion si le téléchargement a réussi
            if result.is_ok() {
                if let Err(e) = std::fs::write(&completion_marker, "") {
                    tracing::warn!(
                        "Failed to create completion marker for pk {}: {}",
                        pk_clone,
                        e
                    );
                } else {
                    tracing::debug!("Created completion marker for pk {}", pk_clone);
                }
            }
        });

        Ok(pk.to_string())
    }

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
    ///     Box::new(|input, file, ctx| {
    ///         Box::pin(async move {
    ///             // Transformation personnalisée
    ///             ctx.report_progress(0);
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
        let db = DB::init(&directory.join("cache.db"))?;

        Ok(Self {
            dir: directory,
            limit,
            db: Arc::new(db),
            downloads: Arc::new(RwLock::new(HashMap::new())),
            transformer_factory,
            min_prebuffer_size: DEFAULT_PREBUFFER_SIZE,
            _phantom: std::marker::PhantomData,
        })
    }

    /// Configure la taille minimale de prébuffering
    ///
    /// # Arguments
    ///
    /// * `size` - Taille minimale en octets (0 = désactivé)
    ///
    /// # Exemple
    ///
    /// ```rust,no_run
    /// use pmocache::{Cache, CacheConfig};
    ///
    /// struct MyConfig;
    /// impl CacheConfig for MyConfig {
    ///     fn file_extension() -> &'static str { "dat" }
    /// }
    ///
    /// let mut cache = Cache::<MyConfig>::new("./cache", 1000).unwrap();
    /// cache.set_prebuffer_size(1024 * 1024); // 1 MB de prébuffering
    /// ```
    pub fn set_prebuffer_size(&mut self, size: u64) {
        self.min_prebuffer_size = size;
    }

    /// Retourne la taille minimale de prébuffering configurée
    pub fn get_prebuffer_size(&self) -> u64 {
        self.min_prebuffer_size
    }

    /// Télécharge un fichier depuis une URL et l'ajoute au cache
    ///
    /// Cette méthode utilise un système d'identifiants basé sur le contenu plutôt que sur l'URL.
    /// Elle télécharge les 512 premiers octets du fichier pour calculer un identifiant unique (pk),
    /// puis vérifie si le fichier est déjà en cache. Si c'est le cas, elle met à jour le timestamp
    /// et retourne rapidement. Sinon, elle lance le téléchargement complet en arrière-plan.
    ///
    /// # Workflow
    ///
    /// 1. Télécharge les 512 premiers octets via une requête HTTP partielle
    /// 2. Calcule le pk en hashant (SHA256) ces premiers octets
    /// 3. Vérifie si le fichier existe déjà dans le cache
    /// 4. Si oui : update timestamp et retour rapide
    /// 5. Si non : lance le téléchargement complet en background
    ///
    /// # Arguments
    ///
    /// * `url` - URL du fichier à télécharger
    /// * `collection` - Collection optionnelle à laquelle appartient le fichier
    ///
    /// # Returns
    ///
    /// La clé primaire (pk) du fichier dans le cache, calculée à partir du contenu
    ///
    /// # Note
    ///
    /// Deux URLs différentes pointant vers le même contenu auront le même pk,
    /// permettant une déduplication automatique.
    pub async fn add_from_url(&self, url: &str, collection: Option<&str>) -> Result<String> {
        // 1. Télécharger les 512 premiers octets pour calculer le pk
        let header = crate::download::peek_header(url, 512)
            .await
            .map_err(|e| anyhow!("Failed to peek header: {}", e))?;

        // 2. Calculer le pk basé sur le contenu
        let pk = crate::cache_trait::pk_from_content_header(&header);
        tracing::debug!("Computed pk {} for URL {}", pk, url);

        // 3. Vérifier si le fichier est déjà en cache ET complet
        if self.check_cached_and_complete(&pk).await? {
            tracing::debug!("File with pk {} already in cache, updating timestamp", pk);
            self.db.update_hit(&pk)?;
            return Ok(pk);
        }

        // 4. Vérifier si un download est déjà en cours pour ce pk
        if let Some(pk) = self.check_ongoing_download(&pk).await? {
            return Ok(pk);
        }

        // 5. Lancer le téléchargement complet avec transformer
        tracing::debug!("Starting full download for pk {} from URL {}", pk, url);
        let file_path = self.get_file_path(&pk);
        let transformer = self.transformer_factory.as_ref().map(|f| f());
        let download = download_with_transformer(&file_path, url, transformer);

        // Stocker dans la map des downloads en cours
        {
            let mut downloads = self.downloads.write().await;
            downloads.insert(pk.clone(), download.clone());
        }

        // Ajouter immédiatement à la DB
        self.db.add(&pk, None, collection)?;
        self.db.set_origin_url(&pk, url)?;

        // Appliquer la politique d'éviction LRU si nécessaire
        if let Err(e) = self.enforce_limit().await {
            tracing::warn!("Error enforcing cache limit: {}", e);
        }

        // Finaliser avec prébuffering et nettoyage
        self.finalize_download(&pk, download).await
    }

    /// Ajoute un fichier à partir d'un flux asynchrone.
    ///
    /// Cette méthode utilise le même système d'identifiants basé sur le contenu que `add_from_url`.
    /// Elle lit les 512 premiers octets du flux pour calculer l'identifiant, puis reconstitue
    /// le flux complet pour l'ingestion.
    ///
    /// # Workflow
    ///
    /// 1. Lit les 512 premiers octets du reader
    /// 2. Calcule le pk en hashant (SHA256) ces premiers octets
    /// 3. Vérifie si le fichier existe déjà dans le cache
    /// 4. Si oui : update timestamp et retour rapide
    /// 5. Si non : reconstitue le reader (header + reste) et lance l'ingestion
    ///
    /// # Arguments
    ///
    /// * `source_uri` - Identifiant logique optionnel du flux (pour traçabilité dans la DB). Si None, l'origin_url ne sera pas sauvegardée.
    /// * `reader` - Flux asynchrone fournissant les données
    /// * `length` - Taille attendue (si connue)
    /// * `collection` - Collection optionnelle à laquelle appartient l'élément
    ///
    /// # Returns
    ///
    /// La clé primaire (pk) du fichier dans le cache, calculée à partir du contenu
    pub async fn add_from_reader<R>(
        &self,
        source_uri: Option<&str>,
        mut reader: R,
        length: Option<u64>,
        collection: Option<&str>,
    ) -> Result<String>
    where
        R: AsyncRead + Send + Unpin + 'static,
    {
        self.add_from_reader_with_pk(source_uri, reader, length, collection, None)
            .await
    }

    /// Ajoute un fichier à partir d'un flux avec un pk explicite optionnel.
    ///
    /// Si `explicit_pk` est fourni, utilise ce pk au lieu de le calculer à partir du contenu.
    /// Ceci est utile quand plusieurs fichiers ont le même header mais doivent être cachés séparément
    /// (par exemple, des fichiers FLAC avec le même format mais du contenu différent).
    pub async fn add_from_reader_with_pk<R>(
        &self,
        source_uri: Option<&str>,
        mut reader: R,
        length: Option<u64>,
        collection: Option<&str>,
        explicit_pk: Option<String>,
    ) -> Result<String>
    where
        R: AsyncRead + Send + Unpin + 'static,
    {
        // 1. Lire EXACTEMENT 1024 octets (ou EOF si fichier plus petit)
        // Utilise read_exact_or_eof qui boucle jusqu'à avoir tous les octets demandés
        let header = crate::download::read_exact_or_eof(&mut reader, 1024)
            .await
            .map_err(|e| anyhow!("Failed to read header bytes: {}", e))?;

        // 2. Calculer le pk selon la taille du fichier
        // - Fichiers >= 1024 octets (FLAC): skip header (512 premiers octets), utilise octets 512-1024
        // - Fichiers < 1024 octets (images, petits fichiers): utilise TOUT le contenu
        let pk = if let Some(explicit) = explicit_pk {
            explicit
        } else {
            let pk_bytes = if header.len() >= 1024 {
                // Gros fichier (>= 1024 octets): skip les 512 premiers (header FLAC)
                &header[512..]
            } else {
                // Petit fichier (< 1024 octets): utiliser TOUT le contenu
                &header[..]
            };
            crate::cache_trait::pk_from_content_header(pk_bytes)
        };
        if let Some(uri) = source_uri {
            tracing::debug!("Computed pk {} for source_uri {}", pk, uri);
        } else {
            tracing::debug!("Computed pk {} from reader", pk);
        }

        // 3. Vérifier si le fichier est déjà en cache ET complet
        if self.check_cached_and_complete(&pk).await? {
            tracing::debug!("File with pk {} already in cache, updating timestamp", pk);
            self.db.update_hit(&pk)?;
            return Ok(pk);
        }

        // 4. Vérifier si un download est déjà en cours pour ce pk
        if let Some(pk) = self.check_ongoing_download(&pk).await? {
            return Ok(pk);
        }

        // 5. Reconstituer le reader complet (header + reste)
        use std::io::Cursor;
        let header_reader = Cursor::new(header);
        let full_reader = header_reader.chain(reader);

        // 6. Lancer l'ingestion avec transformer
        tracing::debug!("Starting ingestion for pk {} from reader", pk);
        let file_path = self.get_file_path(&pk);
        let transformer = self.transformer_factory.as_ref().map(|factory| factory());
        let download = ingest_with_transformer(&file_path, full_reader, length, transformer);

        {
            let mut downloads = self.downloads.write().await;
            downloads.insert(pk.clone(), download.clone());
        }

        self.db.add(&pk, None, collection)?;
        if let Some(uri) = source_uri {
            self.db.set_origin_url(&pk, uri)?;
        }

        if let Err(e) = self.enforce_limit().await {
            tracing::warn!("Error enforcing cache limit: {}", e);
        }

        // Finaliser avec prébuffering et nettoyage
        self.finalize_download(&pk, download).await
    }

    /// Ajoute un fichier local au cache
    ///
    /// Cette méthode lit les 512 premiers octets du fichier local pour calculer
    /// l'identifiant basé sur le contenu, puis utilise `add_from_reader()` pour
    /// l'ingestion complète.
    ///
    /// # Arguments
    ///
    /// * `path` - Chemin du fichier local
    /// * `collection` - Collection optionnelle à laquelle appartient le fichier
    ///
    /// # Returns
    ///
    /// La clé primaire (pk) du fichier dans le cache, calculée à partir du contenu
    ///
    /// # Exemple
    ///
    /// ```rust,ignore
    /// use pmocache::{Cache, CacheConfig};
    ///
    /// struct MyConfig;
    /// impl CacheConfig for MyConfig {
    ///     fn file_extension() -> &'static str { "dat" }
    /// }
    ///
    /// let cache = Cache::<MyConfig>::new("./cache", 1000)?;
    /// let pk = cache.add_from_file("/path/to/file.dat", None).await?;
    /// ```
    pub async fn add_from_file(&self, path: &str, collection: Option<&str>) -> Result<String> {
        let canonical_path = std::fs::canonicalize(path)?;
        let file_url = format!("file://{}", canonical_path.display());
        let length = tokio::fs::metadata(&canonical_path)
            .await
            .ok()
            .map(|m| m.len());
        let reader = tokio::fs::File::open(&canonical_path).await?;

        // add_from_reader() s'occupe de lire les 512 premiers octets et de calculer le pk
        self.add_from_reader(Some(&file_url), reader, length, collection)
            .await
    }

    pub async fn delete_item(&self, pk: &str) -> Result<()> {
        // Vérifie l'existence pour signaler une erreur explicite si l'entrée est absente
        self.db.get(pk, false)?;

        // Oublie un téléchargement en cours pour cette clé
        self.downloads.write().await.remove(pk);

        // Supprime chaque fichier {pk}.{qualifier}.{ext} (ignorer si déjà absent)
        for path in self.get_file_paths(pk)? {
            if let Err(err) = tokio::fs::remove_file(&path).await {
                if err.kind() != std::io::ErrorKind::NotFound {
                    return Err(err.into());
                }
            }
        }

        // Efface l’entrée de la base (les métadonnées partent via ON DELETE CASCADE)
        self.db.delete(pk)?;

        Ok(())
    }

    pub async fn delete_collection(&self, collection: &str) -> Result<()> {
        let entries = self.db.get_by_collection(collection, false)?;

        {
            let mut downloads = self.downloads.write().await;
            for entry in &entries {
                downloads.remove(&entry.pk);
            }
        }

        for entry in &entries {
            for path in self.get_file_paths(&entry.pk)? {
                if let Err(err) = tokio::fs::remove_file(&path).await {
                    if err.kind() != std::io::ErrorKind::NotFound {
                        return Err(err.into());
                    }
                }
            }
        }

        self.db.delete_collection(collection)?;
        Ok(())
    }

    /// Récupère le chemin d'un fichier dans le cache
    ///
    /// # Arguments
    ///
    /// * `pk` - Clé primaire du fichier
    pub async fn get(&self, pk: &str) -> Result<PathBuf> {
        self.db.get(pk, false)?;
        self.db.update_hit(pk)?;

        let file_path = self.get_file_path(pk);
        if file_path.exists() {
            Ok(file_path)
        } else {
            Err(anyhow!("File not found"))
        }
    }

    /// Récupère une métadonnée précise pour une entrée du cache.
    pub async fn get_a_metadata(&self, pk: &str, key: &str) -> Result<Option<Value>> {
        Ok(self.db.get_metadata_value(pk, key)?)
    }

    /// Récupère une métadonnée en tant que chaîne, si disponible.
    pub async fn get_a_metadata_as_string(&self, pk: &str, key: &str) -> Result<Option<String>> {
        match self.get_a_metadata(pk, key).await? {
            Some(Value::String(s)) => Ok(Some(s)),
            Some(Value::Null) | None => Ok(None),
            Some(other) => bail!("metadata '{key}' for pk '{pk}' is not a string (found {other})"),
        }
    }

    /// Récupère une métadonnée en tant que nombre JSON (`serde_json::Number`).
    pub async fn get_a_metadata_as_number(&self, pk: &str, key: &str) -> Result<Option<Number>> {
        match self.get_a_metadata(pk, key).await? {
            Some(Value::Number(n)) => Ok(Some(n)),
            Some(Value::Null) | None => Ok(None),
            Some(other) => bail!("metadata '{key}' for pk '{pk}' is not a number (found {other})"),
        }
    }

    /// Récupère une métadonnée en tant que booléen.
    pub async fn get_a_metadata_as_bool(&self, pk: &str, key: &str) -> Result<Option<bool>> {
        match self.get_a_metadata(pk, key).await? {
            Some(Value::Bool(b)) => Ok(Some(b)),
            Some(Value::Null) | None => Ok(None),
            Some(other) => bail!("metadata '{key}' for pk '{pk}' is not a boolean (found {other})"),
        }
    }
    pub async fn touch(&self, pk: &str) -> Result<()> {
        self.db.update_hit(pk)?;
        Ok(())
    }

    /// Récupère tous les fichiers d'une collection
    ///
    /// # Arguments
    ///
    /// * `collection` - Identifiant de la collection
    pub async fn get_collection(&self, collection: &str) -> Result<Vec<PathBuf>> {
        let entries = self.db.get_by_collection(collection, false)?;
        let mut paths = Vec::new();

        for entry in entries {
            let path = self.get_file_path(&entry.pk);
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
    ///
    /// Cette fonction :
    /// - Supprime les entrées DB sans fichiers (ou re-télécharge si URL disponible)
    /// - Supprime les fichiers sans marker de complétion et leurs entrées DB
    /// - Supprime les fichiers sans entrées DB correspondantes
    pub async fn consolidate(&self) -> Result<()> {
        // Récupérer la liste des entrées à traiter
        let entries = self.db.get_all(false)?;

        // Supprimer les entrées sans fichiers correspondants OU sans marker de complétion
        for entry in entries {
            let file_path = self.get_file_path(&entry.pk);
            let completion_marker = self.get_completion_marker_path(&entry.pk);

            if !file_path.exists() {
                // Fichier manquant, essayer de re-télécharger
                match self.db.get_origin_url(&entry.pk)? {
                    Some(url) => {
                        if let Err(err) = self.add_from_url(&url, entry.collection.as_deref()).await
                        {
                            tracing::warn!(
                                "Unable to redownload missing file for {}: {}",
                                entry.pk,
                                err
                            );
                            self.db.delete(&entry.pk)?;
                        }
                    }
                    None => {
                        self.db.delete(&entry.pk)?;
                    }
                }
            } else if !completion_marker.exists() {
                // Fichier existe mais pas de marker de complétion -> fichier incomplet
                tracing::warn!(
                    "Removing incomplete file {} (no completion marker)",
                    entry.pk
                );
                let _ = tokio::fs::remove_file(&file_path).await;
                self.db.delete(&entry.pk)?;
            }
        }

        // Supprimer les fichiers sans entrées DB correspondantes
        let mut dir_entries = tokio::fs::read_dir(&self.dir).await?;
        while let Some(entry) = dir_entries.next_entry().await? {
            let path = entry.path();
            if path.is_file() && path != self.dir.join("cache.db") {
                if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
                    // Ignorer les fichiers .complete
                    if file_name.ends_with(".complete") {
                        continue;
                    }

                    // Format attendu: {pk}.{qualifier}.{EXT}
                    // On extrait le pk (première partie avant le premier point)
                    if let Some(pk) = file_name.split('.').next() {
                        if self.db.get(pk, false).is_err() {
                            tracing::debug!("Removing orphan file: {}", file_name);
                            tokio::fs::remove_file(&path).await?;
                            // Supprimer aussi le marker de complétion s'il existe
                            let completion_marker = self.get_completion_marker_path(pk);
                            let _ = tokio::fs::remove_file(&completion_marker).await;
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

    /// Vérifie si le téléchargement/ingestion d'un fichier est complètement terminé
    ///
    /// Cette méthode vérifie l'existence du fichier marker de complétion (.complete)
    /// qui est créé uniquement quand le fichier est complètement écrit et fermé.
    ///
    /// Utile pour différencier:
    /// - EOF temporaire : fichier encore en cours d'écriture (retourne false)
    /// - EOF réel : fichier complètement écrit (retourne true)
    ///
    /// # Arguments
    ///
    /// * `pk` - Clé primaire du fichier
    ///
    /// # Returns
    ///
    /// `true` si le fichier est complètement écrit (marker existe), `false` sinon
    pub fn is_download_complete(&self, pk: &str) -> bool {
        let completion_marker = self.get_completion_marker_path(pk);
        completion_marker.exists()
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
            let file_path = self.get_file_path(pk);
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
            let file_path = self.get_file_path(pk);
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

    /// Retourne les métadonnées de transformation (si disponibles)
    pub async fn transform_metadata(&self, pk: &str) -> Option<crate::download::TransformMetadata> {
        if let Some(download) = self.get_download(pk).await {
            download.transform_metadata().await
        } else {
            None
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
            self.get_file_path(pk).exists()
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
            if self.get_file_path(pk).exists() {
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
            if self.get_file_path(pk).exists() {
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
    pub fn get_file_path(&self, pk: &str) -> PathBuf {
        self.get_file_path_with_qualifier(pk, C::default_param())
    }

    /// Construit le chemin d'un fichier dans le cache avec un qualificatif
    ///
    /// Format: `{pk}.{qualifier}.{extension}`
    pub fn get_file_path_with_qualifier(&self, pk: &str, qualifier: &str) -> PathBuf {
        self.dir
            .join(format!("{}.{}.{}", pk, qualifier, C::file_extension()))
    }

    /// Retourne tous les chemins de fichiers stockés pour une clé donnée,
    /// quel que soit le qualifier.
    ///
    /// Format: `{pk}.*.{extension}`
    pub fn get_file_paths(&self, pk: &str) -> Result<Vec<PathBuf>> {
        let mut paths = Vec::new();
        let prefix = format!("{pk}.");
        let expected_ext = C::file_extension();

        for entry in std::fs::read_dir(&self.dir)? {
            let entry = entry?;
            let path = entry.path();

            if !path.is_file() {
                continue;
            }

            let file_name = match entry.file_name().into_string() {
                Ok(name) => name,
                Err(_) => continue, // nom de fichier non UTF-8 : on l’ignore
            };

            if !file_name.starts_with(&prefix) {
                continue;
            }

            if !file_name.ends_with(expected_ext) {
                continue;
            }

            paths.push(path);
        }

        Ok(paths)
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
            // Utiliser get_file_paths() pour obtenir tous les fichiers de cette entrée
            if let Ok(paths) = self.get_file_paths(&entry.pk) {
                for path in paths {
                    let _ = tokio::fs::remove_file(path).await;
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
