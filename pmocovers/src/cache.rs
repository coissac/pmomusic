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
    Ok(pmocache::Cache::with_consolidation(cache).await)
}

/// Détecte si un buffer contient un fichier WebP
///
/// Le format WebP commence par "RIFF" (4 octets), suivi de la taille (4 octets),
/// puis "WEBP" (4 octets).
fn is_webp_header(buf: &[u8]) -> bool {
    buf.len() >= 12 && &buf[0..4] == b"RIFF" && &buf[8..12] == b"WEBP"
}

/// Ajoute un fichier image local au cache
///
/// Les images WebP sont référencées sans copie (symlink/hardlink), les autres formats
/// sont convertis en WebP via le pipeline classique.
///
/// # Arguments
///
/// * `cache` - Instance du cache de couvertures
/// * `path` - Chemin vers le fichier image local
/// * `collection` - Collection optionnelle (ex: "album:xyz")
///
/// # Returns
///
/// Clé primaire (pk) de l'image ajoutée au cache
///
/// # Exemples
///
/// ```rust,no_run
/// use pmocovers::cache;
///
/// # async fn example() -> anyhow::Result<()> {
/// let cache = cache::new_cache("./covers", 1000)?;
/// let pk = cache::add_local_file(&cache, "/path/to/cover.webp", None).await?;
/// println!("Image ajoutée avec pk: {}", pk);
/// # Ok(())
/// # }
/// ```
pub async fn add_local_file(cache: &Cache, path: &str, collection: Option<&str>) -> Result<String> {
    use pmocache::download::read_exact_or_eof;
    use pmocache::pk_from_content_header;
    use serde_json::json;
    use tokio::io::AsyncSeekExt;

    let canonical_path = std::fs::canonicalize(path)?;
    let file_url = format!("file://{}", canonical_path.display());
    let length = tokio::fs::metadata(&canonical_path)
        .await
        .ok()
        .map(|m| m.len());
    let mut reader = tokio::fs::File::open(&canonical_path).await?;

    let header = read_exact_or_eof(&mut reader, 1024)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to read header bytes: {}", e))?;

    let pk_bytes = if header.len() >= 1024 {
        &header[512..]
    } else {
        &header[..]
    };
    let pk = pk_from_content_header(pk_bytes);

    // Si déjà en cache, incrémenter hit et retourner
    if cache.db.get(&pk, false).is_ok() {
        cache.db.update_hit(&pk)?;
        return Ok(pk);
    }

    // Si téléchargement en cours, attendre et retourner
    if let Some(download) = cache.get_download(&pk).await {
        if download.finished().await {
            cache.db.update_hit(&pk)?;
        }
        return Ok(pk);
    }

    let is_webp = is_webp_header(&header);
    if !is_webp {
        // Format non-WebP : passer par le pipeline de conversion
        reader
            .rewind()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to rewind local file: {}", e))?;

        return cache
            .add_from_reader_with_pk(Some(&file_url), reader, length, collection, Some(pk))
            .await;
    }

    // Format WebP : créer un lien sans copie (passthrough)
    let mut metadata = vec![
        ("local_passthrough".to_string(), json!(true)),
        (
            "local_source_path".to_string(),
            json!(canonical_path.to_string_lossy().to_string()),
        ),
    ];
    if let Some(len) = length {
        metadata.push(("source_size".to_string(), json!(len)));
    }

    cache
        .register_local_file_reference(
            &pk,
            &canonical_path,
            collection,
            Some(&file_url),
            Some(&metadata),
        )
        .await?;

    Ok(pk)
}
