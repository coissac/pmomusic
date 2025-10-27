//! PlaylistTrack : résultat d'un pop() avec helpers pour accéder au cache

use crate::Result;
use pmocache::cache_trait::FileCache;
use pmoaudiocache::AudioMetadataExt;
use std::path::PathBuf;

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
    /// **Important** : Cette méthode récupère TOUTES les métadonnées de la base de données.
    /// Si vous n'avez besoin que d'un seul champ (ex: titre), utilisez plutôt les méthodes
    /// légères `title()`, `artist()`, etc. qui utilisent `get_a_metadata()`.
    ///
    /// # Exemples
    ///
    /// ```no_run
    /// # use pmoplaylist::*;
    /// # async fn example(track: PlaylistTrack) -> Result<()> {
    /// // ✅ BON : Si vous avez besoin de plusieurs champs
    /// let metadata = track.metadata().await?;
    /// let title = metadata.title.as_deref().unwrap_or("Unknown");
    /// let artist = metadata.artist.as_deref().unwrap_or("Unknown");
    /// let album = metadata.album.as_deref().unwrap_or("Unknown");
    ///
    /// // ✅ MIEUX : Si vous n'avez besoin que d'un seul champ (plus léger)
    /// let title = track.title().await?.unwrap_or_else(|| "Unknown".to_string());
    /// # Ok(())
    /// # }
    /// ```
    pub async fn metadata(&self) -> Result<pmoaudiocache::AudioMetadata> {
        let cache = crate::manager::audio_cache()?;
        pmoaudiocache::get_metadata(&*cache, &self.cache_pk)
            .map_err(|e| crate::Error::CacheError(e.to_string()))
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
