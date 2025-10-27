//! Module de gestion du cache audio avec conversion FLAC
//!
//! Ce module étend le cache générique de `pmocache` avec des fonctionnalités
//! spécifiques aux fichiers audio : conversion FLAC automatique et stockage
//! des métadonnées en JSON dans la base de données.

use anyhow::Result;
use pmocache::CacheConfig;
use serde_json::Value;
use std::sync::Arc;

/// Configuration pour le cache audio
pub struct AudioConfig;

impl CacheConfig for AudioConfig {
    fn file_extension() -> &'static str {
        "flac"
    }

    fn cache_type() -> &'static str {
        "flac"
    }

    fn cache_name() -> &'static str {
        "audio"
    }

    fn default_param() -> &'static str {
        "orig"
    }
}

/// Type alias pour le cache audio avec conversion FLAC
pub type Cache = pmocache::Cache<AudioConfig>;

/// Crée un cache audio avec conversion FLAC automatique
///
/// # Arguments
///
/// * `dir` - Répertoire de stockage du cache
/// * `limit` - Limite de taille du cache (nombre de pistes)
///
/// # Returns
///
/// Instance du cache configurée pour la conversion FLAC automatique
///
/// # Exemple
///
/// ```rust,no_run
/// use pmoaudiocache::cache;
///
/// let cache = cache::new_cache("./audio_cache", 1000).unwrap();
/// ```
pub fn new_cache(dir: &str, limit: usize) -> Result<Cache> {
    let transformer_factory = Arc::new(|| crate::streaming::create_streaming_flac_transformer());
    Cache::with_transformer(dir, limit, Some(transformer_factory))
}

/// Ajoute une piste audio depuis une URL avec extraction et stockage des métadonnées
///
/// Cette fonction étend `add_from_url` du cache en ajoutant :
/// 1. Téléchargement et conversion FLAC (via transformer)
/// 2. Extraction et stockage des métadonnées en JSON dans la DB
///
/// # Arguments
///
/// * `cache` - Instance du cache
/// * `url` - URL du fichier audio
/// * `collection` - Collection optionnelle (ex: "pink_floyd:wish_you_were_here")
///
/// # Returns
///
/// Clé primaire (pk) du fichier ajouté
///
/// # Exemple
///
/// ```rust,no_run
/// use pmoaudiocache::cache;
///
/// # async fn example() -> anyhow::Result<()> {
/// let cache = cache::new_cache("./audio_cache", 1000)?;
/// let pk = cache::add_with_metadata_extraction(
///     &cache,
///     "https://example.com/track.flac",
///     Some("artist:album")
/// ).await?;
/// # Ok(())
/// # }
/// ```
pub async fn add_with_metadata_extraction(
    cache: &Cache,
    url: &str,
    collection: Option<&str>,
) -> Result<String> {
    // Ajouter au cache (déclenche le download et la conversion)
    let pk = cache.add_from_url(url, collection).await?;

    // Attendre que le fichier soit téléchargé et converti
    cache.wait_until_finished(&pk).await?;

    // Lire le fichier FLAC pour extraire les métadonnées
    let file_path = cache.get_file_path(&pk);
    let flac_bytes = tokio::fs::read(&file_path).await?;

    // Extraire les métadonnées
    let metadata = crate::metadata::AudioMetadata::from_bytes(&flac_bytes)?;
    let metadata_json: Value = serde_json::to_value(&metadata)
        .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
    // Stocker dans la DB
    cache
        .db
        .set_metadata(&pk, &metadata_json)
        .map_err(|e| anyhow::anyhow!("Database error: {}", e))?;

    // Mettre à jour la collection si les métadonnées en fournissent une
    if collection.is_none() {
        if let Some(auto_collection) = metadata.collection_key() {
            cache
                .db
                .add(&pk, None, Some(&auto_collection))
                .map_err(|e| anyhow::anyhow!("Database error: {}", e))?;
            cache.db.set_origin_url(&pk, url)?;
        }
    }

    Ok(pk)
}

/// Récupère les métadonnées audio d'un fichier en cache
///
/// # Arguments
///
/// * `cache` - Instance du cache
/// * `pk` - Clé primaire du fichier
///
/// # Returns
///
/// Les métadonnées audio désérialisées depuis le JSON stocké en DB
///
/// # Exemple
///
/// ```rust,no_run
/// use pmoaudiocache::cache;
///
/// # async fn example(cache: &pmoaudiocache::cache::Cache, pk: &str) -> anyhow::Result<()> {
/// let metadata = cache::get_metadata(cache, pk)?;
/// println!("Title: {:?}", metadata.title);
/// println!("Artist: {:?}", metadata.artist);
/// # Ok(())
/// # }
/// ```
pub fn get_metadata(cache: &Cache, pk: &str) -> Result<crate::metadata::AudioMetadata> {
    let metadata_json = cache
        .db
        .get_metadata_json(pk)
        .map_err(|e| anyhow::anyhow!("Database error: {}", e))?
        .ok_or_else(|| anyhow::anyhow!("No metadata found for pk: {}", pk))?;

    let metadata: crate::metadata::AudioMetadata = serde_json::from_str(&metadata_json)
        .map_err(|e| anyhow::anyhow!("Metadata deserialization error: {}", e))?;

    Ok(metadata)
}
