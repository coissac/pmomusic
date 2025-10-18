//! Registre centralisé des caches pour le serveur UPnP
//!
//! Ce module gère les caches partagés entre toutes les sources musicales :
//! - Cache de couvertures d'albums (WebP)
//! - Cache de pistes audio (FLAC)
//!
//! Les caches supportent les collections, permettant à chaque source
//! d'avoir sa propre collection dans le cache partagé.

use std::sync::Arc;
use once_cell::sync::Lazy;
use std::sync::RwLock;
use pmocovers::Cache as CoverCache;
use pmoaudiocache::Cache as AudioCache;

/// Registre global des caches
///
/// Contient les instances partagées des caches de couvertures et audio.
/// Ces caches sont uniques et partagés entre toutes les sources musicales.
pub struct CacheRegistry {
    /// URL de base du serveur (ex: "http://localhost:8080")
    base_url: Option<String>,

    /// Cache de couvertures (WebP)
    cover_cache: Option<Arc<CoverCache>>,

    /// Cache audio (FLAC)
    audio_cache: Option<Arc<AudioCache>>,
}

impl CacheRegistry {
    /// Créer un nouveau registre vide
    pub fn new() -> Self {
        Self {
            base_url: None,
            cover_cache: None,
            audio_cache: None,
        }
    }

    /// Définir l'URL de base du serveur
    pub fn set_base_url(&mut self, url: String) {
        self.base_url = Some(url);
    }

    /// Récupérer l'URL de base du serveur
    pub fn base_url(&self) -> Option<&str> {
        self.base_url.as_deref()
    }

    /// Enregistrer le cache de couvertures
    pub fn set_cover_cache(&mut self, cache: Arc<CoverCache>) {
        self.cover_cache = Some(cache);
    }

    /// Récupérer le cache de couvertures
    pub fn cover_cache(&self) -> Option<Arc<CoverCache>> {
        self.cover_cache.clone()
    }

    /// Enregistrer le cache audio
    pub fn set_audio_cache(&mut self, cache: Arc<AudioCache>) {
        self.audio_cache = Some(cache);
    }

    /// Récupérer le cache audio
    pub fn audio_cache(&self) -> Option<Arc<AudioCache>> {
        self.audio_cache.clone()
    }

    /// Construit l'URL complète pour une couverture
    ///
    /// # Arguments
    ///
    /// * `pk` - Clé primaire de la couverture
    /// * `size` - Taille optionnelle de l'image
    ///
    /// # Returns
    ///
    /// URL complète (ex: "http://localhost:8080/covers/images/abc123/300")
    pub fn build_cover_url(&self, pk: &str, size: Option<usize>) -> anyhow::Result<String> {
        let base_url = self.base_url
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Base URL not set in CacheRegistry"))?;
        let route = pmocovers::cache::route_for(pk, size);
        Ok(format!("{}{}", base_url, route))
    }

    /// Construit l'URL complète pour une piste audio
    ///
    /// # Arguments
    ///
    /// * `pk` - Clé primaire de la piste
    /// * `param` - Paramètre optionnel (ex: "orig", "128k")
    ///
    /// # Returns
    ///
    /// URL complète (ex: "http://localhost:8080/audio/tracks/abc123/orig")
    pub fn build_audio_url(&self, pk: &str, param: Option<&str>) -> anyhow::Result<String> {
        let base_url = self.base_url
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Base URL not set in CacheRegistry"))?;
        let route = pmoaudiocache::cache::route_for(pk, param);
        Ok(format!("{}{}", base_url, route))
    }
}

impl Default for CacheRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Registre global thread-safe
///
/// Utilise Lazy pour une initialisation paresseuse et RwLock pour le partage entre threads.
/// Permet aux handlers et aux sources d'accéder aux caches depuis n'importe où.
pub(crate) static CACHE_REGISTRY: Lazy<RwLock<CacheRegistry>> = Lazy::new(|| {
    RwLock::new(CacheRegistry::new())
});

/// Accès global au cache de couvertures
///
/// # Examples
///
/// ```rust,ignore
/// use pmoupnp::cache_registry::get_cover_cache;
///
/// if let Some(cache) = get_cover_cache() {
///     let pk = cache.add_from_url("http://example.com/cover.jpg").await?;
/// }
/// ```
pub fn get_cover_cache() -> Option<Arc<CoverCache>> {
    CACHE_REGISTRY.read().unwrap().cover_cache()
}

/// Accès global au cache audio
///
/// # Examples
///
/// ```rust,ignore
/// use pmoupnp::cache_registry::get_audio_cache;
///
/// if let Some(cache) = get_audio_cache() {
///     let (pk, _) = cache.add_from_url("http://example.com/track.flac", None).await?;
/// }
/// ```
pub fn get_audio_cache() -> Option<Arc<AudioCache>> {
    CACHE_REGISTRY.read().unwrap().audio_cache()
}

/// Construit l'URL complète pour une couverture
///
/// Fonction globale qui utilise le registre de caches pour construire l'URL.
///
/// # Arguments
///
/// * `pk` - Clé primaire de la couverture
/// * `size` - Taille optionnelle de l'image
///
/// # Examples
///
/// ```rust,ignore
/// use pmoupnp::cache_registry::build_cover_url;
///
/// let url = build_cover_url("abc123", Some(300))?;
/// // url = "http://localhost:8080/covers/images/abc123/300"
/// ```
pub fn build_cover_url(pk: &str, size: Option<usize>) -> anyhow::Result<String> {
    CACHE_REGISTRY.read().unwrap().build_cover_url(pk, size)
}

/// Construit l'URL complète pour une piste audio
///
/// Fonction globale qui utilise le registre de caches pour construire l'URL.
///
/// # Arguments
///
/// * `pk` - Clé primaire de la piste
/// * `param` - Paramètre optionnel (ex: "orig", "128k")
///
/// # Examples
///
/// ```rust,ignore
/// use pmoupnp::cache_registry::build_audio_url;
///
/// let url = build_audio_url("abc123", Some("orig"))?;
/// // url = "http://localhost:8080/audio/tracks/abc123/orig"
/// ```
pub fn build_audio_url(pk: &str, param: Option<&str>) -> anyhow::Result<String> {
    CACHE_REGISTRY.read().unwrap().build_audio_url(pk, param)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_registry_empty() {
        let registry = CacheRegistry::new();
        assert!(registry.cover_cache().is_none());
        assert!(registry.audio_cache().is_none());
    }
}
