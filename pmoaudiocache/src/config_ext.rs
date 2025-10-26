//! Extension pour intégrer le cache audio dans pmoconfig
//!
//! Ce module fournit le trait `AudioCacheConfigExt` qui permet d'ajouter facilement
//! des méthodes de gestion du cache audio à pmoconfig::Config.

use anyhow::Result;
use pmocache::CacheConfigExt;
use pmoconfig::Config;
use std::sync::Arc;

const DEFAULT_AUDIO_CACHE_DIR: &str = "cache_audio";
const DEFAULT_AUDIO_CACHE_SIZE: usize = 500;

/// Trait d'extension pour gérer le cache audio dans pmoconfig
///
/// Ce trait étend `pmoconfig::Config` avec des méthodes spécifiques
/// au cache audio avec conversion FLAC.
///
/// # Exemple
///
/// ```rust,ignore
/// use pmoconfig::get_config;
/// use pmoaudiocache::AudioCacheConfigExt;
///
/// let config = get_config();
/// let cache = config.create_audio_cache()?;
///
/// // Utiliser le cache
/// let pk = cache.add_from_url("http://example.com/track.mp3", Some("album:123")).await?;
/// ```
pub trait AudioCacheConfigExt {
    /// Récupère le répertoire du cache audio
    ///
    /// # Returns
    ///
    /// Le chemin absolu du répertoire du cache audio (default: "cache_audio")
    fn get_audiocache_dir(&self) -> Result<String>;

    /// Définit le répertoire du cache audio
    ///
    /// # Arguments
    ///
    /// * `directory` - Chemin du répertoire (absolu ou relatif au config_dir)
    fn set_audiocache_dir(&self, directory: String) -> Result<()>;

    /// Récupère la taille maximale du cache audio
    ///
    /// # Returns
    ///
    /// Le nombre maximal de pistes audio dans le cache (default: 500)
    fn get_audiocache_size(&self) -> Result<usize>;

    /// Définit la taille maximale du cache audio
    ///
    /// # Arguments
    ///
    /// * `size` - Nombre maximal de pistes audio
    fn set_audiocache_size(&self, size: usize) -> Result<()>;

    /// Crée une instance du cache audio configurée avec conversion FLAC
    ///
    /// Cette méthode factory crée un cache audio en utilisant les paramètres
    /// de configuration (répertoire et taille) et active la conversion FLAC
    /// automatique pour tous les fichiers audio téléchargés.
    ///
    /// # Returns
    ///
    /// Une instance Arc du cache audio configuré
    ///
    /// # Exemple
    ///
    /// ```rust,ignore
    /// use pmoconfig::get_config;
    /// use pmoaudiocache::AudioCacheConfigExt;
    ///
    /// let config = get_config();
    /// let cache = config.create_audio_cache()?;
    ///
    /// // Le cache est prêt à être utilisé avec conversion FLAC automatique
    /// ```
    fn create_audio_cache(&self) -> Result<Arc<crate::Cache>>;
}

impl AudioCacheConfigExt for Config {
    fn get_audiocache_dir(&self) -> Result<String> {
        self.get_cache_dir("audio_cache", DEFAULT_AUDIO_CACHE_DIR)
    }

    fn set_audiocache_dir(&self, directory: String) -> Result<()> {
        self.set_cache_dir("audio_cache", directory)
    }

    fn get_audiocache_size(&self) -> Result<usize> {
        self.get_cache_size("audio_cache", DEFAULT_AUDIO_CACHE_SIZE)
    }

    fn set_audiocache_size(&self, size: usize) -> Result<()> {
        self.set_cache_size("audio_cache", size)
    }

    fn create_audio_cache(&self) -> Result<Arc<crate::Cache>> {
        let dir = self.get_audiocache_dir()?;
        let size = self.get_audiocache_size()?;
        Ok(Arc::new(crate::cache::new_cache(&dir, size)?))
    }
}
