//! Gestion du cache pour les sources musicales
//!
//! Ce module fournit `SourceCacheManager` qui permet aux sources
//! d'utiliser les caches centralisés du serveur.
//!
//! ## Architecture
//!
//! Les caches (couvertures et audio) sont centralisés au niveau du serveur UPnP.
//! Chaque source utilise ces caches partagés avec sa propre collection.
//!
//! ```text
//! UpnpServer
//!   ├─ CoverCache (partagé)
//!   │   ├─ collection: "radio-paradise"
//!   │   └─ collection: "qobuz"
//!   └─ AudioCache (partagé)
//!       ├─ collection: "radio-paradise"
//!       └─ collection: "qobuz"
//! ```

use crate::{CacheStatus, MusicSourceError, Result};
use pmoaudiocache::{AudioMetadata, Cache as AudioCache};
use pmocovers::Cache as CoverCache;
use serde_json::{json, Value as JsonValue};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::AsyncRead;
use tokio::sync::RwLock;

fn log_metadata_warning(key: &str, audio_pk: &str, error: &str) {
    #[cfg(feature = "server")]
    {
        tracing::warn!(
            "Failed to store metadata {} for {}: {}",
            key,
            audio_pk,
            error
        );
    }
    #[cfg(not(feature = "server"))]
    {
        let _ = (key, audio_pk, error);
    }
}

/// Métadonnées d'une piste en cache
#[derive(Debug, Clone)]
pub struct TrackMetadata {
    /// URI originale de la piste
    pub original_uri: String,

    /// Clé primaire du fichier audio en cache
    pub cached_audio_pk: Option<String>,

    /// Clé primaire de la couverture en cache
    pub cached_cover_pk: Option<String>,
}

/// Manager centralisé pour gérer le cache d'une source
///
/// Utilise les caches centralisés du serveur avec la collection de la source.
/// Chaque source a son propre `SourceCacheManager` mais partage les mêmes
/// caches (cover et audio) avec les autres sources.
pub struct SourceCacheManager {
    /// Métadonnées des pistes (track_id → metadata)
    track_cache: RwLock<HashMap<String, TrackMetadata>>,

    /// ID de collection pour cette source (ex: "radio-paradise", "qobuz")
    collection_id: String,

    /// Référence au cache de couvertures centralisé
    cover_cache: Arc<CoverCache>,

    /// Référence au cache audio centralisé
    audio_cache: Arc<AudioCache>,
}

impl SourceCacheManager {
    /// Créer un nouveau manager depuis le registre de caches
    ///
    /// Cette méthode utilise le registre global de caches (`CACHE_REGISTRY`)
    /// pour récupérer les caches centralisés du serveur.
    ///
    /// # Arguments
    ///
    /// * `collection_id` - ID de collection pour cette source (ex: "radio-paradise", "qobuz")
    ///
    /// # Returns
    ///
    /// Un nouveau `SourceCacheManager` configuré avec les caches centralisés
    ///
    /// # Errors
    ///
    /// Retourne une erreur si les caches ne sont pas encore initialisés dans le registre
    #[cfg(feature = "server")]
    pub fn from_registry(collection_id: String) -> Result<Self> {
        let cover_cache = pmoupnp::cache_registry::get_cover_cache().ok_or_else(|| {
            MusicSourceError::CacheError("Cover cache not initialized in registry".to_string())
        })?;

        let audio_cache = pmoupnp::cache_registry::get_audio_cache().ok_or_else(|| {
            MusicSourceError::CacheError("Audio cache not initialized in registry".to_string())
        })?;

        Ok(Self {
            track_cache: RwLock::new(HashMap::new()),
            collection_id,
            cover_cache,
            audio_cache,
        })
    }

    /// Créer un nouveau manager (ancien constructeur pour tests)
    ///
    /// # Arguments
    ///
    /// * `collection_id` - ID de collection (source ID)
    /// * `cover_cache` - Cache de couvertures centralisé
    /// * `audio_cache` - Cache audio centralisé
    pub fn new(
        collection_id: String,
        cover_cache: Arc<CoverCache>,
        audio_cache: Arc<AudioCache>,
    ) -> Self {
        Self {
            track_cache: RwLock::new(HashMap::new()),
            collection_id,
            cover_cache,
            audio_cache,
        }
    }

    /// Résoudre l'URI d'une piste (priorité au cache)
    ///
    /// Retourne l'URI du fichier audio en cache si disponible,
    /// sinon l'URI originale.
    pub async fn resolve_uri(&self, object_id: &str) -> Result<String> {
        let cache = self.track_cache.read().await;

        if let Some(metadata) = cache.get(object_id) {
            if let Some(ref _pk) = metadata.cached_audio_pk {
                #[cfg(feature = "server")]
                {
                    let url = pmoupnp::cache_registry::build_audio_url(_pk, Some("stream"))
                        .map_err(|e| MusicSourceError::CacheError(e.to_string()))?;
                    return Ok(url);
                }
                #[cfg(not(feature = "server"))]
                {
                    return Err(MusicSourceError::CacheError(
                        "Server feature not enabled".to_string(),
                    ));
                }
            }
            return Ok(metadata.original_uri.clone());
        }

        Err(MusicSourceError::ObjectNotFound(object_id.to_string()))
    }

    /// Obtenir le statut du cache pour une piste
    pub async fn get_cache_status(&self, object_id: &str) -> Result<CacheStatus> {
        let cache = self.track_cache.read().await;

        if let Some(metadata) = cache.get(object_id) {
            if let Some(ref _pk) = metadata.cached_audio_pk {
                // TODO: Ajouter get_info() à AudioCache
                // Pour l'instant, on retourne juste Cached sans taille
                return Ok(CacheStatus::Cached { size_bytes: 0 });
            }
        }

        Ok(CacheStatus::NotCached)
    }

    /// Cacher une couverture depuis une URL
    ///
    /// Utilise la collection de cette source pour organiser les images.
    ///
    /// # Returns
    ///
    /// La clé primaire (pk) de l'image dans le cache
    pub async fn cache_cover(&self, url: &str) -> Result<String> {
        self.cover_cache
            .add_from_url(url, Some(&self.collection_id))
            .await
            .map_err(|e| MusicSourceError::CacheError(e.to_string()))
    }

    /// Obtenir l'URL d'une couverture en cache
    ///
    /// # Arguments
    ///
    /// * `pk` - Clé primaire de l'image dans le cache
    /// * `size` - Taille optionnelle (génère une variante si spécifiée)
    ///
    /// # Returns
    ///
    /// L'URL complète de l'image
    pub fn cover_url(&self, _pk: &str, _size: Option<usize>) -> Result<String> {
        #[cfg(feature = "server")]
        {
            pmoupnp::cache_registry::build_cover_url(_pk, _size)
                .map_err(|e| MusicSourceError::CacheError(e.to_string()))
        }
        #[cfg(not(feature = "server"))]
        {
            Err(MusicSourceError::CacheError(
                "Server feature not enabled - cannot build cover URL".to_string(),
            ))
        }
    }

    /// Cacher une piste audio depuis une URL
    ///
    /// Utilise la collection de cette source pour organiser les pistes.
    ///
    /// # Arguments
    ///
    /// * `url` - URL source de la piste
    /// * `_metadata` - Métadonnées audio optionnelles (unused, kept for API compatibility)
    ///
    /// # Returns
    ///
    /// La clé primaire (pk) de la piste dans le cache
    pub async fn cache_audio(&self, url: &str, _metadata: Option<AudioMetadata>) -> Result<String> {
        // Note: Les métadonnées seront extraites automatiquement par le cache audio
        // lors de la conversion FLAC
        let pk = self
            .audio_cache
            .add_from_url(url, Some(&self.collection_id))
            .await
            .map_err(|e| MusicSourceError::CacheError(e.to_string()))?;
        Ok(pk)
    }

    /// Cache audio with lazy loading (deferred download)
    ///
    /// Creates a lazy PK for the audio file without downloading it immediately.
    /// The actual download will occur when the HTTP endpoint is first requested.
    ///
    /// # Arguments
    ///
    /// * `url` - URL source de la piste
    /// * `metadata` - Métadonnées audio optionnelles
    ///
    /// # Returns
    ///
    /// La clé primaire lazy (pk) commençant par "L:"
    pub async fn cache_audio_lazy(
        &self,
        url: &str,
        metadata: Option<AudioMetadata>,
    ) -> Result<String> {
        // Use pmocache add_from_url_deferred (already implemented)
        let lazy_pk = self
            .audio_cache
            .add_from_url_deferred(url, Some(&self.collection_id))
            .await
            .map_err(|e| MusicSourceError::CacheError(e.to_string()))?;

        // Store metadata in lazy_pk metadata table if provided
        if let Some(meta) = metadata {
            // Serialize metadata to JSON and store
            if let Ok(json) = serde_json::to_value(&meta) {
                let _ = self
                    .audio_cache
                    .db
                    .set_a_metadata_by_key(&lazy_pk, "audio_metadata", json);
            }
            self.seed_audio_metadata(&lazy_pk, &meta);
        }

        Ok(lazy_pk)
    }

    /// Cache un flux audio via un reader asynchrone
    pub async fn cache_audio_from_reader<R>(
        &self,
        source_uri: &str,
        reader: R,
        length: Option<u64>,
    ) -> Result<String>
    where
        R: AsyncRead + Send + Unpin + 'static,
    {
        let pk = self
            .audio_cache
            .add_from_reader(Some(source_uri), reader, length, Some(&self.collection_id))
            .await
            .map_err(|e| MusicSourceError::CacheError(e.to_string()))?;
        Ok(pk)
    }

    /// Attend que le fichier audio correspondant soit complètement disponible
    pub async fn wait_audio_ready(&self, pk: &str) -> Result<()> {
        self.audio_cache
            .wait_until_finished(pk)
            .await
            .map_err(|e| MusicSourceError::CacheError(e.to_string()))
    }

    /// Mettre à jour les métadonnées d'une piste
    ///
    /// Enregistre ou met à jour les métadonnées de cache pour une piste.
    pub async fn update_metadata(&self, track_id: String, metadata: TrackMetadata) {
        let mut cache = self.track_cache.write().await;
        cache.insert(track_id, metadata);
    }

    /// Récupérer les métadonnées d'une piste
    pub async fn get_metadata(&self, track_id: &str) -> Option<TrackMetadata> {
        let cache = self.track_cache.read().await;
        cache.get(track_id).cloned()
    }

    /// Supprimer une piste du cache
    pub async fn remove_track(&self, track_id: &str) {
        let mut cache = self.track_cache.write().await;
        cache.remove(track_id);
    }

    /// Pré-remplit les métadonnées audio pour une entrée lazy
    fn seed_audio_metadata(&self, audio_pk: &str, metadata: &AudioMetadata) {
        let audio_pk = audio_pk.to_string();
        let mut store = |key: &str, value: JsonValue| {
            if let Err(e) = self.audio_cache.db.set_a_metadata(&audio_pk, key, value) {
                log_metadata_warning(key, &audio_pk, &e.to_string());
            }
        };

        if let Some(title) = metadata.title.as_ref() {
            store("title", JsonValue::String(title.clone()));
        }
        if let Some(artist) = metadata.artist.as_ref() {
            store("artist", JsonValue::String(artist.clone()));
        }
        if let Some(album) = metadata.album.as_ref() {
            store("album", JsonValue::String(album.clone()));
        }
        if let Some(year) = metadata.year {
            store("year", json!(year));
        }
        if let Some(track_number) = metadata.track_number {
            store("track_number", json!(track_number));
        }
        if let Some(track_total) = metadata.track_total {
            store("track_total", json!(track_total));
        }
        if let Some(disc_number) = metadata.disc_number {
            store("disc_number", json!(disc_number));
        }
        if let Some(disc_total) = metadata.disc_total {
            store("disc_total", json!(disc_total));
        }
        if let Some(genre) = metadata.genre.as_ref() {
            store("genre", JsonValue::String(genre.clone()));
        }
        if let Some(duration_secs) = metadata.duration_secs {
            store("duration_secs", json!(duration_secs));
            store("duration_ms", json!(duration_secs * 1000));
        }
        if let Some(sample_rate) = metadata.sample_rate {
            store("sample_rate", json!(sample_rate));
        }
        if let Some(channels) = metadata.channels {
            store("channels", json!(channels));
        }
        if let Some(bitrate) = metadata.bitrate {
            store("bitrate", json!(bitrate));
        }
        if let Some(conversion) = metadata.conversion.as_ref() {
            if let Ok(value) = serde_json::to_value(conversion) {
                store("conversion", value);
            }
        }
    }

    /// Stocke une métadonnée personnalisée pour un fichier audio caché
    ///
    /// Permet de stocker des métadonnées arbitraires (clé/valeur JSON) associées
    /// à un fichier audio identifié par son PK. Ces métadonnées sont persistées
    /// dans la base de données SQLite du cache audio.
    ///
    /// # Arguments
    ///
    /// * `audio_pk` - Clé primaire du fichier audio dans le cache
    /// * `key` - Nom de la métadonnée à stocker
    /// * `value` - Valeur JSON à stocker
    ///
    /// # Exemples
    ///
    /// ```no_run
    /// # use pmosource::SourceCacheManager;
    /// # use serde_json::json;
    /// # async fn example(cache_manager: &SourceCacheManager, audio_pk: &str) {
    /// // Stocker une métadonnée simple
    /// cache_manager.set_audio_metadata(audio_pk, "genre", json!("Rock")).unwrap();
    ///
    /// // Stocker une métadonnée numérique
    /// cache_manager.set_audio_metadata(audio_pk, "rating", json!(8.5)).unwrap();
    /// # }
    /// ```
    pub fn set_audio_metadata(&self, audio_pk: &str, key: &str, value: JsonValue) -> Result<()> {
        self.audio_cache
            .db
            .set_a_metadata(audio_pk, key, value)
            .map_err(|e| MusicSourceError::CacheError(format!("Failed to set metadata: {}", e)))
    }

    /// Récupère une métadonnée personnalisée pour un fichier audio caché
    ///
    /// Lit une métadonnée précédemment stockée via `set_audio_metadata()`.
    ///
    /// # Arguments
    ///
    /// * `audio_pk` - Clé primaire du fichier audio dans le cache
    /// * `key` - Nom de la métadonnée à récupérer
    ///
    /// # Returns
    ///
    /// * `Ok(Some(value))` - La métadonnée existe
    /// * `Ok(None)` - La métadonnée n'existe pas
    /// * `Err(_)` - Erreur de lecture
    ///
    /// # Exemples
    ///
    /// ```no_run
    /// # use pmosource::SourceCacheManager;
    /// # async fn example(cache_manager: &SourceCacheManager, audio_pk: &str) {
    /// if let Some(genre) = cache_manager.get_audio_metadata(audio_pk, "genre").unwrap() {
    ///     println!("Genre: {}", genre);
    /// }
    /// # }
    /// ```
    pub fn get_audio_metadata(&self, audio_pk: &str, key: &str) -> Result<Option<JsonValue>> {
        match self.audio_cache.db.get_a_metadata(audio_pk, key) {
            Ok(value) => Ok(value),
            Err(e) if e.to_string().contains("QueryReturnedNoRows") => Ok(None),
            Err(e) => Err(MusicSourceError::CacheError(format!(
                "Failed to get metadata: {}",
                e
            ))),
        }
    }

    /// Obtenir l'ID de collection
    pub fn collection_id(&self) -> &str {
        &self.collection_id
    }

    /// Obtenir les statistiques du cache pour cette source
    pub async fn statistics(&self) -> CacheStatistics {
        let cache = self.track_cache.read().await;
        let cached_count = cache
            .values()
            .filter(|m| m.cached_audio_pk.is_some())
            .count();

        CacheStatistics {
            total_tracks: cache.len(),
            cached_tracks: cached_count,
            collection_id: self.collection_id.clone(),
        }
    }

    /// Récupère le chemin de fichier pour une piste audio en cache
    ///
    /// Retourne `None` si le fichier n'est pas encore disponible.
    pub async fn audio_file_path(&self, pk: &str) -> Option<std::path::PathBuf> {
        match self.audio_cache.get(pk).await {
            Ok(path) => Some(path),
            Err(_) => None,
        }
    }
}

/// Statistiques du cache pour une source
#[derive(Debug, Clone)]
pub struct CacheStatistics {
    /// Nombre total de pistes connues
    pub total_tracks: usize,

    /// Nombre de pistes en cache
    pub cached_tracks: usize,

    /// ID de collection
    pub collection_id: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_track_metadata() {
        let metadata = TrackMetadata {
            original_uri: "http://example.com/track.flac".to_string(),
            cached_audio_pk: Some("abc123".to_string()),
            cached_cover_pk: Some("def456".to_string()),
        };

        assert_eq!(metadata.original_uri, "http://example.com/track.flac");
        assert_eq!(metadata.cached_audio_pk, Some("abc123".to_string()));
    }
}
