//! Module de gestion du cache d'images avec conversion WebP
//!
//! Ce module étend le cache générique de `pmocache` avec des fonctionnalités
//! spécifiques aux images : conversion WebP automatique lors du téléchargement.

use anyhow::Result;
use pmocache::{CacheConfig, StreamTransformer};
use std::sync::Arc;

/// Configuration pour le cache de couvertures.
///
/// Spécifie l'extension finale (`webp`), le type logique exposé (`image`) et
/// le nom de cache (`covers`) utilisés par les routes générées par `pmocache`.
pub struct CoversConfig;

impl CacheConfig for CoversConfig {
    fn file_extension() -> &'static str {
        "webp"
    }

    fn cache_type() -> &'static str {
        "image"
    }

    fn cache_name() -> &'static str {
        "covers"
    }
}

/// Type alias pour le cache de couvertures avec conversion WebP.
pub type Cache = pmocache::Cache<CoversConfig>;

/// Créateur de transformer WebP.
///
/// Convertit automatiquement toute image téléchargée en format WebP
/// avant de l'écrire sur disque. Les octets d'entrée sont lus en mémoire,
/// décodés via `image`, ré-encodés en WebP puis persistés. La progression
/// est reportée pour que le cache puisse suivre la taille transformée.
fn create_webp_transformer() -> StreamTransformer {
    Box::new(|mut input, mut file, context| {
        Box::pin(async move {
            // Télécharger tout en mémoire
            let bytes = input.bytes().await?;

            // Convertir en WebP
            let img = image::load_from_memory(&bytes)
                .map_err(|e| format!("Image decode error: {}", e))?;
            let webp_data =
                crate::webp::encode_webp(&img).map_err(|e| format!("WebP encode error: {}", e))?;

            // Écrire et mettre à jour la progression
            use tokio::io::AsyncWriteExt;
            file.write_all(&webp_data)
                .await
                .map_err(|e| e.to_string())?;
            file.flush().await.map_err(|e| e.to_string())?;
            context.report_progress(webp_data.len() as u64);

            Ok(())
        })
    })
}

/// Crée un cache de couvertures avec conversion WebP automatique.
///
/// # Arguments
///
/// * `dir` - Répertoire de stockage du cache
/// * `limit` - Limite de taille du cache (nombre d'images)
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
/// let cache = cache::new_cache("./cache", 1000).unwrap();
/// ```
pub fn new_cache(dir: &str, limit: usize) -> Result<Cache> {
    let transformer_factory = Arc::new(|| create_webp_transformer());
    Cache::with_transformer(dir, limit, Some(transformer_factory))
}

/// Crée un cache de couvertures et lance une consolidation en arrière-plan.
///
/// Idéal pour un démarrage de service : la consolidation supprime les fichiers
/// incomplets et recalcule les markers `.complete` au besoin avant d'accepter
/// des requêtes.
pub async fn new_cache_with_consolidation(dir: &str, limit: usize) -> Result<Arc<Cache>> {
    let cache = Arc::new(new_cache(dir, limit)?);
    let cache_clone = cache.clone();
    tokio::spawn(async move {
        if let Err(e) = cache_clone.consolidate().await {
            tracing::warn!("Failed to consolidate cover cache on startup: {}", e);
        } else {
            tracing::info!("Cover cache consolidated successfully on startup");
        }
    });
    Ok(cache)
}
