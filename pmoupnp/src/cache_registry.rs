//! Registre centralisé des caches pour le serveur UPnP (couche de compatibilité)
//!
//! Ce module fournit une couche de compatibilité pour pmosource qui utilise
//! les singletons de pmoaudiocache et pmocovers pour accéder aux caches.

use pmoaudiocache::Cache as AudioCache;
use pmocache::FileCache;
use pmocovers::Cache as CoverCache;
use std::sync::Arc;

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
    pmocovers::get_cover_cache()
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
    pmoaudiocache::get_audio_cache()
}

/// Construit l'URL complète pour une couverture
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
    // Récupérer l'URL de base depuis la variable d'environnement ou une config
    let base_url =
        std::env::var("PMO_SERVER_URL").unwrap_or_else(|_| "http://localhost:8080".to_string());

    let cache = get_cover_cache().ok_or_else(|| anyhow::anyhow!("No registered cover cache"))?;

    let param = match size {
        Some(size_) => Some(size_.to_string()),
        None => None,
    };
    let route = cache.route_for(pk, param.as_deref());
    Ok(format!("{}{}", base_url, route))
}

/// Construit l'URL complète pour une piste audio
///
/// # Arguments
///
/// * `pk` - Clé primaire de la piste
/// * `param` - Paramètre optionnel (ex: "orig", "stream")
///
/// # Examples
///
/// ```rust,ignore
/// use pmoupnp::cache_registry::build_audio_url;
///
/// let url = build_audio_url("abc123", Some("stream"))?;
/// // url = "http://localhost:8080/audio/tracks/abc123/stream"
/// ```
pub fn build_audio_url(pk: &str, param: Option<&str>) -> anyhow::Result<String> {
    // Récupérer l'URL de base depuis la variable d'environnement ou une config
    let base_url =
        std::env::var("PMO_SERVER_URL").unwrap_or_else(|_| "http://localhost:8080".to_string());

    let cache = get_audio_cache().ok_or_else(|| anyhow::anyhow!("No registered audio cache"))?;

    let route = cache.route_for(pk, param);
    Ok(format!("{}{}", base_url, route))
}
