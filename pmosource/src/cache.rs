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
use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::AsyncRead;
use tokio::sync::RwLock;

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
            if let Some(ref pk) = metadata.cached_audio_pk {
                #[cfg(feature = "server")]
                {
                    let url = pmoupnp::cache_registry::build_audio_url(pk, Some("stream"))
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
            if let Some(ref pk) = metadata.cached_audio_pk {
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
    pub fn cover_url(&self, pk: &str, size: Option<usize>) -> Result<String> {
        #[cfg(feature = "server")]
        {
            pmoupnp::cache_registry::build_cover_url(pk, size)
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
            .add_from_reader(source_uri, reader, length, Some(&self.collection_id))
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
