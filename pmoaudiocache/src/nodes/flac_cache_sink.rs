//! Sink qui encode les AudioSegment au format FLAC et les stocke dans le cache audio

use crate::metadata_ext::AudioTrackMetadataExt;
use pmoaudio::{
    nodes::{AudioError, TypedAudioNode, DEFAULT_CHANNEL_SIZE},
    type_constraints::TypeRequirement,
    AudioChunk, AudioSegment, SyncMarker, _AudioSegment,
};
use pmoflac::{encode_flac_stream, EncoderOptions, PcmFormat};
use std::{
    collections::VecDeque,
    io::Cursor,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};
use tokio::{
    io::{self, AsyncRead, ReadBuf},
    sync::{mpsc, RwLock},
};

/// Sink qui encode les `AudioSegment` reçus au format FLAC et les stocke dans le cache audio.
///
/// Ce sink :
/// - Filtre les chunks audio et ignore les autres syncmarkers (sauf TrackBoundary et EndOfStream)
/// - Crée une nouvelle entrée de cache pour chaque TrackBoundary rencontré
/// - Adapte automatiquement l'encodage FLAC selon la profondeur de bit du chunk (8/16/24/32-bit)
/// - Copie les métadonnées du TrackBoundary dans le cache après ingestion
/// - Termine l'encodage proprement quand il reçoit EndOfStream
pub struct FlacCacheSink {
    rx: mpsc::Receiver<Arc<AudioSegment>>,
    cache: Arc<crate::Cache>,
    collection: Option<String>,
    encoder_options: EncoderOptions,
    pcm_buffer_capacity: usize,
}

impl FlacCacheSink {
    /// Crée un sink FLAC cache avec les options par défaut (compression 5, buffer de 16 segments).
    ///
    /// # Arguments
    ///
    /// * `cache` - Arc vers le cache audio où stocker les fichiers FLAC encodés
    pub fn new(cache: Arc<crate::Cache>) -> (Self, mpsc::Sender<Arc<AudioSegment>>) {
        Self::with_channel_size(cache, DEFAULT_CHANNEL_SIZE)
    }

    /// Crée un sink FLAC cache avec une taille de buffer MPSC personnalisée.
    ///
    /// # Arguments
    ///
    /// * `cache` - Arc vers le cache audio
    /// * `channel_size` - Taille du buffer MPSC (nombre de segments en attente avant backpressure)
    pub fn with_channel_size(
        cache: Arc<crate::Cache>,
        channel_size: usize,
    ) -> (Self, mpsc::Sender<Arc<AudioSegment>>) {
        Self::with_config(cache, channel_size, EncoderOptions::default(), None)
    }

    /// Crée un sink FLAC cache avec une configuration complète.
    ///
    /// # Arguments
    ///
    /// * `cache` - Arc vers le cache audio
    /// * `channel_size` - Taille du buffer MPSC
    /// * `encoder_options` - Options d'encodage FLAC (compression, etc.)
    /// * `collection` - Collection optionnelle à laquelle appartiennent les fichiers
    pub fn with_config(
        cache: Arc<crate::Cache>,
        channel_size: usize,
        encoder_options: EncoderOptions,
        collection: Option<String>,
    ) -> (Self, mpsc::Sender<Arc<AudioSegment>>) {
        let (tx, rx) = mpsc::channel(channel_size);
        let sink = Self {
            rx,
            cache,
            collection,
            encoder_options,
            pcm_buffer_capacity: 8,
        };
        (sink, tx)
    }

    /// Lance l'encodage et l'ingestion dans le cache.
    ///
    /// Cette méthode crée une nouvelle entrée de cache pour chaque TrackBoundary rencontré.
    /// Les métadonnées du TrackBoundary sont copiées dans le cache après l'ingestion.
    pub async fn run(self) -> Result<FlacCacheSinkStats, AudioError> {
        let FlacCacheSink {
            mut rx,
            cache,
            collection,
            encoder_options,
            pcm_buffer_capacity,
        } = self;

        let mut all_tracks = Vec::new();
        let mut track_number = 0;

        loop {
            // Attendre le premier chunk audio pour cette track, en capturant les métadonnées du TrackBoundary
            let (first_segment, track_metadata) =
                match wait_for_first_audio_chunk_with_metadata(&mut rx).await {
                    Ok(result) => result,
                    Err(_) => {
                        // Plus d'audio disponible
                        if all_tracks.is_empty() {
                            return Err(AudioError::ProcessingError(
                                "No audio data received".into(),
                            ));
                        }
                        break;
                    }
                };

            // Extraire les informations du premier chunk
            let first_chunk = first_segment.as_chunk().unwrap();
            let sample_rate = first_chunk.sample_rate();
            let bits_per_sample = get_chunk_bit_depth(first_chunk);

            let format = PcmFormat {
                sample_rate,
                channels: 2,
                bits_per_sample,
            };
            if let Err(err) = format.validate() {
                return Err(AudioError::ProcessingError(format!(
                    "Invalid PCM format: {}",
                    err
                )));
            }

            // Créer le pipeline d'encodage pour cette track
            let (pcm_tx, pcm_rx) = mpsc::channel::<Vec<u8>>(pcm_buffer_capacity);

            // Préparer les options d'encodage avec les métadonnées du TrackBoundary
            let mut options_with_metadata = encoder_options.clone();
            options_with_metadata.metadata = track_metadata.clone();

            // Créer l'encoder
            let reader = ByteStreamReader::new(pcm_rx);
            let mut flac_stream = encode_flac_stream(reader, format, options_with_metadata)
                .await
                .map_err(|e| {
                    AudioError::ProcessingError(format!("FLAC encode init failed: {}", e))
                })?;

            // Créer un buffer pour collecter le FLAC encodé
            let mut flac_buffer = Vec::new();

            // Exécuter pump et copy en parallèle
            let pump_future = pump_track_segments(
                first_segment,
                &mut rx,
                pcm_tx,
                bits_per_sample,
                sample_rate,
            );
            let copy_future = async {
                tokio::io::copy(&mut flac_stream, &mut flac_buffer)
                    .await
                    .map_err(|e| AudioError::ProcessingError(format!("FLAC write failed: {}", e)))?;
                flac_stream
                    .wait()
                    .await
                    .map_err(|e| AudioError::ProcessingError(format!("Encoder failed: {}", e)))?;
                Ok::<_, AudioError>(())
            };

            // Attendre les deux tâches en parallèle
            let (copy_result, pump_result): (Result<(), AudioError>, Result<(u64, u64, f64, StopReason), AudioError>) =
                tokio::join!(copy_future, pump_future);
            copy_result?;
            let (chunks, samples, duration_sec, stop_reason) = pump_result?;

            // Ingérer le FLAC dans le cache
            let flac_reader = Cursor::new(flac_buffer.clone());
            let collection_ref = collection.as_deref();
            let pk = cache
                .add_from_reader(None, flac_reader, Some(flac_buffer.len() as u64), collection_ref)
                .await
                .map_err(|e| {
                    AudioError::ProcessingError(format!("Failed to add to cache: {}", e))
                })?;

            // Copier les métadonnées du TrackBoundary dans le cache
            if let Some(src_metadata) = track_metadata {
                let dest_metadata = cache.track_metadata(&pk);

                // Utiliser copy_metadata_into pour copier toutes les métadonnées
                pmometadata::copy_metadata_into(&src_metadata, &dest_metadata)
                    .await
                    .map_err(|e| {
                        AudioError::ProcessingError(format!(
                            "Failed to copy metadata to cache: {}",
                            e
                        ))
                    })?;
            }

            // Ajouter les stats de cette track
            all_tracks.push(TrackStats {
                pk,
                track_number,
                chunks_received: chunks,
                total_samples: samples,
                total_duration_sec: duration_sec,
            });

            // Vérifier le stop_reason pour savoir si on continue
            match stop_reason {
                StopReason::TrackBoundary(_metadata) => {
                    // Continuer avec la prochaine track
                    track_number += 1;
                    continue;
                }
                StopReason::EndOfStream | StopReason::ChannelClosed => {
                    // Fin de l'encodage
                    break;
                }
            }
        }

        Ok(FlacCacheSinkStats { tracks: all_tracks })
    }
}

/// Signal retourné par pump_segments indiquant pourquoi l'encodage s'est arrêté.
enum StopReason {
    TrackBoundary(Arc<RwLock<dyn pmometadata::TrackMetadata>>),
    EndOfStream,
    ChannelClosed,
}

/// Attend et retourne le premier chunk audio avec les métadonnées du TrackBoundary si présent.
/// Retourne une erreur si EndOfStream est reçu avant tout audio.
async fn wait_for_first_audio_chunk_with_metadata(
    rx: &mut mpsc::Receiver<Arc<AudioSegment>>,
) -> Result<
    (
        Arc<AudioSegment>,
        Option<Arc<RwLock<dyn pmometadata::TrackMetadata>>>,
    ),
    AudioError,
> {
    let mut track_metadata: Option<Arc<RwLock<dyn pmometadata::TrackMetadata>>> = None;

    loop {
        let segment = rx
            .recv()
            .await
            .ok_or_else(|| AudioError::ProcessingError("No audio data received".into()))?;

        match &segment.segment {
            _AudioSegment::Chunk(chunk) => {
                if chunk.len() == 0 {
                    return Err(AudioError::ProcessingError("Received empty chunk".into()));
                }
                return Ok((segment, track_metadata));
            }
            _AudioSegment::Sync(marker) => match **marker {
                SyncMarker::TrackBoundary { ref metadata, .. } => {
                    // Capturer les métadonnées du TrackBoundary
                    track_metadata = Some(metadata.clone());
                    continue;
                }
                SyncMarker::EndOfStream => {
                    return Err(AudioError::ProcessingError(
                        "EndOfStream received before any audio".into(),
                    ));
                }
                _ => {
                    // Ignorer TopZeroSync, Heartbeat, etc.
                    continue;
                }
            },
        }
    }
}

/// Pompe les segments pour une seule track (s'arrête au TrackBoundary).
async fn pump_track_segments(
    first_segment: Arc<AudioSegment>,
    rx: &mut mpsc::Receiver<Arc<AudioSegment>>,
    pcm_tx: mpsc::Sender<Vec<u8>>,
    bits_per_sample: u8,
    expected_rate: u32,
) -> Result<(u64, u64, f64, StopReason), AudioError> {
    let mut chunks = 0u64;
    let mut samples = 0u64;
    let mut duration_sec = 0.0f64;

    // Traiter le premier segment
    if let Some(chunk) = first_segment.as_chunk() {
        let pcm_bytes = chunk_to_pcm_bytes(chunk, bits_per_sample)?;
        if !pcm_bytes.is_empty() {
            pcm_tx
                .send(pcm_bytes)
                .await
                .map_err(|_| AudioError::SendError)?;
            chunks += 1;
            samples += chunk.len() as u64;
            duration_sec += chunk.len() as f64 / expected_rate as f64;
        }
    }

    // Boucle sur les segments suivants
    loop {
        let segment = match rx.recv().await {
            Some(seg) => seg,
            None => {
                drop(pcm_tx); // Fermer le channel PCM
                return Ok((chunks, samples, duration_sec, StopReason::ChannelClosed));
            }
        };

        match &segment.segment {
            _AudioSegment::Chunk(chunk) => {
                // Vérifier la cohérence du sample rate
                if chunk.sample_rate() != expected_rate {
                    return Err(AudioError::ProcessingError(format!(
                        "FlacCacheSink: inconsistent sample rate ({} vs {})",
                        chunk.sample_rate(),
                        expected_rate
                    )));
                }

                let pcm_bytes = chunk_to_pcm_bytes(chunk, bits_per_sample)?;
                if pcm_bytes.is_empty() {
                    continue;
                }

                pcm_tx
                    .send(pcm_bytes)
                    .await
                    .map_err(|_| AudioError::SendError)?;

                chunks += 1;
                samples += chunk.len() as u64;
                duration_sec += chunk.len() as f64 / expected_rate as f64;
            }
            _AudioSegment::Sync(marker) => match &**marker {
                SyncMarker::TrackBoundary { metadata, .. } => {
                    drop(pcm_tx); // Fermer le channel PCM
                    return Ok((
                        chunks,
                        samples,
                        duration_sec,
                        StopReason::TrackBoundary(metadata.clone()),
                    ));
                }
                SyncMarker::EndOfStream => {
                    drop(pcm_tx); // Fermer le channel PCM
                    return Ok((chunks, samples, duration_sec, StopReason::EndOfStream));
                }
                _ => {} // Ignorer les autres syncmarkers
            },
        }
    }
}

/// Détermine la profondeur de bit d'un chunk audio
fn get_chunk_bit_depth(chunk: &AudioChunk) -> u8 {
    match chunk {
        AudioChunk::I16(_) => 16,
        AudioChunk::I24(_) => 24,
        AudioChunk::I32(_) => 32,
        AudioChunk::F32(_) => 32, // Les flottants seront convertis en 32-bit
        AudioChunk::F64(_) => 32, // Les flottants seront convertis en 32-bit
    }
}

/// Convertit un chunk audio en bytes PCM avec la profondeur de bit spécifiée
fn chunk_to_pcm_bytes(chunk: &AudioChunk, bits_per_sample: u8) -> Result<Vec<u8>, AudioError> {
    // Vérifier que le chunk est de type entier
    match chunk {
        AudioChunk::F32(_) | AudioChunk::F64(_) => {
            return Err(AudioError::ProcessingError(
                "FlacCacheSink only supports integer audio chunks (I16, I24, I32)".into(),
            ));
        }
        _ => {}
    }

    let len = chunk.len();
    let bytes_per_frame = (bits_per_sample / 8) as usize * 2; // 2 channels
    let mut bytes = Vec::with_capacity(len * bytes_per_frame);

    // Convertir selon le type du chunk
    match (chunk, bits_per_sample) {
        // I16 source
        (AudioChunk::I16(data), 16) => {
            for frame in data.frames() {
                bytes.extend_from_slice(&frame[0].to_le_bytes());
                bytes.extend_from_slice(&frame[1].to_le_bytes());
            }
        }
        (AudioChunk::I16(data), 24) => {
            for frame in data.frames() {
                let left = (frame[0] as i32) << 8;
                let right = (frame[1] as i32) << 8;
                bytes.extend_from_slice(&left.to_le_bytes()[..3]);
                bytes.extend_from_slice(&right.to_le_bytes()[..3]);
            }
        }
        (AudioChunk::I16(data), 32) => {
            for frame in data.frames() {
                let left = (frame[0] as i32) << 16;
                let right = (frame[1] as i32) << 16;
                bytes.extend_from_slice(&left.to_le_bytes());
                bytes.extend_from_slice(&right.to_le_bytes());
            }
        }

        // I24 source
        (AudioChunk::I24(data), 16) => {
            for frame in data.frames() {
                let left = (frame[0].as_i32() >> 8) as i16;
                let right = (frame[1].as_i32() >> 8) as i16;
                bytes.extend_from_slice(&left.to_le_bytes());
                bytes.extend_from_slice(&right.to_le_bytes());
            }
        }
        (AudioChunk::I24(data), 24) => {
            for frame in data.frames() {
                bytes.extend_from_slice(&frame[0].as_i32().to_le_bytes()[..3]);
                bytes.extend_from_slice(&frame[1].as_i32().to_le_bytes()[..3]);
            }
        }
        (AudioChunk::I24(data), 32) => {
            for frame in data.frames() {
                let left = frame[0].as_i32() << 8;
                let right = frame[1].as_i32() << 8;
                bytes.extend_from_slice(&left.to_le_bytes());
                bytes.extend_from_slice(&right.to_le_bytes());
            }
        }

        // I32 source
        (AudioChunk::I32(data), 16) => {
            for frame in data.frames() {
                let left = (frame[0] >> 16) as i16;
                let right = (frame[1] >> 16) as i16;
                bytes.extend_from_slice(&left.to_le_bytes());
                bytes.extend_from_slice(&right.to_le_bytes());
            }
        }
        (AudioChunk::I32(data), 24) => {
            for frame in data.frames() {
                let left = frame[0] >> 8;
                let right = frame[1] >> 8;
                bytes.extend_from_slice(&left.to_le_bytes()[..3]);
                bytes.extend_from_slice(&right.to_le_bytes()[..3]);
            }
        }
        (AudioChunk::I32(data), 32) => {
            for frame in data.frames() {
                bytes.extend_from_slice(&frame[0].to_le_bytes());
                bytes.extend_from_slice(&frame[1].to_le_bytes());
            }
        }

        _ => {
            return Err(AudioError::ProcessingError(format!(
                "Unsupported bits_per_sample: {}",
                bits_per_sample
            )));
        }
    }

    Ok(bytes)
}

struct ByteStreamReader {
    rx: mpsc::Receiver<Vec<u8>>,
    buffer: VecDeque<u8>,
    finished: bool,
}

impl ByteStreamReader {
    fn new(rx: mpsc::Receiver<Vec<u8>>) -> Self {
        Self {
            rx,
            buffer: VecDeque::new(),
            finished: false,
        }
    }
}

impl AsyncRead for ByteStreamReader {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        loop {
            if !self.buffer.is_empty() {
                let to_copy = self.buffer.len().min(buf.remaining());
                if to_copy == 0 {
                    return Poll::Ready(Ok(()));
                }

                // VecDeque::make_contiguous pour copier efficacement
                let slice = self.buffer.make_contiguous();
                buf.put_slice(&slice[..to_copy]);
                self.buffer.drain(..to_copy);
                return Poll::Ready(Ok(()));
            }

            if self.finished {
                return Poll::Ready(Ok(()));
            }

            match Pin::new(&mut self.rx).poll_recv(cx) {
                Poll::Ready(Some(bytes)) => {
                    if bytes.is_empty() {
                        continue;
                    }
                    self.buffer.extend(bytes);
                }
                Poll::Ready(None) => {
                    self.finished = true;
                    return Poll::Ready(Ok(()));
                }
                Poll::Pending => return Poll::Pending,
            }
        }
    }
}

/// Statistiques pour une track individuelle.
#[derive(Debug, Clone)]
pub struct TrackStats {
    pub pk: String,
    pub track_number: usize,
    pub chunks_received: u64,
    pub total_samples: u64,
    pub total_duration_sec: f64,
}

/// Statistiques produites par le `FlacCacheSink`.
#[derive(Debug, Clone)]
pub struct FlacCacheSinkStats {
    pub tracks: Vec<TrackStats>,
}

impl TypedAudioNode for FlacCacheSink {
    fn input_type(&self) -> Option<TypeRequirement> {
        // FlacCacheSink accepte n'importe quel type entier (I16, I24, I32)
        // mais rejette les chunks flottants
        Some(TypeRequirement::any_integer())
    }

    fn output_type(&self) -> Option<TypeRequirement> {
        // FlacCacheSink est un sink, il ne produit pas d'audio
        None
    }
}
