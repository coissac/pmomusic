//! Module de gestion du cache audio avec conversion FLAC
//!
//! Ce module étend le cache générique de `pmocache` avec des fonctionnalités
//! spécifiques aux fichiers audio : conversion FLAC automatique et stockage
//! des métadonnées en JSON dans la base de données.

use anyhow::Result;
use pmocache::CacheConfig;
use serde_json::{Number, Value};
use std::sync::Arc;
use pmocache::download::TransformMetadata;

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

fn persist_transform_streaminfo(cache: &Cache, pk: &str, tmeta: &TransformMetadata) {
    if let Some(sr) = tmeta.sample_rate {
        let _ = cache
            .db
            .set_a_metadata(pk, "sample_rate", Value::Number(Number::from(sr)));
    }
    if let Some(bps) = tmeta.bits_per_sample {
        let _ = cache
            .db
            .set_a_metadata(pk, "bits_per_sample", Value::Number(Number::from(bps)));
    }
    if let Some(ch) = tmeta.channels {
        let _ = cache
            .db
            .set_a_metadata(pk, "channels", Value::Number(Number::from(ch)));
    }
    if let Some(ts) = tmeta.total_samples {
        let _ = cache
            .db
            .set_a_metadata(pk, "total_samples", Value::Number(Number::from(ts)));
        if let Some(sr) = tmeta.sample_rate {
            if sr > 0 {
                let secs = (ts as f64 / sr as f64).round() as u64;
                let _ = cache
                    .db
                    .set_a_metadata(pk, "duration_secs", Value::Number(Number::from(secs)));
            }
        }
    }
}

/// Crée un cache audio et lance la consolidation en arrière-plan
///
/// Cette fonction crée le cache et lance immédiatement une consolidation
/// pour nettoyer les fichiers incomplets (sans marker de complétion).
///
/// # Arguments
///
/// * `dir` - Répertoire de stockage du cache
/// * `limit` - Limite de taille du cache (nombre de pistes)
///
/// # Returns
///
/// Arc vers l'instance du cache configurée pour la conversion FLAC automatique
///
/// # Exemple
///
/// ```rust,no_run
/// use pmoaudiocache::cache;
///
/// # async fn example() -> anyhow::Result<()> {
/// let cache = cache::new_cache_with_consolidation("./audio_cache", 1000).await?;
/// # Ok(())
/// # }
/// ```
pub async fn new_cache_with_consolidation(dir: &str, limit: usize) -> Result<Arc<Cache>> {
    let cache = Arc::new(new_cache(dir, limit)?);

    // Lancer la consolidation en arrière-plan pour nettoyer les fichiers incomplets
    let cache_clone = cache.clone();
    tokio::spawn(async move {
        if let Err(e) = cache_clone.consolidate().await {
            tracing::warn!("Failed to consolidate cache on startup: {}", e);
        } else {
            tracing::info!("Cache consolidated successfully on startup");
        }
    });

    Ok(cache)
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

    if let Some(transform) = cache.transform_metadata(&pk).await {
        persist_transform_streaminfo(cache, &pk, &transform);
    }

    // Lire le fichier FLAC pour extraire les métadonnées
    let file_path = cache.get_file_path(&pk);
    let flac_bytes = tokio::fs::read(&file_path).await?;

    // Extraire les métadonnées
    let mut metadata = crate::metadata::AudioMetadata::from_bytes(&flac_bytes)?;
    // Propager les métadonnées techniques dans la DB (sans passer par le JSON)
    if let Some(d) = metadata.duration_secs {
        let _ = cache
            .db
            .set_a_metadata(&pk, "duration_secs", Value::Number(Number::from(d)));
    }
    if let Some((sr, bps, total_samples)) = parse_flac_streaminfo(&flac_bytes) {
        let _ = cache
            .db
            .set_a_metadata(&pk, "sample_rate", Value::Number(Number::from(sr)));
        let _ = cache
            .db
            .set_a_metadata(&pk, "bits_per_sample", Value::Number(Number::from(bps)));
        let _ = cache.db.set_a_metadata(
            &pk,
            "total_samples",
            Value::Number(Number::from(total_samples)),
        );
        if metadata.duration_secs.is_none() && sr > 0 {
            let secs = (total_samples as f64 / sr as f64).round() as u64;
            let _ = cache
                .db
                .set_a_metadata(&pk, "duration_secs", Value::Number(Number::from(secs)));
            metadata.duration_secs = Some(secs);
        }
        if metadata.sample_rate.is_none() {
            metadata.sample_rate = Some(sr);
        }
    }

    // Extraire aussi les informations FLAC de base (STREAMINFO) pour peupler TrackMetadata
    if let Some((sr, bps, total_samples)) = parse_flac_streaminfo(&flac_bytes) {
        if let Err(e) = cache
            .db
            .set_a_metadata(&pk, "sample_rate", Value::Number(Number::from(sr)))
        {
            tracing::warn!("Failed to persist sample_rate for {}: {}", pk, e);
        }
        if let Err(e) = cache
            .db
            .set_a_metadata(&pk, "bits_per_sample", Value::Number(Number::from(bps)))
        {
            tracing::warn!("Failed to persist bits_per_sample for {}: {}", pk, e);
        }
        if let Err(e) = cache
            .db
            .set_a_metadata(&pk, "total_samples", Value::Number(Number::from(total_samples)))
        {
            tracing::warn!("Failed to persist total_samples for {}: {}", pk, e);
        }
    }

    if let Some(transform) = cache.transform_metadata(&pk).await {
        if let Some(mode) = transform.mode {
            metadata.conversion = Some(crate::metadata::AudioConversionInfo {
                mode,
                source_codec: transform.input_codec,
            });
        }
    }
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

/// Parse minimal FLAC STREAMINFO (first metadata block) to retrieve sample rate,
/// bits per sample, and total samples.
fn parse_flac_streaminfo(data: &[u8]) -> Option<(u32, u8, u64)> {
    // Expect "fLaC" + 4-byte metadata block header + 34-byte STREAMINFO
    if data.len() < 4 + 4 + 34 {
        return None;
    }
    if &data[0..4] != b"fLaC" {
        return None;
    }

    let block_type = data[4] & 0x7F;
    let block_len = ((data[5] as usize) << 16) | ((data[6] as usize) << 8) | data[7] as usize;
    if block_type != 0 || block_len < 34 || data.len() < 8 + block_len {
        return None;
    }

    let s = &data[8..8 + 34];

    // sample_rate: 20 bits: bytes 10..12
    let sample_rate =
        ((s[10] as u32) << 12) | ((s[11] as u32) << 4) | ((s[12] as u32 & 0xF0) >> 4);

    // bits_per_sample: 5 bits spanning byte 12 (lsb) and byte 13 (msb)
    let bps_raw = (((s[12] & 0x01) as u8) << 4) | ((s[13] & 0xF0) >> 4);
    let bits_per_sample = bps_raw.saturating_add(1);

    // total_samples: 36 bits: lower 4 bits of byte 13 + bytes 14..17
    let total_samples = (((s[13] & 0x0F) as u64) << 32)
        | ((s[14] as u64) << 24)
        | ((s[15] as u64) << 16)
        | ((s[16] as u64) << 8)
        | (s[17] as u64);

    Some((sample_rate, bits_per_sample, total_samples))
}
