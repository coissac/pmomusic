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

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use pmocovers::Cache as CoverCache;
use pmoaudiocache::{Cache as AudioCache, AudioMetadata};
use crate::{MusicSourceError, Result, CacheStatus};

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

    /// URL de base du serveur
    cache_base_url: String,

    /// ID de collection pour cette source (ex: "radio-paradise", "qobuz")
    collection_id: String,

    /// Référence au cache de couvertures centralisé
    cover_cache: Arc<CoverCache>,

    /// Référence au cache audio centralisé
    audio_cache: Arc<AudioCache>,
}

impl SourceCacheManager {
    /// Créer un nouveau manager
    ///
    /// # Arguments
    ///
    /// * `cache_base_url` - URL de base du serveur
    /// * `collection_id` - ID de collection (source ID)
    /// * `cover_cache` - Cache de couvertures centralisé
    /// * `audio_cache` - Cache audio centralisé
    pub fn new(
        cache_base_url: String,
        collection_id: String,
        cover_cache: Arc<CoverCache>,
        audio_cache: Arc<AudioCache>,
    ) -> Self {
        Self {
            track_cache: RwLock::new(HashMap::new()),
            cache_base_url,
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
                return Ok(format!("{}/audio/tracks/{}/stream", self.cache_base_url, pk));
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
    pub fn cover_url(&self, pk: &str, size: Option<usize>) -> String {
        if let Some(s) = size {
            format!("{}/covers/images/{}/{}", self.cache_base_url, pk, s)
        } else {
            format!("{}/covers/images/{}", self.cache_base_url, pk)
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
    pub async fn cache_audio(&self, url: &str, _metadata: Option<AudioMetadata>)
        -> Result<String> {
        // Note: Les métadonnées seront extraites automatiquement par le cache audio
        // lors de la conversion FLAC
        let pk = self.audio_cache
            .add_from_url(url, Some(&self.collection_id))
            .await
            .map_err(|e| MusicSourceError::CacheError(e.to_string()))?;
        Ok(pk)
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
        let cached_count = cache.values()
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
