#[cfg(feature = "pmoserver")]
use crate::Cache;

/// Trait pour étendre un serveur HTTP avec des fonctionnalités de cache audio.
///
/// Ce trait permet à `pmoaudiocache` d'ajouter des méthodes d'extension sur des types
/// de serveurs externes (comme `pmoserver::Server`) sans que ces crates dépendent de `pmoaudiocache`.
#[cfg(feature = "pmoserver")]
pub trait AudioCacheExt {
    /// Initialise le cache audio et enregistre les routes HTTP.
    ///
    /// # Arguments
    ///
    /// * `cache_dir` - Répertoire de stockage du cache
    /// * `limit` - Limite de taille du cache (en nombre de pistes)
    ///
    /// # Returns
    ///
    /// * `Arc<Cache>` - Instance partagée du cache
    async fn init_audio_cache(&mut self, cache_dir: &str, limit: usize) -> anyhow::Result<std::sync::Arc<Cache>>;

    /// Initialise le cache audio avec la configuration par défaut.
    ///
    /// Utilise automatiquement les paramètres de `pmoconfig::Config`.
    async fn init_audio_cache_configured(&mut self) -> anyhow::Result<std::sync::Arc<Cache>>;
}
