use crate::{
    nodes::{AudioError, TypedAudioNode, DEFAULT_CHUNK_DURATION_MS},
    pipeline::{send_to_children, AudioPipelineNode, Node, NodeLogic},
    type_constraints::TypeRequirement,
    AudioChunk, AudioChunkData, AudioSegment, I24,
};
use pmoflac::{decode_audio_stream, AudioFileMetadata, StreamInfo};
use pmometadata::{MemoryTrackMetadata, TrackMetadata};
use std::{path::PathBuf, sync::Arc, time::Duration};
use tokio::{fs::File, io::AsyncReadExt, sync::mpsc};
use tokio_util::sync::CancellationToken;
use tracing;

// ═══════════════════════════════════════════════════════════════════════════
// NOUVELLE ARCHITECTURE - FileSourceLogic
// ═══════════════════════════════════════════════════════════════════════════

/// Logique pure de lecture de fichier audio
///
/// Contient seulement la logique de décodage et d'envoi des segments,
/// sans la plomberie d'orchestration (gérée par Node<FileSourceLogic>).
pub struct FileSourceLogic {
    path: PathBuf,
    chunk_frames: usize,
}

impl FileSourceLogic {
    pub fn new<P: Into<PathBuf>>(path: P, chunk_frames: usize) -> Self {
        Self {
            path: path.into(),
            chunk_frames,
        }
    }
}

#[async_trait::async_trait]
impl NodeLogic for FileSourceLogic {
    async fn process(
        &mut self,
        _input: Option<mpsc::Receiver<Arc<AudioSegment>>>,
        output: Vec<mpsc::Sender<Arc<AudioSegment>>>,
        stop_token: CancellationToken,
    ) -> Result<(), AudioError> {
        tracing::debug!(
            "FileSourceLogic::process started, path={:?}, {} children",
            self.path,
            output.len()
        );

        // Ouvrir le fichier
        let file = File::open(&self.path)
            .await
            .map_err(|e| AudioError::IoError(format!("Failed to open {:?}: {}", self.path, e)))?;

        // Décoder le flux audio
        let mut stream = decode_audio_stream(file)
            .await
            .map_err(|e| AudioError::ProcessingError(format!("Decode error: {}", e)))?;
        let stream_info = stream.info().clone();

        validate_stream(&stream_info)?;

        // Calculer la taille des chunks si non spécifiée (0 = auto)
        let chunk_frames = if self.chunk_frames == 0 {
            let frames =
                (stream_info.sample_rate as f64 * DEFAULT_CHUNK_DURATION_MS / 1000.0) as usize;
            frames.next_power_of_two().max(256)
        } else {
            self.chunk_frames.max(1)
        };

        // Émettre TopZeroSync
        let top_zero = AudioSegment::new_top_zero_sync();
        send_to_children(std::any::type_name::<Self>(), &output, top_zero).await?;

        // Extraire et émettre les métadonnées du fichier
        if let Ok(file_metadata) = AudioFileMetadata::from_file(&self.path) {
            let mut metadata = MemoryTrackMetadata::new();
            if let Some(title) = file_metadata.title {
                let _ = metadata.set_title(Some(title)).await;
            }
            if let Some(artist) = file_metadata.artist {
                let _ = metadata.set_artist(Some(artist)).await;
            }
            if let Some(album) = file_metadata.album {
                let _ = metadata.set_album(Some(album)).await;
            }
            if let Some(year) = file_metadata.year {
                let _ = metadata.set_year(Some(year)).await;
            }
            if let Some(duration_secs) = file_metadata.duration_secs {
                let _ = metadata
                    .set_duration(Some(Duration::from_secs(duration_secs)))
                    .await;
            }

            let track_boundary = AudioSegment::new_track_boundary(
                0,
                0.0,
                Arc::new(tokio::sync::RwLock::new(metadata)),
            );
            send_to_children(std::any::type_name::<Self>(), &output, track_boundary).await?;
        }

        // Préparer la lecture des chunks audio
        let frame_bytes = stream_info.bytes_per_sample() * stream_info.channels as usize;
        let chunk_byte_len = chunk_frames * frame_bytes;
        let mut pending = Vec::new();
        let mut read_buf = vec![0u8; frame_bytes * 512.max(chunk_frames)];
        let mut chunk_index = 0u64;
        let mut total_frames = 0u64;

        // Lire et émettre les chunks audio
        loop {
            tokio::select! {
                _ = stop_token.cancelled() => {
                    tracing::info!("FileSourceLogic: stop requested");
                    break;
                }

                read_result = stream.read(&mut read_buf) => {
                    // Remplir le buffer
                    if pending.len() < chunk_byte_len {
                        let read = read_result.map_err(|e| {
                            AudioError::IoError(format!("I/O error while decoding: {}", e))
                        })?;
                        if read == 0 && pending.is_empty() {
                            break;
                        }
                        if read > 0 {
                            pending.extend_from_slice(&read_buf[..read]);
                        }
                    }

                    if pending.is_empty() {
                        break;
                    }

                    // Extraire un chunk
                    let frames_in_pending = pending.len() / frame_bytes;
                    let frames_to_emit = frames_in_pending.min(chunk_frames);
                    if frames_to_emit == 0 {
                        break;
                    }
                    let take_bytes = frames_to_emit * frame_bytes;
                    let chunk_bytes = pending.drain(..take_bytes).collect::<Vec<u8>>();

                    // Calculer le timestamp
                    let timestamp_sec = total_frames as f64 / stream_info.sample_rate as f64;

                    // Créer et envoyer le segment audio
                    let segment = bytes_to_segment(
                        &chunk_bytes,
                        &stream_info,
                        frames_to_emit,
                        chunk_index,
                        timestamp_sec,
                    )?;

                    send_to_children(std::any::type_name::<Self>(), &output, segment).await?;

                    chunk_index += 1;
                    total_frames += frames_to_emit as u64;
                }
            }
        }

        // Traiter le reste éventuel (moins qu'un chunk complet)
        if !pending.is_empty() {
            let frames = pending.len() / frame_bytes;
            if frames > 0 {
                let timestamp_sec = total_frames as f64 / stream_info.sample_rate as f64;
                let segment =
                    bytes_to_segment(&pending, &stream_info, frames, chunk_index, timestamp_sec)?;
                send_to_children(std::any::type_name::<Self>(), &output, segment).await?;
                total_frames += frames as u64;
                chunk_index += 1;
            }
        }

        // Émettre EndOfStream
        let final_timestamp = total_frames as f64 / stream_info.sample_rate as f64;
        let eos = AudioSegment::new_end_of_stream(chunk_index, final_timestamp);
        send_to_children(std::any::type_name::<Self>(), &output, eos).await?;

        // Attendre la fin du décodage
        stream
            .wait()
            .await
            .map_err(|e| AudioError::ProcessingError(format!("Decode task failed: {}", e)))?;

        Ok(())
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// WRAPPER FileSource - Délègue à Node<FileSourceLogic>
// ═══════════════════════════════════════════════════════════════════════════════

/// FileSource - Lit un fichier audio et publie des `AudioSegment`
///
/// Cette source utilise `pmoflac` pour décoder le fichier (FLAC/MP3/OGG/WAV/AIFF)
/// puis transforme les échantillons PCM en `AudioSegment` stéréo avec le type approprié
/// (I16, I24, ou I32) selon la profondeur de bit du fichier source.
///
/// Le node émet trois types de syncmarkers :
/// - `TopZeroSync` au début du flux
/// - `TrackBoundary` avec les métadonnées du fichier
/// - `EndOfStream` à la fin du flux
///
/// # Architecture
///
/// Utilise la nouvelle architecture avec `Node<FileSourceLogic>` pour séparer
/// la logique métier (décodage) de la plomberie (spawning, monitoring).
pub struct FileSource {
    inner: Node<FileSourceLogic>,
}

impl FileSource {
    /// Crée une nouvelle source de fichier avec calcul automatique de la taille des chunks.
    ///
    /// La taille des chunks sera calculée automatiquement pour obtenir environ 50ms
    /// de latence par chunk, en fonction du sample rate du fichier.
    ///
    /// * `path` - chemin du fichier audio à lire
    pub fn new<P: Into<PathBuf>>(path: P) -> Self {
        Self::with_chunk_size(path, 0) // 0 = auto-calculer
    }

    /// Crée une nouvelle source de fichier avec une taille de chunk spécifique.
    ///
    /// * `path` - chemin du fichier audio à lire
    /// * `chunk_frames` - nombre d'échantillons par canal par chunk (0 = auto)
    pub fn with_chunk_size<P: Into<PathBuf>>(path: P, chunk_frames: usize) -> Self {
        let logic = FileSourceLogic::new(path, chunk_frames);
        Self {
            inner: Node::new_source(logic),
        }
    }
}

#[async_trait::async_trait]
impl AudioPipelineNode for FileSource {
    fn get_tx(&self) -> Option<mpsc::Sender<Arc<AudioSegment>>> {
        self.inner.get_tx()
    }

    fn register(&mut self, child: Box<dyn AudioPipelineNode>) {
        self.inner.register(child)
    }

    async fn run(self: Box<Self>, stop_token: CancellationToken) -> Result<(), AudioError> {
        Box::new(self.inner).run(stop_token).await
    }
}

fn validate_stream(info: &StreamInfo) -> Result<(), AudioError> {
    if !(1..=2).contains(&info.channels) {
        return Err(AudioError::ProcessingError(format!(
            "Unsupported channel count: {}",
            info.channels
        )));
    }
    match info.bits_per_sample {
        8 | 16 | 24 | 32 => Ok(()),
        other => Err(AudioError::ProcessingError(format!(
            "Unsupported bit depth: {}",
            other
        ))),
    }
}

/// Convertit des bytes PCM en AudioSegment avec le type approprié
fn bytes_to_segment(
    chunk_bytes: &[u8],
    info: &StreamInfo,
    frames: usize,
    order: u64,
    timestamp_sec: f64,
) -> Result<Arc<AudioSegment>, AudioError> {
    let bytes_per_sample = info.bytes_per_sample();
    let channels = info.channels as usize;
    let frame_bytes = bytes_per_sample * channels;

    // Créer le chunk du bon type selon la profondeur de bit
    let chunk = match info.bits_per_sample {
        16 => {
            // Type I16
            let mut stereo = Vec::with_capacity(frames);
            for frame_idx in 0..frames {
                let base = frame_idx * frame_bytes;
                let l = i16::from_le_bytes(
                    chunk_bytes[base..base + bytes_per_sample]
                        .try_into()
                        .unwrap(),
                );
                let r = if channels == 1 {
                    l
                } else {
                    i16::from_le_bytes(
                        chunk_bytes[base + bytes_per_sample..base + 2 * bytes_per_sample]
                            .try_into()
                            .unwrap(),
                    )
                };
                stereo.push([l, r]);
            }
            let chunk_data = AudioChunkData::new(stereo, info.sample_rate, 0.0);
            AudioChunk::I16(chunk_data)
        }
        24 => {
            // Type I24
            let mut stereo = Vec::with_capacity(frames);
            for frame_idx in 0..frames {
                let base = frame_idx * frame_bytes;
                let l_i32 = {
                    let mut buf = [0u8; 4];
                    buf[..3].copy_from_slice(&chunk_bytes[base..base + 3]);
                    // Sign extend
                    if chunk_bytes[base + 2] & 0x80 != 0 {
                        buf[3] = 0xFF;
                    }
                    i32::from_le_bytes(buf)
                };
                let l = I24::new(l_i32).ok_or_else(|| {
                    AudioError::ProcessingError(format!("Invalid I24 value: {}", l_i32))
                })?;

                let r = if channels == 1 {
                    l
                } else {
                    let r_i32 = {
                        let mut buf = [0u8; 4];
                        buf[..3].copy_from_slice(
                            &chunk_bytes[base + bytes_per_sample..base + bytes_per_sample + 3],
                        );
                        // Sign extend
                        if chunk_bytes[base + bytes_per_sample + 2] & 0x80 != 0 {
                            buf[3] = 0xFF;
                        }
                        i32::from_le_bytes(buf)
                    };
                    I24::new(r_i32).ok_or_else(|| {
                        AudioError::ProcessingError(format!("Invalid I24 value: {}", r_i32))
                    })?
                };
                stereo.push([l, r]);
            }
            let chunk_data = AudioChunkData::new(stereo, info.sample_rate, 0.0);
            AudioChunk::I24(chunk_data)
        }
        32 => {
            // Type I32
            let mut stereo = Vec::with_capacity(frames);
            for frame_idx in 0..frames {
                let base = frame_idx * frame_bytes;
                let l = i32::from_le_bytes(
                    chunk_bytes[base..base + bytes_per_sample]
                        .try_into()
                        .unwrap(),
                );
                let r = if channels == 1 {
                    l
                } else {
                    i32::from_le_bytes(
                        chunk_bytes[base + bytes_per_sample..base + 2 * bytes_per_sample]
                            .try_into()
                            .unwrap(),
                    )
                };
                stereo.push([l, r]);
            }
            let chunk_data = AudioChunkData::new(stereo, info.sample_rate, 0.0);
            AudioChunk::I32(chunk_data)
        }
        other => {
            return Err(AudioError::ProcessingError(format!(
                "Unsupported bit depth: {}",
                other
            )))
        }
    };

    // Créer le segment audio
    Ok(Arc::new(AudioSegment {
        order,
        timestamp_sec,
        segment: crate::_AudioSegment::Chunk(Arc::new(chunk)),
    }))
}

impl TypedAudioNode for FileSource {
    fn input_type(&self) -> Option<TypeRequirement> {
        // FileSource est une source, elle ne consomme pas d'audio
        None
    }

    fn output_type(&self) -> Option<TypeRequirement> {
        // FileSource peut produire n'importe quel type entier (I16, I24, I32)
        // selon la profondeur de bit du fichier source
        Some(TypeRequirement::any_integer())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pmoflac::{encode_flac_stream, EncoderOptions, PcmFormat};
    use std::io::Cursor;
    use tokio::io::AsyncWriteExt;
    use tokio::sync::mpsc;

    #[tokio::test]
    async fn test_file_source_decodes_flac() {
        let temp_dir = tempfile::tempdir().unwrap();
        let flac_path = temp_dir.path().join("test.flac");

        let sample_rate = 48_000;
        let frames = 256;
        let mut pcm = Vec::with_capacity(frames * 4);
        for i in 0..frames {
            let sample = ((i % 32) as f32 / 31.0 * 2.0 - 1.0) * 0.5; // simple ramp
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

        let mut file = File::create(&flac_path).await.expect("create flac file");
        tokio::io::copy(&mut flac_stream, &mut file)
            .await
            .expect("write flac");
        file.flush().await.expect("flush file");
        flac_stream.wait().await.unwrap();

        // Créer un collecteur simple qui transmet les segments à un channel de test
        struct TestCollectorNode {
            tx: mpsc::Sender<Arc<AudioSegment>>,
            rx: mpsc::Receiver<Arc<AudioSegment>>,
            test_tx: mpsc::Sender<Arc<AudioSegment>>,
        }

        impl TestCollectorNode {
            fn new(test_tx: mpsc::Sender<Arc<AudioSegment>>) -> Self {
                let (tx, rx) = mpsc::channel(16);
                Self { tx, rx, test_tx }
            }
        }

        #[async_trait::async_trait]
        impl AudioPipelineNode for TestCollectorNode {
            fn get_tx(&self) -> Option<mpsc::Sender<Arc<AudioSegment>>> {
                Some(self.tx.clone())
            }

            fn register(&mut self, _child: Box<dyn AudioPipelineNode>) {
                panic!("TestCollectorNode is a terminal node");
            }

            async fn run(
                mut self: Box<Self>,
                stop_token: CancellationToken,
            ) -> Result<(), AudioError> {
                // Transférer tous les segments au test
                loop {
                    tokio::select! {
                        _ = stop_token.cancelled() => {
                            break;
                        }
                        segment = self.rx.recv() => {
                            match segment {
                                Some(seg) => {
                                    if self.test_tx.send(seg).await.is_err() {
                                        break;
                                    }
                                }
                                None => break,
                            }
                        }
                    }
                }
                Ok(())
            }
        }

        let (test_tx, mut rx) = mpsc::channel(1024);
        let mut source = FileSource::with_chunk_size(&flac_path, 64);
        let collector = TestCollectorNode::new(test_tx);
        source.register(Box::new(collector));

        tokio::spawn(async move {
            let token = CancellationToken::new();
            Box::new(source).run(token).await.unwrap();
        });

        let mut received_frames = 0usize;
        let mut received_syncmarkers = 0usize;
        let mut seen_top_zero = false;
        let mut seen_eos = false;

        while let Some(segment) = rx.recv().await {
            if segment.is_audio_chunk() {
                if let Some(chunk) = segment.as_chunk() {
                    received_frames += chunk.len();
                    assert_eq!(chunk.sample_rate(), sample_rate);
                }
            } else {
                received_syncmarkers += 1;
                if let Some(marker) = segment.as_sync_marker() {
                    match **marker {
                        crate::SyncMarker::TopZeroSync => seen_top_zero = true,
                        crate::SyncMarker::EndOfStream => seen_eos = true,
                        crate::SyncMarker::TrackBoundary { .. } => {}
                        _ => {}
                    }
                }
            }
        }

        // Vérifier que tous les frames ont été reçus
        assert_eq!(received_frames, frames);
        // Vérifier qu'on a bien reçu des syncmarkers
        assert!(received_syncmarkers >= 2); // Au moins TopZeroSync et EndOfStream
        assert!(seen_top_zero, "Should have received TopZeroSync");
        assert!(seen_eos, "Should have received EndOfStream");
    }
}
