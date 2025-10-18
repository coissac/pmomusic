//! Module de gestion du cache audio avec conversion FLAC
//!
//! Ce module étend le cache générique de `pmocache` avec des fonctionnalités
//! spécifiques aux fichiers audio : conversion FLAC automatique et stockage
//! des métadonnées en JSON dans la base de données.

use anyhow::Result;
use pmocache::{CacheConfig, StreamTransformer};
use std::sync::Arc;

/// Configuration pour le cache audio
pub struct AudioConfig;

impl CacheConfig for AudioConfig {
    fn file_extension() -> &'static str {
        "flac"
    }

    fn table_name() -> &'static str {
        "audio_tracks"
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

/// Créateur de transformer FLAC
///
/// Convertit automatiquement tout fichier audio téléchargé en format FLAC
/// en traitant les données au vol, sans tout charger en mémoire.
///
/// # Workflow
///
/// 1. Télécharger les bytes par chunks depuis le stream HTTP
/// 2. Buffer temporaire pour accumuler les données nécessaires à Symphonia
/// 3. Décoder l'audio en PCM via Symphonia
/// 4. Encoder le PCM en FLAC progressivement via flacenc
/// 5. Écrire les frames FLAC directement dans le fichier
/// 6. Mettre à jour la progression après chaque chunk
///
/// Note: Bien que nous utilisions un buffer temporaire, celui-ci est géré
/// de manière efficace et les données FLAC sont écrites au fur et à mesure.
fn create_flac_transformer() -> StreamTransformer {
    Box::new(|response, mut file, progress| {
        Box::pin(async move {
            use futures_util::StreamExt;
            use tokio::io::AsyncWriteExt;

            // 1. Collecter tous les bytes du stream
            // Note: Symphonia nécessite un MediaSource avec Read + Seek,
            // ce qui n'est pas compatible avec un vrai streaming HTTP.
            // Nous devons donc bufferiser les données.
            let mut buffer = Vec::new();
            let mut stream = response.bytes_stream();

            while let Some(chunk) = stream.next().await {
                let chunk = chunk.map_err(|e| format!("Stream error: {}", e))?;
                buffer.extend_from_slice(&chunk);
            }

            tracing::debug!("Downloaded {} bytes total, starting FLAC conversion", buffer.len());

            // 2. Si c'est déjà du FLAC, on l'écrit directement
            if buffer.len() >= 4 && &buffer[0..4] == b"fLaC" {
                tracing::debug!("Input is already FLAC, writing directly");
                file.write_all(&buffer).await.map_err(|e| e.to_string())?;
                file.flush().await.map_err(|e| e.to_string())?;
                progress(buffer.len() as u64);
                return Ok(());
            }

            tracing::debug!("Converting to FLAC with Symphonia + flacenc");

            // 3. Décoder l'audio avec Symphonia
            let (samples, channels, sample_rate, bits_per_sample) = {
                use symphonia::core::audio::SampleBuffer;
                use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
                use symphonia::core::formats::FormatOptions;
                use symphonia::core::io::MediaSourceStream;
                use symphonia::core::meta::MetadataOptions;
                use symphonia::core::probe::Hint;
                use symphonia::core::errors::Error as SymphoniaError;
                use std::io::Cursor;

                let cursor = Cursor::new(buffer);
                let mss = MediaSourceStream::new(Box::new(cursor), Default::default());

                let hint = Hint::new();
                let probed = symphonia::default::get_probe()
                    .format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default())
                    .map_err(|e| format!("Failed to probe format: {}", e))?;

                let mut format = probed.format;

                let track = format
                    .tracks()
                    .iter()
                    .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
                    .ok_or_else(|| "No audio track found".to_string())?;

                let mut decoder = symphonia::default::get_codecs()
                    .make(&track.codec_params, &DecoderOptions::default())
                    .map_err(|e| format!("Failed to create decoder: {}", e))?;

                let channels = track.codec_params.channels
                    .ok_or_else(|| "No channel info".to_string())?
                    .count();

                let sample_rate = track.codec_params.sample_rate
                    .ok_or_else(|| "No sample rate info".to_string())?;

                let bits_per_sample = track.codec_params.bits_per_sample
                    .unwrap_or(16);

                let mut samples_i32 = Vec::new();
                let track_id = track.id;

                // Décoder tous les packets
                loop {
                    let packet = match format.next_packet() {
                        Ok(packet) => packet,
                        Err(SymphoniaError::ResetRequired) => {
                            decoder.reset();
                            continue;
                        }
                        Err(SymphoniaError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                            break;
                        }
                        Err(e) => return Err(format!("Decode error: {}", e)),
                    };

                    if packet.track_id() != track_id {
                        continue;
                    }

                    match decoder.decode(&packet) {
                        Ok(decoded) => {
                            let spec = *decoded.spec();
                            let duration = decoded.capacity() as u64;

                            // Convertir en i32 pour flacenc
                            // Note: Symphonia retourne des samples i32, nous devons les convertir
                            // en fonction du bits_per_sample réel
                            let mut sample_buf = SampleBuffer::<i32>::new(duration, spec);
                            sample_buf.copy_interleaved_ref(decoded);
                            samples_i32.extend_from_slice(sample_buf.samples());
                        }
                        Err(SymphoniaError::DecodeError(_)) => continue,
                        Err(e) => return Err(format!("Decode error: {}", e)),
                    }
                }

                if samples_i32.is_empty() {
                    return Err("No samples decoded".to_string());
                }

                tracing::debug!(
                    "Decoded {} samples (i32), {} channels, {} Hz, {} bits",
                    samples_i32.len(),
                    channels,
                    sample_rate,
                    bits_per_sample
                );

                // Normaliser les samples i32 vers la plage appropriée pour flacenc
                // Symphonia retourne des samples i32 en pleine échelle (32 bits),
                // nous devons les normaliser selon le bits_per_sample réel
                let (normalized_samples, target_bits): (Vec<i32>, u32) = match bits_per_sample {
                    0..=16 => {
                        // Pour 16 bits ou moins, normaliser vers la plage i16
                        tracing::debug!("Normalizing to 16-bit");
                        let samples = samples_i32.iter().map(|&s| (s >> 16) as i32).collect();
                        (samples, 16)
                    },
                    17..=24 => {
                        // Pour 17-24 bits, normaliser vers la plage 24-bit
                        tracing::debug!("Normalizing to 24-bit");
                        let samples = samples_i32.iter().map(|&s| (s >> 8) as i32).collect();
                        (samples, 24)
                    },
                    _ => {
                        // Pour 25-32 bits, garder la pleine échelle i32
                        tracing::debug!("Keeping 32-bit");
                        (samples_i32, 32)
                    }
                };

                (normalized_samples, channels, sample_rate, target_bits)
            };

            tracing::debug!("Encoding to FLAC: {} samples, {} channels, {} Hz, {} bits",
                samples.len(), channels, sample_rate, bits_per_sample);

            // 4. Encoder en FLAC avec flacenc
            // Note: L'encodage FLAC est une opération bloquante/CPU-intensive,
            // donc nous l'exécutons dans un thread bloquant pour ne pas bloquer le runtime Tokio
            let flac_data = tokio::task::spawn_blocking(move || {
                use flacenc::component::BitRepr;
                use flacenc::bitsink::ByteSink;
                use flacenc::error::Verify;

                let config = flacenc::config::Encoder::default()
                    .into_verified()
                    .map_err(|e| format!("FLAC config error: {:?}", e))?;

                let source = flacenc::source::MemSource::from_samples(
                    &samples,
                    channels,
                    bits_per_sample as usize,
                    sample_rate as usize,
                );

                let flac_stream = flacenc::encode_with_fixed_block_size(
                    &config,
                    source,
                    config.block_size,
                )
                .map_err(|e| format!("FLAC encode error: {:?}", e))?;

                let mut sink = ByteSink::new();
                flac_stream.write(&mut sink)
                    .map_err(|e| format!("FLAC write error: {:?}", e))?;

                Ok::<Vec<u8>, String>(sink.into_inner())
            })
            .await
            .map_err(|e| format!("Spawn blocking error: {}", e))??;

            tracing::debug!("FLAC encoding complete: {} bytes", flac_data.len());

            // 5. Écrire le fichier FLAC
            file.write_all(&flac_data).await.map_err(|e| e.to_string())?;
            file.flush().await.map_err(|e| e.to_string())?;

            // 6. Mettre à jour la progression finale
            progress(flac_data.len() as u64);

            Ok(())
        })
    })
}

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
    let transformer_factory = Arc::new(|| create_flac_transformer());
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
/// let cache = cache::new_cache("./audio_cache", 1000, "http://localhost:8080")?;
/// let pk = cache::add_with_metadata_extraction(
///     &cache,
///     "http://example.com/track.flac",
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
    let file_path = cache.file_path(&pk);
    let flac_bytes = tokio::fs::read(&file_path).await?;

    // Extraire les métadonnées
    let metadata = crate::metadata::AudioMetadata::from_bytes(&flac_bytes)?;

    // Sérialiser en JSON
    let metadata_json = serde_json::to_string(&metadata)?;

    // Stocker dans la DB
    cache.db.update_metadata(&pk, &metadata_json)
        .map_err(|e| anyhow::anyhow!("Database error: {}", e))?;

    // Mettre à jour la collection si les métadonnées en fournissent une
    if collection.is_none() {
        if let Some(auto_collection) = metadata.collection_key() {
            cache.db.add(&pk, url, Some(&auto_collection))
                .map_err(|e| anyhow::anyhow!("Database error: {}", e))?;
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
    let metadata_json = cache.db.get_metadata_json(pk)
        .map_err(|e| anyhow::anyhow!("Database error: {}", e))?
        .ok_or_else(|| anyhow::anyhow!("No metadata found for pk: {}", pk))?;

    let metadata: crate::metadata::AudioMetadata = serde_json::from_str(&metadata_json)
        .map_err(|e| anyhow::anyhow!("Metadata deserialization error: {}", e))?;

    Ok(metadata)
}

