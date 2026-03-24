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
    Ok(pmocache::covers_absolute_url_for(
        pk,
        size.map(|s| s.to_string()).as_deref(),
    ))
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
    let cache = get_audio_cache().ok_or_else(|| anyhow::anyhow!("No registered audio cache"))?;
    Ok(cache.absolute_url_for(pk, param))
}
