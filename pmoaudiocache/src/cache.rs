//! Module de gestion du cache audio avec conversion FLAC
//!
//! Ce module étend le cache générique de `pmocache` avec des fonctionnalités
//! spécifiques aux fichiers audio : conversion FLAC automatique et stockage
//! des métadonnées en JSON dans la base de données.

use anyhow::Result;
use crate::metadata_ext::AudioTrackMetadataExt;
use pmocache::CacheConfig;
use pmocache::download::TransformMetadata;
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

async fn persist_transform_streaminfo(cache: Arc<Cache>, pk: &str, tmeta: &TransformMetadata) {
    use std::time::Duration;

    let track_meta = cache.track_metadata(pk);
    let mut meta = track_meta.write().await;

    if let Some(sr) = tmeta.sample_rate {
        let _ = meta.set_sample_rate(Some(sr)).await;
    }
    if let Some(bps) = tmeta.bits_per_sample {
        let _ = meta.set_bits_per_sample(Some(bps)).await;
    }
    if let Some(ts) = tmeta.total_samples {
        let _ = meta.set_total_samples(Some(ts)).await;

        // Calculer la durée à partir de total_samples et sample_rate
        if let Some(sr) = tmeta.sample_rate {
            if sr > 0 {
                let secs = (ts as f64 / sr as f64).round() as u64;
                let _ = meta.set_duration(Some(Duration::from_secs(secs))).await;
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
    cache: Arc<Cache>,
    url: &str,
    collection: Option<&str>,
) -> Result<String> {
    use std::time::Duration;

    // Ajouter au cache (déclenche le download et la conversion)
    let pk = cache.add_from_url(url, collection).await?;

    // Attendre que le fichier soit téléchargé et converti
    cache.wait_until_finished(&pk).await?;

    // Persister les métadonnées de transformation (taux d'échantillonnage, bits par échantillon, etc.)
    if let Some(transform) = cache.transform_metadata(&pk).await {
        persist_transform_streaminfo(cache.clone(), &pk, &transform).await;
    }

    // Lire le fichier FLAC pour extraire les métadonnées
    let file_path = cache.get_file_path(&pk);
    let flac_bytes = tokio::fs::read(&file_path).await?;

    // Extraire les métadonnées depuis le fichier audio
    let metadata = crate::metadata::AudioMetadata::from_bytes(&flac_bytes)?;

    // Créer une instance TrackMetadata pour persister via l'interface unifiée
    let track_meta = cache.clone().track_metadata(&pk);
    let mut meta = track_meta.write().await;

    // Informations techniques issues du flux FLAC (streaminfo)
    let streaminfo = parse_flac_streaminfo(&flac_bytes);
    if let Some((sr, bps, total_samples)) = streaminfo {
        let _ = meta.set_sample_rate(Some(sr)).await;
        let _ = meta.set_bits_per_sample(Some(bps)).await;
        let _ = meta.set_total_samples(Some(total_samples)).await;

        // Calculer la durée si elle n'est pas disponible depuis les tags
        if metadata.duration_secs.is_none() && sr > 0 {
            let secs = (total_samples as f64 / sr as f64).round() as u64;
            let _ = meta.set_duration(Some(Duration::from_secs(secs))).await;
        }
    }

    // Déterminer la collection automatique avant de move les valeurs
    let auto_collection = if collection.is_none() {
        metadata.collection_key()
    } else {
        None
    };

    // Métadonnées descriptives (tags) - en écrasant éventuellement celles du streaminfo
    if let Some(d) = metadata.duration_secs {
        let _ = meta.set_duration(Some(Duration::from_secs(d))).await;
    }
    if let Some(title) = metadata.title {
        let _ = meta.set_title(Some(title)).await;
    }
    if let Some(artist) = metadata.artist {
        let _ = meta.set_artist(Some(artist)).await;
    }
    if let Some(album) = metadata.album {
        let _ = meta.set_album(Some(album)).await;
    }
    if let Some(year) = metadata.year {
        let _ = meta.set_year(Some(year)).await;
    }
    if let Some(genre) = metadata.genre {
        let _ = meta.set_genre(Some(genre)).await;
    }
    if let Some(track_number) = metadata.track_number {
        let _ = meta.set_track_number(Some(track_number)).await;
    }
    if let Some(track_total) = metadata.track_total {
        let _ = meta.set_track_total(Some(track_total)).await;
    }
    if let Some(disc_number) = metadata.disc_number {
        let _ = meta.set_disc_number(Some(disc_number)).await;
    }
    if let Some(disc_total) = metadata.disc_total {
        let _ = meta.set_disc_total(Some(disc_total)).await;
    }
    if let Some(channels) = metadata.channels {
        let _ = meta.set_channels(Some(channels)).await;
    }
    if let Some(bitrate) = metadata.bitrate {
        let _ = meta.set_bitrate(Some(bitrate)).await;
    }

    // Libérer le lock explicitement avant les opérations de collection
    drop(meta);

    // Mettre à jour la collection si les métadonnées en fournissent une
    if let Some(auto_collection) = auto_collection {
        cache
            .db
            .add(&pk, None, Some(&auto_collection))
            .map_err(|e| anyhow::anyhow!("Database error: {}", e))?;
        cache.db.set_origin_url(&pk, url)?;
    }

    Ok(pk)
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
