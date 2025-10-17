//! Module de gestion du cache d'images avec conversion WebP
//!
//! Ce module étend le cache générique de `pmocache` avec des fonctionnalités
//! spécifiques aux images : conversion WebP automatique lors du téléchargement.

use anyhow::Result;
use pmocache::{CacheConfig, StreamTransformer};
use std::sync::Arc;

/// Configuration pour le cache de couvertures
pub struct CoversConfig;

impl CacheConfig for CoversConfig {
    fn file_extension() -> &'static str {
        "webp"
    }

    fn table_name() -> &'static str {
        "covers"
    }

    fn cache_type() -> &'static str {
        "image"
    }

    fn cache_name() -> &'static str {
        "covers"
    }
}

/// Type alias pour le cache de couvertures avec conversion WebP
pub type Cache = pmocache::Cache<CoversConfig>;

/// Créateur de transformer WebP
///
/// Convertit automatiquement toute image téléchargée en format WebP
fn create_webp_transformer() -> StreamTransformer {
    Box::new(|response, mut file, progress| {
        Box::pin(async move {
            // Télécharger tout en mémoire
            let bytes = response.bytes().await.map_err(|e| e.to_string())?;

            // Convertir en WebP
            let img = image::load_from_memory(&bytes)
                .map_err(|e| format!("Image decode error: {}", e))?;
            let webp_data = crate::webp::encode_webp(&img)
                .map_err(|e| format!("WebP encode error: {}", e))?;

            // Écrire et mettre à jour la progression
            use tokio::io::AsyncWriteExt;
            file.write_all(&webp_data).await.map_err(|e| e.to_string())?;
            file.flush().await.map_err(|e| e.to_string())?;
            progress(webp_data.len() as u64);

            Ok(())
        })
    })
}

/// Crée un cache de couvertures avec conversion WebP automatique
///
/// # Arguments
///
/// * `dir` - Répertoire de stockage du cache
/// * `limit` - Limite de taille du cache (nombre d'images)
/// * `base_url` - URL de base pour la génération d'URLs
///
/// # Returns
///
/// Instance du cache configurée pour la conversion WebP automatique
///
/// # Exemple
///
/// ```rust,no_run
/// use pmocovers::cache;
///
/// let cache = cache::new_cache("./cache", 1000, "http://localhost:8080").unwrap();
/// ```
pub fn new_cache(dir: &str, limit: usize, base_url: &str) -> Result<Cache> {
    let transformer_factory = Arc::new(|| create_webp_transformer());
    Cache::with_transformer(dir, limit, base_url, Some(transformer_factory))
}
