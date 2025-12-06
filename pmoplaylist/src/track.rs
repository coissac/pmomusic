//! PlaylistTrack : résultat d'un pop() avec helpers pour accéder au cache

use crate::Result;
use pmoaudiocache::{AudioMetadataExt, AudioTrackMetadataExt};
use pmocache::cache_trait::FileCache;
use pmometadata::TrackMetadata;
use std::{path::PathBuf, sync::Arc};
use tokio::sync::RwLock;

/// Un morceau récupéré depuis une playlist
///
/// Wrapper minimal autour d'un `cache_pk` qui délègue toutes les opérations
/// au système de cache (pmoaudiocache). Aucune métadonnée n'est stockée ici.
///
/// # Exemples
///
/// ```no_run
/// # use pmoplaylist::*;
/// # async fn example(track: PlaylistTrack) -> Result<()> {
/// // Accès à la clé cache
/// let pk = track.cache_pk();
///
/// // Récupérer les métadonnées (1 seul I/O)
/// let metadata = track.metadata().await?;
/// println!("Titre: {:?}", metadata.title);
/// println!("Artiste: {:?}", metadata.artist);
/// println!("Durée: {:?}s", metadata.duration_secs);
///
/// // Récupérer le chemin du fichier pour streaming
/// let path = track.file_path()?;
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone)]
pub struct PlaylistTrack {
    cache_pk: String,
}

impl PlaylistTrack {
    /// Crée un nouveau track
    pub(crate) fn new(cache_pk: String) -> Self {
        Self { cache_pk }
    }

    /// Retourne la clé primaire dans le cache audio
    pub fn cache_pk(&self) -> &str {
        &self.cache_pk
    }

    /// Récupère le chemin du fichier audio depuis le cache
    ///
    /// Délègue directement à `FileCache::file_path()`. La validation de
    /// l'existence du fichier est déjà faite par `ReadHandle::pop()`.
    ///
    /// # Note
    ///
    /// Cette méthode ne vérifie PAS l'existence du fichier. Pour valider
    /// avant de récupérer le chemin, utilisez `cache.is_valid_pk()`.
    pub fn file_path(&self) -> Result<PathBuf> {
        let cache = crate::manager::audio_cache()?;
        Ok(cache.file_path(&self.cache_pk))
    }

    /// Récupère les métadonnées audio complètes depuis le cache
    ///
    /// Retourne une instance de TrackMetadata pour ce morceau
    ///
    /// Cette méthode fournit un accès unifié aux métadonnées via le trait `TrackMetadata`.
    /// L'instance retournée implémente toutes les méthodes du trait et permet un accès
    /// asynchrone thread-safe aux métadonnées via RwLock.
    ///
    /// # Exemples
    ///
    /// ```no_run
    /// # use pmoplaylist::*;
    /// # async fn example(track: PlaylistTrack) -> Result<()> {
    /// // Obtenir l'instance TrackMetadata
    /// let metadata = track.track_metadata()?;
    ///
    /// // Accéder aux métadonnées via le trait
    /// let title = metadata.read().await.get_title().await?;
    /// let artist = metadata.read().await.get_artist().await?;
    ///
    /// println!("Titre: {:?}", title);
    /// println!("Artiste: {:?}", artist);
    /// # Ok(())
    /// # }
    /// ```
    pub fn track_metadata(&self) -> Result<Arc<RwLock<dyn TrackMetadata>>> {
        let cache = crate::manager::audio_cache()?;
        Ok(cache.track_metadata(&self.cache_pk))
    }

    /// Récupère uniquement le titre du morceau (méthode légère)
    ///
    /// Utilise l'extension trait `AudioMetadataExt` pour récupérer qu'une seule valeur
    /// de la base de données au lieu de toutes les métadonnées.
    ///
    /// **Beaucoup plus rapide** que `metadata().await?.title` si vous n'avez
    /// besoin que du titre.
    pub async fn title(&self) -> Result<Option<String>> {
        let cache = crate::manager::audio_cache()?;
        cache
            .get_title(&self.cache_pk)
            .await
            .map_err(|e| crate::Error::CacheError(e.to_string()))
    }

    /// Récupère uniquement l'artiste du morceau (méthode légère)
    ///
    /// Utilise l'extension trait `AudioMetadataExt`.
    pub async fn artist(&self) -> Result<Option<String>> {
        let cache = crate::manager::audio_cache()?;
        cache
            .get_artist(&self.cache_pk)
            .await
            .map_err(|e| crate::Error::CacheError(e.to_string()))
    }

    /// Récupère uniquement l'album du morceau (méthode légère)
    ///
    /// Utilise l'extension trait `AudioMetadataExt`.
    pub async fn album(&self) -> Result<Option<String>> {
        let cache = crate::manager::audio_cache()?;
        cache
            .get_album(&self.cache_pk)
            .await
            .map_err(|e| crate::Error::CacheError(e.to_string()))
    }

    /// Récupère uniquement la durée en secondes (méthode légère)
    ///
    /// Utilise l'extension trait `AudioMetadataExt`.
    pub async fn duration_secs(&self) -> Result<Option<u64>> {
        let cache = crate::manager::audio_cache()?;
        match cache
            .get_duration_secs(&self.cache_pk)
            .await
            .map_err(|e| crate::Error::CacheError(e.to_string()))?
        {
            Some(duration) => Ok(Some(duration as u64)),
            None => Ok(None),
        }
    }
}
