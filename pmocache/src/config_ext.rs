//! Extension pour intégrer la gestion des caches dans pmoconfig
//!
//! Ce module fournit le trait `CacheConfigExt` qui permet d'ajouter facilement
//! des méthodes de gestion de cache générique à pmoconfig::Config.
//!
//! Il propose également un macro `impl_cache_config_ext!` pour simplifier
//! l'implémentation de traits d'extension spécialisés (audio, covers, etc.).

use anyhow::Result;
use pmoconfig::Config;
use serde_yaml::{Number, Value};
use std::sync::Arc;

/// Trait d'extension pour ajouter la gestion des caches à pmoconfig
///
/// Ce trait étend `pmoconfig::Config` avec des méthodes génériques pour gérer
/// n'importe quel type de cache (audio, images, etc.).
///
/// # Exemple
///
/// ```rust,ignore
/// use pmoconfig::get_config;
/// use pmocache::{CacheConfigExt, AudioConfig};
///
/// let config = get_config();
/// let cache_dir = config.get_cache_dir("audio_cache", "cache_audio")?;
/// let cache_size = config.get_cache_size("audio_cache", 500)?;
/// ```
pub trait CacheConfigExt {
    /// Récupère le répertoire d'un cache
    ///
    /// # Arguments
    ///
    /// * `cache_type` - Type de cache (ex: "audio_cache", "cover_cache")
    /// * `default` - Nom de répertoire par défaut si non configuré
    ///
    /// # Returns
    ///
    /// Le chemin absolu du répertoire du cache
    fn get_cache_dir(&self, cache_type: &str, default: &str) -> Result<String>;

    /// Définit le répertoire d'un cache
    ///
    /// # Arguments
    ///
    /// * `cache_type` - Type de cache (ex: "audio_cache", "cover_cache")
    /// * `directory` - Chemin du répertoire (absolu ou relatif au config_dir)
    fn set_cache_dir(&self, cache_type: &str, directory: String) -> Result<()>;

    /// Récupère la taille maximale d'un cache
    ///
    /// # Arguments
    ///
    /// * `cache_type` - Type de cache (ex: "audio_cache", "cover_cache")
    /// * `default` - Taille par défaut si non configurée
    ///
    /// # Returns
    ///
    /// Le nombre maximal d'éléments dans le cache
    fn get_cache_size(&self, cache_type: &str, default: usize) -> Result<usize>;

    /// Définit la taille maximale d'un cache
    ///
    /// # Arguments
    ///
    /// * `cache_type` - Type de cache (ex: "audio_cache", "cover_cache")
    /// * `size` - Nombre maximal d'éléments
    fn set_cache_size(&self, cache_type: &str, size: usize) -> Result<()>;

    /// Crée une instance de cache générique configurée
    ///
    /// Cette méthode factory crée un cache en utilisant les paramètres
    /// de configuration (répertoire et taille).
    ///
    /// # Arguments
    ///
    /// * `cache_type` - Type de cache (ex: "audio_cache", "cover_cache")
    /// * `default_dir` - Répertoire par défaut
    /// * `default_size` - Taille par défaut
    ///
    /// # Returns
    ///
    /// Une instance Arc du cache configuré
    ///
    /// # Exemple
    ///
    /// ```rust,ignore
    /// use pmoconfig::get_config;
    /// use pmocache::{CacheConfigExt, AudioConfig};
    ///
    /// let config = get_config();
    /// let cache = config.create_cache::<AudioConfig>("audio_cache", "cache_audio", 500)?;
    /// ```
    fn create_cache<C: crate::CacheConfig>(
        &self,
        cache_type: &str,
        default_dir: &str,
        default_size: usize,
    ) -> Result<Arc<crate::Cache<C>>>;
}

impl CacheConfigExt for Config {
    fn get_cache_dir(&self, cache_type: &str, default: &str) -> Result<String> {
        self.get_managed_dir(&["host", cache_type, "directory"], default)
    }

    fn set_cache_dir(&self, cache_type: &str, directory: String) -> Result<()> {
        self.set_managed_dir(&["host", cache_type, "directory"], directory)
    }

    fn get_cache_size(&self, cache_type: &str, default: usize) -> Result<usize> {
        match self.get_value(&["host", cache_type, "size"])? {
            Value::Number(n) if n.is_i64() => Ok(n.as_i64().unwrap() as usize),
            Value::Number(n) if n.is_u64() => Ok(n.as_u64().unwrap() as usize),
            _ => Ok(default),
        }
    }

    fn set_cache_size(&self, cache_type: &str, size: usize) -> Result<()> {
        let n = Number::from(size);
        self.set_value(&["host", cache_type, "size"], Value::Number(n))
    }

    fn create_cache<C: crate::CacheConfig>(
        &self,
        cache_type: &str,
        default_dir: &str,
        default_size: usize,
    ) -> Result<Arc<crate::Cache<C>>> {
        let dir = self.get_cache_dir(cache_type, default_dir)?;
        let size = self.get_cache_size(cache_type, default_size)?;
        Ok(Arc::new(crate::Cache::<C>::new(&dir, size)?))
    }
}

/// Macro pour simplifier l'implémentation de traits d'extension de cache spécialisés
///
/// Ce macro génère automatiquement un trait d'extension pour `pmoconfig::Config`
/// avec des méthodes spécifiques à un type de cache (audio, covers, etc.).
///
/// # Arguments
///
/// * `trait_name` - Nom du trait à générer (ex: `AudioCacheConfigExt`)
/// * `cache_type` - Type de cache dans la config (ex: `"audio_cache"`)
/// * `default_dir` - Répertoire par défaut (ex: `"cache_audio"`)
/// * `default_size` - Taille par défaut (ex: `500`)
/// * `cache_struct` - Type du cache (ex: `crate::Cache`)
/// * `constructor` - Expression pour construire le cache (ex: `crate::cache::new_cache(&dir, size)`)
///
/// # Exemple
///
/// ```rust,ignore
/// use pmocache::impl_cache_config_ext;
///
/// impl_cache_config_ext! {
///     AudioCacheConfigExt,
///     "audio_cache",
///     "cache_audio",
///     500,
///     crate::Cache,
///     |dir, size| crate::cache::new_cache(dir, size)
/// }
/// ```
///
/// Cela génère un trait avec les méthodes :
/// - `get_audiocache_dir()` / `set_audiocache_dir()`
/// - `get_audiocache_size()` / `set_audiocache_size()`
/// - `create_audio_cache()`
#[macro_export]
macro_rules! impl_cache_config_ext {
    (
        $trait_name:ident,
        $cache_type:expr,
        $default_dir:expr,
        $default_size:expr,
        $cache_type_struct:ty,
        $constructor:expr
    ) => {
        pub trait $trait_name {
            /// Récupère le répertoire du cache
            fn get_cache_dir_ext(&self) -> anyhow::Result<String>;

            /// Définit le répertoire du cache
            fn set_cache_dir_ext(&self, directory: String) -> anyhow::Result<()>;

            /// Récupère la taille maximale du cache
            fn get_cache_size_ext(&self) -> anyhow::Result<usize>;

            /// Définit la taille maximale du cache
            fn set_cache_size_ext(&self, size: usize) -> anyhow::Result<()>;

            /// Crée une instance du cache configurée
            fn create_cache_ext(&self) -> anyhow::Result<std::sync::Arc<$cache_type_struct>>;
        }

        impl $trait_name for pmoconfig::Config {
            fn get_cache_dir_ext(&self) -> anyhow::Result<String> {
                use $crate::CacheConfigExt;
                self.get_cache_dir($cache_type, $default_dir)
            }

            fn set_cache_dir_ext(&self, directory: String) -> anyhow::Result<()> {
                use $crate::CacheConfigExt;
                self.set_cache_dir($cache_type, directory)
            }

            fn get_cache_size_ext(&self) -> anyhow::Result<usize> {
                use $crate::CacheConfigExt;
                self.get_cache_size($cache_type, $default_size)
            }

            fn set_cache_size_ext(&self, size: usize) -> anyhow::Result<()> {
                use $crate::CacheConfigExt;
                self.set_cache_size($cache_type, size)
            }

            fn create_cache_ext(&self) -> anyhow::Result<std::sync::Arc<$cache_type_struct>> {
                let dir = self.get_cache_dir_ext()?;
                let size = self.get_cache_size_ext()?;
                let constructor = $constructor;
                Ok(std::sync::Arc::new(constructor(&dir, size)?))
            }
        }
    };
}
