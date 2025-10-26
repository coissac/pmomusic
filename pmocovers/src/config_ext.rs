//! Extension pour intégrer le cache de couvertures dans pmoconfig
//!
//! Ce module fournit le trait `CoverCacheConfigExt` qui permet d'ajouter facilement
//! des méthodes de gestion du cache de couvertures à pmoconfig::Config.

use anyhow::Result;
use pmocache::CacheConfigExt;
use pmoconfig::Config;
use std::sync::Arc;

const DEFAULT_COVER_CACHE_DIR: &str = "cache_covers";
const DEFAULT_COVER_CACHE_SIZE: usize = 2000;

/// Trait d'extension pour gérer le cache de couvertures dans pmoconfig
///
/// Ce trait étend `pmoconfig::Config` avec des méthodes spécifiques
/// au cache de couvertures avec conversion WebP.
///
/// # Exemple
///
/// ```rust,ignore
/// use pmoconfig::get_config;
/// use pmocovers::CoverCacheConfigExt;
///
/// let config = get_config();
/// let cache = config.create_cover_cache()?;
///
/// // Utiliser le cache
/// let pk = cache.add_from_url("http://example.com/cover.jpg", Some("album:123")).await?;
/// ```
pub trait CoverCacheConfigExt {
    /// Récupère le répertoire du cache de couvertures
    ///
    /// # Returns
    ///
    /// Le chemin absolu du répertoire du cache de couvertures (default: "cache_covers")
    fn get_covers_dir(&self) -> Result<String>;

    /// Définit le répertoire du cache de couvertures
    ///
    /// # Arguments
    ///
    /// * `directory` - Chemin du répertoire (absolu ou relatif au config_dir)
    fn set_covers_dir(&self, directory: String) -> Result<()>;

    /// Récupère la taille maximale du cache de couvertures
    ///
    /// # Returns
    ///
    /// Le nombre maximal d'images dans le cache (default: 2000)
    fn get_covers_size(&self) -> Result<usize>;

    /// Définit la taille maximale du cache de couvertures
    ///
    /// # Arguments
    ///
    /// * `size` - Nombre maximal d'images
    fn set_covers_size(&self, size: usize) -> Result<()>;

    /// Crée une instance du cache de couvertures configurée avec conversion WebP
    ///
    /// Cette méthode factory crée un cache de couvertures en utilisant les paramètres
    /// de configuration (répertoire et taille) et active la conversion WebP
    /// automatique pour toutes les images téléchargées.
    ///
    /// # Returns
    ///
    /// Une instance Arc du cache de couvertures configuré
    ///
    /// # Exemple
    ///
    /// ```rust,ignore
    /// use pmoconfig::get_config;
    /// use pmocovers::CoverCacheConfigExt;
    ///
    /// let config = get_config();
    /// let cache = config.create_cover_cache()?;
    ///
    /// // Le cache est prêt à être utilisé avec conversion WebP automatique
    /// ```
    fn create_cover_cache(&self) -> Result<Arc<crate::Cache>>;
}

impl CoverCacheConfigExt for Config {
    fn get_covers_dir(&self) -> Result<String> {
        self.get_cache_dir("cover_cache", DEFAULT_COVER_CACHE_DIR)
    }

    fn set_covers_dir(&self, directory: String) -> Result<()> {
        self.set_cache_dir("cover_cache", directory)
    }

    fn get_covers_size(&self) -> Result<usize> {
        self.get_cache_size("cover_cache", DEFAULT_COVER_CACHE_SIZE)
    }

    fn set_covers_size(&self, size: usize) -> Result<()> {
        self.set_cache_size("cover_cache", size)
    }

    fn create_cover_cache(&self) -> Result<Arc<crate::Cache>> {
        let dir = self.get_covers_dir()?;
        let size = self.get_covers_size()?;
        Ok(Arc::new(crate::cache::new_cache(&dir, size)?))
    }
}
