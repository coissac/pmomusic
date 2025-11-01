use crate::{
    nodes::{AudioError, TypedAudioNode, DEFAULT_CHANNEL_SIZE},
    type_constraints::TypeRequirement,
    AudioChunk, AudioSegment, SyncMarker,
};
use pmoflac::{encode_flac_stream, EncoderOptions, PcmFormat};
use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};
use tokio::{
    fs::File,
    io::{self, AsyncRead, AsyncWriteExt, ReadBuf},
    sync::mpsc,
};

/// Sink qui encode les `AudioSegment` reçus au format FLAC.
///
/// Ce sink :
/// - Filtre les chunks audio et ignore les autres syncmarkers (sauf TrackBoundary et EndOfStream)
/// - Crée un nouveau fichier FLAC pour chaque TrackBoundary rencontré
/// - Adapte automatiquement l'encodage FLAC selon la profondeur de bit du chunk (8/16/24/32-bit)
/// - Termine l'encodage proprement quand il reçoit EndOfStream
pub struct FlacFileSink {
    rx: mpsc::Receiver<Arc<AudioSegment>>,
    base_path: PathBuf,
    encoder_options: EncoderOptions,
    pcm_buffer_capacity: usize,
}

impl FlacFileSink {
    /// Crée un sink FLAC avec les options par défaut (compression 5, buffer de 16 segments).
    ///
    /// # Arguments
    ///
    /// * `base_path` - Chemin de base pour les fichiers FLAC. Si des TrackBoundary sont reçus,
    ///   des fichiers seront créés avec des suffixes (_01, _02, etc.)
    pub fn new<P: Into<PathBuf>>(base_path: P) -> (Self, mpsc::Sender<Arc<AudioSegment>>) {
        Self::with_channel_size(base_path, DEFAULT_CHANNEL_SIZE)
    }

    /// Crée un sink FLAC avec une taille de buffer MPSC personnalisée.
    ///
    /// # Arguments
    ///
    /// * `base_path` - Chemin de base pour les fichiers FLAC
    /// * `channel_size` - Taille du buffer MPSC (nombre de segments en attente avant backpressure)
    pub fn with_channel_size<P: Into<PathBuf>>(
        base_path: P,
        channel_size: usize,
    ) -> (Self, mpsc::Sender<Arc<AudioSegment>>) {
        Self::with_config(base_path, channel_size, EncoderOptions::default())
    }

    /// Crée un sink FLAC avec une configuration complète.
    ///
    /// # Arguments
    ///
    /// * `base_path` - Chemin de base pour les fichiers FLAC
    /// * `channel_size` - Taille du buffer MPSC
    /// * `encoder_options` - Options d'encodage FLAC (compression, etc.)
    pub fn with_config<P: Into<PathBuf>>(
        base_path: P,
        channel_size: usize,
        encoder_options: EncoderOptions,
    ) -> (Self, mpsc::Sender<Arc<AudioSegment>>) {
        let (tx, rx) = mpsc::channel(channel_size);
        let sink = Self {
            rx,
            base_path: base_path.into(),
            encoder_options,
            pcm_buffer_capacity: 8,
        };
        (sink, tx)
    }

    /// Lance l'encodage vers le(s) fichier(s) cible(s).
    ///
    /// Cette méthode crée un nouveau fichier FLAC pour chaque TrackBoundary rencontré.
    /// Les fichiers sont nommés selon la convention :
    /// - Track 0 : base_path.flac
    /// - Track 1 : base_path_01.flac
    /// - Track 2 : base_path_02.flac, etc.
    pub async fn run(self) -> Result<FlacFileSinkStats, AudioError> {
        let FlacFileSink {
            mut rx,
            base_path,
            encoder_options,
            pcm_buffer_capacity,
        } = self;

        let mut all_tracks = Vec::new();
        let mut track_number = 0;

        loop {
            // Attendre le premier chunk audio pour cette track, en capturant les métadonnées du TrackBoundary
            let (first_segment, track_metadata) = match wait_for_first_audio_chunk_with_metadata(&mut rx).await {
                Ok(result) => result,
                Err(_) => {
                    // Plus d'audio disponible
                    if all_tracks.is_empty() {
                        return Err(AudioError::ProcessingError("No audio data received".into()));
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

            // Générer le chemin du fichier pour cette track
            let track_path = generate_track_path(&base_path, track_number);

            // Créer le pipeline d'encodage pour cette track
            let (pcm_tx, pcm_rx) = mpsc::channel::<Vec<u8>>(pcm_buffer_capacity);

            // Préparer les options d'encodage avec les métadonnées du TrackBoundary
            let mut options_with_metadata = encoder_options.clone();
            options_with_metadata.metadata = track_metadata;

            // Créer l'encoder et le fichier
            let reader = ByteStreamReader::new(pcm_rx);
            let mut flac_stream = encode_flac_stream(reader, format, options_with_metadata)
                .await
                .map_err(|e| {
                    AudioError::ProcessingError(format!("FLAC encode init failed: {}", e))
                })?;

            let mut output = File::create(&track_path).await.map_err(|e| {
                AudioError::ProcessingError(format!("Failed to create {:?}: {}", track_path, e))
            })?;

            // Exécuter pump et copy en parallèle avec tokio::select! en boucle
            let pump_future =
                pump_track_segments(first_segment, &mut rx, pcm_tx, bits_per_sample, sample_rate);
            let copy_future = async {
                let copy_result = tokio::io::copy(&mut flac_stream, &mut output).await;
                let flush_result = output.flush().await;
                let wait_result = flac_stream.wait().await;

                copy_result.map_err(|e| {
                    AudioError::ProcessingError(format!("FLAC write failed: {}", e))
                })?;
                flush_result
                    .map_err(|e| AudioError::ProcessingError(format!("Failed to flush: {}", e)))?;
                wait_result
                    .map_err(|e| AudioError::ProcessingError(format!("Encoder failed: {}", e)))?;
                Ok::<_, AudioError>(())
            };

            // Attendre les deux tâches en parallèle
            let (copy_result, pump_result) = tokio::join!(copy_future, pump_future);
            copy_result?;
            let (chunks, samples, duration_sec, stop_reason) = pump_result?;

            // Ajouter les stats de cette track
            all_tracks.push(TrackStats {
                path: track_path,
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

        Ok(FlacFileSinkStats { tracks: all_tracks })
    }
}

/// Génère le chemin de fichier pour une track donnée.
/// - track 0 → base_path.flac
/// - track 1 → base_path_01.flac
/// - track 2 → base_path_02.flac, etc.
fn generate_track_path(base_path: &Path, track_number: usize) -> PathBuf {
    if track_number == 0 {
        base_path.to_path_buf()
    } else {
        let stem = base_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("output");
        let extension = base_path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("flac");
        let parent = base_path.parent().unwrap_or(Path::new("."));
        parent.join(format!("{}_{:02}.{}", stem, track_number, extension))
    }
}

/// Signal retourné par pump_segments indiquant pourquoi l'encodage s'est arrêté.
enum StopReason {
    TrackBoundary(Arc<dyn pmometadata::TrackMetadata + Send + Sync>),
    EndOfStream,
    ChannelClosed,
}

/// Attend et retourne le premier chunk audio avec les métadonnées du TrackBoundary si présent.
/// Retourne une erreur si EndOfStream est reçu avant tout audio.
async fn wait_for_first_audio_chunk_with_metadata(
    rx: &mut mpsc::Receiver<Arc<AudioSegment>>,
) -> Result<(Arc<AudioSegment>, Option<Arc<dyn pmometadata::TrackMetadata + Send + Sync>>), AudioError> {
    let mut track_metadata: Option<Arc<dyn pmometadata::TrackMetadata + Send + Sync>> = None;

    loop {
        let segment = rx
            .recv()
            .await
            .ok_or_else(|| AudioError::ProcessingError("No audio data received".into()))?;

        match &segment.segment {
            crate::_AudioSegment::Chunk(chunk) => {
                if chunk.len() == 0 {
                    return Err(AudioError::ProcessingError("Received empty chunk".into()));
                }
                return Ok((segment, track_metadata));
            }
            crate::_AudioSegment::Sync(marker) => {
                match **marker {
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
                }
            }
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
            crate::_AudioSegment::Chunk(chunk) => {
                // Vérifier la cohérence du sample rate
                if chunk.sample_rate() != expected_rate {
                    return Err(AudioError::ProcessingError(format!(
                        "FlacFileSink: inconsistent sample rate ({} vs {})",
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
            crate::_AudioSegment::Sync(marker) => {
                match &**marker {
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
                }
            }
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
                "FlacFileSink only supports integer audio chunks (I16, I24, I32)".into(),
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
    pub path: PathBuf,
    pub track_number: usize,
    pub chunks_received: u64,
    pub total_samples: u64,
    pub total_duration_sec: f64,
}

/// Statistiques produites par le `FlacFileSink`.
#[derive(Debug, Clone)]
pub struct FlacFileSinkStats {
    pub tracks: Vec<TrackStats>,
}

impl TypedAudioNode for FlacFileSink {
    fn input_type(&self) -> Option<TypeRequirement> {
        // FlacFileSink accepte n'importe quel type entier (I16, I24, I32)
        // mais rejette les chunks flottants
        Some(TypeRequirement::any_integer())
    }

    fn output_type(&self) -> Option<TypeRequirement> {
        // FlacFileSink est un sink, il ne produit pas d'audio
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pmoflac::{decode_flac_stream, AudioFileMetadata};
    use pmometadata::{MemoryTrackMetadata, TrackMetadata};
    use tokio::io::AsyncReadExt;

    #[tokio::test]
    async fn test_flac_file_sink_writes_metadata() {

        let temp_dir = tempfile::tempdir().unwrap();
        let output_path = temp_dir.path().join("output_with_metadata.flac");

        let sample_rate = 44_100;
        let frames = 256;

        // Créer le sink
        let (sink, tx) = FlacFileSink::with_channel_size(&output_path, 16);
        let sink_handle = tokio::spawn(async move { sink.run().await.unwrap() });

        // Envoyer des segments avec métadonnées
        tokio::spawn(async move {
            // TopZeroSync
            tx.send(crate::AudioSegment::new_top_zero_sync())
                .await
                .unwrap();

            // TrackBoundary avec métadonnées
            let mut metadata = MemoryTrackMetadata::new();
            metadata.set_title(Some("Test Track Title".to_string())).await.unwrap();
            metadata.set_artist(Some("Test Artist".to_string())).await.unwrap();
            metadata.set_album(Some("Test Album".to_string())).await.unwrap();
            metadata.set_year(Some(2024)).await.unwrap();

            let track_boundary =
                crate::AudioSegment::new_track_boundary(0, 0.0, std::sync::Arc::new(metadata));
            tx.send(track_boundary).await.unwrap();

            // Générer et envoyer des chunks audio
            let chunk_frames = 64;
            let mut order = 0u64;
            let mut total_frames = 0u64;

            for chunk_start in (0..frames).step_by(chunk_frames) {
                let chunk_len = (frames - chunk_start).min(chunk_frames);
                let mut stereo = Vec::with_capacity(chunk_len);

                for i in 0..chunk_len {
                    let frame_idx = chunk_start + i;
                    let sample = ((frame_idx % 32) as f32 / 31.0 * 2.0 - 1.0) * 0.5;
                    let sample_i16 = (sample * 32767.0) as i16;
                    stereo.push([sample_i16, sample_i16]);
                }

                let timestamp = total_frames as f64 / sample_rate as f64;
                let chunk_data = crate::AudioChunkData::new(stereo, sample_rate, 0.0);
                let chunk = crate::AudioChunk::I16(chunk_data);
                let segment = crate::AudioSegment {
                    order,
                    timestamp_sec: timestamp,
                    segment: crate::_AudioSegment::Chunk(std::sync::Arc::new(chunk)),
                };

                tx.send(std::sync::Arc::new(segment)).await.unwrap();
                total_frames += chunk_len as u64;
                order += 1;
            }

            // EndOfStream
            let final_timestamp = total_frames as f64 / sample_rate as f64;
            tx.send(crate::AudioSegment::new_end_of_stream(
                order,
                final_timestamp,
            ))
            .await
            .unwrap();

            drop(tx);
        });

        sink_handle.await.unwrap();

        // Vérifier que le fichier a été créé et contient les métadonnées
        assert!(output_path.exists(), "Output file should exist");

        // Lire les métadonnées du fichier FLAC généré
        let file_metadata = AudioFileMetadata::from_file(&output_path).unwrap();

        // Vérifier que les métadonnées ont été correctement écrites
        assert_eq!(file_metadata.title, Some("Test Track Title".to_string()));
        assert_eq!(file_metadata.artist, Some("Test Artist".to_string()));
        assert_eq!(file_metadata.album, Some("Test Album".to_string()));
        assert_eq!(file_metadata.year, Some(2024));
    }

    #[tokio::test]
    async fn test_flac_file_sink_writes_audio() {
        use pmoflac::{encode_flac_stream, EncoderOptions, PcmFormat};
        use std::io::Cursor;

        let temp_dir = tempfile::tempdir().unwrap();
        let input_path = temp_dir.path().join("input.flac");
        let output_path = temp_dir.path().join("output.flac");

        // Créer un petit fichier FLAC de test (comme dans file_source test)
        let sample_rate = 44_100;
        let frames = 512;
        let mut pcm = Vec::with_capacity(frames * 4);
        for i in 0..frames {
            let sample = ((i % 32) as f32 / 31.0 * 2.0 - 1.0) * 0.5;
            let sample_i16 = (sample * 32767.0) as i16;
            pcm.extend_from_slice(&sample_i16.to_le_bytes());
            pcm.extend_from_slice(&sample_i16.to_le_bytes());
        }

        let format = PcmFormat {
            sample_rate,
            channels: 2,
            bits_per_sample: 16,
        };

        let mut flac_stream =
            encode_flac_stream(Cursor::new(pcm.clone()), format, EncoderOptions::default())
                .await
                .unwrap();

        let mut input_file = File::create(&input_path).await.unwrap();
        tokio::io::copy(&mut flac_stream, &mut input_file)
            .await
            .unwrap();
        input_file.flush().await.unwrap();
        flac_stream.wait().await.unwrap();

        // Maintenant utiliser FlacFileSink pour réécrire le fichier
        let (sink, tx) = FlacFileSink::with_channel_size(&output_path, 16);
        let sink_handle = tokio::spawn(async move { sink.run().await.unwrap() });

        // Lire le fichier input et envoyer les segments au sink
        tokio::spawn(async move {
            let source_file = File::open(&input_path).await.unwrap();
            let mut decode_stream = pmoflac::decode_audio_stream(source_file).await.unwrap();
            let info = decode_stream.info().clone();

            // TopZeroSync
            tx.send(crate::AudioSegment::new_top_zero_sync())
                .await
                .unwrap();

            // Lire et envoyer les chunks
            let mut buffer = vec![0u8; info.bytes_per_sample() * info.channels as usize * 256];
            let mut total_frames = 0u64;
            let mut order = 0u64;

            loop {
                let read = decode_stream.read(&mut buffer).await.unwrap();
                if read == 0 {
                    break;
                }

                let chunk_frames = read / (info.bytes_per_sample() * info.channels as usize);
                let timestamp = total_frames as f64 / info.sample_rate as f64;

                // Créer un segment I16
                let mut stereo = Vec::with_capacity(chunk_frames);
                for i in 0..chunk_frames {
                    let offset = i * info.bytes_per_sample() * info.channels as usize;
                    let l = i16::from_le_bytes([buffer[offset], buffer[offset + 1]]);
                    let r = i16::from_le_bytes([buffer[offset + 2], buffer[offset + 3]]);
                    stereo.push([l, r]);
                }

                let chunk_data = crate::AudioChunkData::new(stereo, info.sample_rate, 0.0);
                let chunk = crate::AudioChunk::I16(chunk_data);
                let segment = crate::AudioSegment {
                    order,
                    timestamp_sec: timestamp,
                    segment: crate::_AudioSegment::Chunk(std::sync::Arc::new(chunk)),
                };

                tx.send(std::sync::Arc::new(segment)).await.unwrap();
                total_frames += chunk_frames as u64;
                order += 1;
            }

            // EndOfStream
            let final_timestamp = total_frames as f64 / info.sample_rate as f64;
            tx.send(crate::AudioSegment::new_end_of_stream(
                order,
                final_timestamp,
            ))
            .await
            .unwrap();

            drop(tx);
            decode_stream.wait().await.unwrap();
        });

        let stats = sink_handle.await.unwrap();
        assert_eq!(stats.tracks.len(), 1);
        assert!(stats.tracks[0].chunks_received > 0);
        assert_eq!(stats.tracks[0].total_samples, frames as u64);

        // Vérifier que le fichier de sortie est valide
        let file = File::open(&output_path).await.unwrap();
        let mut stream = decode_flac_stream(file).await.unwrap();
        let info = stream.info().clone();
        assert_eq!(info.channels, 2);
        assert_eq!(info.sample_rate, sample_rate);
        assert_eq!(info.bits_per_sample, 16);

        let mut decoded = Vec::new();
        stream.read_to_end(&mut decoded).await.unwrap();
        stream.wait().await.unwrap();
        assert!(decoded.len() > 0);
    }
}
