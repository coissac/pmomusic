//! PlaylistSource - Source audio depuis une playlist pmoplaylist
//!
//! Cette source lit une playlist (via `ReadHandle`) et émet un flux audio
//! continu en décodant les fichiers depuis le cache audio.
//!
//! # ⚠️ Format de sortie hétérogène
//!
//! **IMPORTANT** : Cette source émet du PCM avec des caractéristiques
//! **variables** selon les fichiers sources :
//! - **Sample rate** : peut varier (44.1kHz, 48kHz, 96kHz, etc.)
//! - **Bit depth** : peut varier (I16, I24, I32)
//!
//! Pour obtenir un flux **homogène**, ajoutez les nœuds suivants dans le pipeline :
//! - `ResamplingNode` : normalise le sample_rate (à implémenter dans pmoaudio)
//! - `ToI24Node` / `ToI16Node` : normalise la profondeur de bits
//!
//! # Cas d'usage
//!
//! ## Radio Paradise (format homogène connu)
//! ```rust,no_run
//! use pmoaudio_ext::PlaylistSource;
//! use pmoaudio::ToI24Node;
//! use pmoplaylist::PlaylistManager;
//! use pmoaudiocache::AudioCache;
//! use std::sync::Arc;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let manager = PlaylistManager::get();
//! let read_handle = manager.get_read_handle("radio-paradise").await?;
//! let cache = Arc::new(AudioCache::new("./cache", 500)?);
//!
//! let mut source = PlaylistSource::new(read_handle, cache);
//! let to_i24 = ToI24Node::new();
//! source.register(to_i24);
//! # Ok(())
//! # }
//! ```
//!
//! ## Playlist mixte (nécessite homogénéisation)
//! ```rust,no_run
//! use pmoaudio_ext::PlaylistSource;
//! use pmoaudio::{ToI24Node, ResamplingNode};
//! # use pmoplaylist::PlaylistManager;
//! # use pmoaudiocache::AudioCache;
//! # use std::sync::Arc;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! # let manager = PlaylistManager::get();
//! # let read_handle = manager.get_read_handle("mixed").await?;
//! # let cache = Arc::new(AudioCache::new("./cache", 500)?);
//! let mut source = PlaylistSource::new(read_handle, cache);
//! let mut resampler = ResamplingNode::new(48000);  // Force 48kHz
//! let to_i24 = ToI24Node::new();                   // Force I24
//! source.register(Box::new(resampler));
//! resampler.register(Box::new(to_i24));
//! # Ok(())
//! # }
//! ```
//!
//! # Comportement
//!
//! - **Polling** : Si la playlist est vide, attend `poll_interval_ms` avant de réessayer
//! - **TrackBoundary** : Émet un marqueur avec metadata entre chaque piste
//! - **Erreurs** : Si un fichier est inaccessible, émet un `Error` marker et continue
//! - **Arrêt** : Via `CancellationToken`, émet `EndOfStream` avant de terminer
//!
//! # Synchronisation
//!
//! - `TopZeroSync` : émis une seule fois au début
//! - `TrackBoundary` : émis avant chaque nouvelle piste (contient metadata)
//! - Pas d'`EndOfStream` entre les pistes (flux continu)
//! - `EndOfStream` final uniquement lors de l'arrêt

use pmoaudio::{
    nodes::{AudioError, Node, NodeLogic, TypedAudioNode, DEFAULT_CHUNK_DURATION_MS},
    pipeline::AudioPipelineNode,
    type_constraints::TypeRequirement,
    AudioChunk, AudioChunkData, AudioSegment, I24,
};
use pmoaudiocache::AudioCache;
use pmoflac::{decode_audio_stream, StreamInfo};
use pmoplaylist::ReadHandle;
use std::{path::PathBuf, sync::Arc, time::Duration};
use tokio::{fs::File, io::AsyncReadExt, sync::mpsc};
use tokio_util::sync::CancellationToken;
use tracing;

// ═══════════════════════════════════════════════════════════════════════════
// PlaylistSourceLogic - Logique pure de lecture de playlist
// ═══════════════════════════════════════════════════════════════════════════

/// Logique pure de lecture de playlist
///
/// Contient seulement la logique de lecture de playlist et décodage des pistes,
/// sans la plomberie d'orchestration (gérée par Node<PlaylistSourceLogic>).
pub struct PlaylistSourceLogic {
    playlist_handle: ReadHandle,
    cache: Arc<AudioCache>,
    chunk_frames: usize,
    poll_interval_ms: u64,
}

impl PlaylistSourceLogic {
    pub fn new(
        playlist_handle: ReadHandle,
        cache: Arc<AudioCache>,
        chunk_frames: usize,
        poll_interval_ms: u64,
    ) -> Self {
        Self {
            playlist_handle,
            cache,
            chunk_frames,
            poll_interval_ms,
        }
    }
}

#[async_trait::async_trait]
impl NodeLogic for PlaylistSourceLogic {
    async fn process(
        &mut self,
        _input: Option<mpsc::Receiver<Arc<AudioSegment>>>,
        output: Vec<mpsc::Sender<Arc<AudioSegment>>>,
        stop_token: CancellationToken,
    ) -> Result<(), AudioError> {
        tracing::debug!(
            "PlaylistSourceLogic::process started, playlist={}, {} children",
            self.playlist_handle.id(),
            output.len()
        );

        // Macro helper pour envoyer à tous les enfants
        macro_rules! send_to_children {
            ($segment:expr) => {
                for tx in &output {
                    tx.send($segment.clone())
                        .await
                        .map_err(|_| AudioError::ChildDied)?;
                }
            };
        }

        let mut first_track = true;

        loop {
            // Vérifier arrêt immédiat
            if stop_token.is_cancelled() {
                tracing::info!("PlaylistSourceLogic: stop requested, emitting EndOfStream");
                let eos = AudioSegment::new_end_of_stream(0, 0.0);
                send_to_children!(eos);
                break;
            }

            // Pop avec timeout pour supporter stop_token
            let track = tokio::select! {
                _ = stop_token.cancelled() => {
                    tracing::info!("PlaylistSourceLogic: stop cancelled during pop");
                    let eos = AudioSegment::new_end_of_stream(0, 0.0);
                    send_to_children!(eos);
                    break;
                }
                result = self.playlist_handle.pop() => {
                    match result {
                        Ok(Some(t)) => {
                            tracing::debug!("PlaylistSourceLogic: popped track from playlist");
                            t
                        },
                        Ok(None) => {
                            // Playlist vide, attendre avant retry
                            tracing::trace!(
                                "PlaylistSourceLogic: playlist empty, waiting {}ms",
                                self.poll_interval_ms
                            );
                            tokio::time::sleep(
                                Duration::from_millis(self.poll_interval_ms)
                            ).await;
                            continue;
                        }
                        Err(e) => {
                            // Erreur playlist (deleted, etc.)
                            tracing::warn!("PlaylistSourceLogic: playlist error: {}", e);
                            let error_marker = AudioSegment::new_error(
                                0,
                                0.0,
                                format!("Playlist error: {}", e)
                            );
                            send_to_children!(error_marker);
                            continue;
                        }
                    }
                }
            };

            // Émettre TopZeroSync pour la première piste seulement
            if first_track {
                tracing::debug!("PlaylistSourceLogic: emitting TopZeroSync");
                let top_zero = AudioSegment::new_top_zero_sync();
                send_to_children!(top_zero);
                first_track = false;
            }

            // Émettre TrackBoundary avec metadata du cache
            let metadata = match track.track_metadata() {
                Ok(m) => m,
                Err(e) => {
                    tracing::warn!("PlaylistSourceLogic: failed to get metadata: {}", e);
                    let error_marker = AudioSegment::new_error(
                        0,
                        0.0,
                        format!("Failed to get metadata: {}", e),
                    );
                    send_to_children!(error_marker);
                    continue;
                }
            };

            tracing::debug!("PlaylistSourceLogic: emitting TrackBoundary");
            let boundary = AudioSegment::new_track_boundary(0, 0.0, metadata);
            send_to_children!(boundary);

            // Obtenir le chemin du fichier
            let file_path = match track.file_path() {
                Ok(p) => p,
                Err(e) => {
                    tracing::warn!("PlaylistSourceLogic: failed to get file path: {}", e);
                    let error_marker = AudioSegment::new_error(
                        0,
                        0.0,
                        format!("Failed to get file path: {}", e),
                    );
                    send_to_children!(error_marker);
                    continue;
                }
            };

            tracing::debug!("PlaylistSourceLogic: decoding track: {:?}", file_path);

            // Décoder et émettre les chunks PCM
            if let Err(e) = decode_and_emit_track(
                &file_path,
                self.chunk_frames,
                &output,
                &stop_token,
            )
            .await
            {
                tracing::error!("PlaylistSourceLogic: error decoding track: {}", e);
                let error_marker = AudioSegment::new_error(0, 0.0, format!("Decode error: {}", e));
                send_to_children!(error_marker);
                // Continue vers la piste suivante
            }

            // Boucler pour la piste suivante (pas d'EndOfStream entre pistes !)
        }

        tracing::debug!("PlaylistSourceLogic::process finished");
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Helper Functions
// ═══════════════════════════════════════════════════════════════════════════

/// Décode un fichier et émet ses chunks audio
async fn decode_and_emit_track(
    path: &PathBuf,
    chunk_frames: usize,
    output: &[mpsc::Sender<Arc<AudioSegment>>],
    stop_token: &CancellationToken,
) -> Result<(), AudioError> {
    // Ouvrir et décoder
    let file = File::open(path)
        .await
        .map_err(|e| AudioError::IoError(format!("Failed to open {:?}: {}", path, e)))?;

    let mut stream = decode_audio_stream(file)
        .await
        .map_err(|e| AudioError::ProcessingError(format!("Decode error: {}", e)))?;

    let stream_info = stream.info().clone();

    // Valider le stream
    validate_stream(&stream_info)?;

    // Calculer chunk_frames (auto = 50ms)
    let chunk_frames = if chunk_frames == 0 {
        let frames = (stream_info.sample_rate as f64 * DEFAULT_CHUNK_DURATION_MS / 1000.0) as usize;
        frames.next_power_of_two().max(256)
    } else {
        chunk_frames.max(1)
    };

    tracing::trace!(
        "decode_and_emit_track: sample_rate={}, bit_depth={}, chunk_frames={}",
        stream_info.sample_rate,
        stream_info.bits_per_sample,
        chunk_frames
    );

    // Lire et émettre les chunks
    let frame_bytes = stream_info.bytes_per_sample() * stream_info.channels as usize;
    let chunk_byte_len = chunk_frames * frame_bytes;
    let mut pending = Vec::new();
    let mut read_buf = vec![0u8; frame_bytes * 512.max(chunk_frames)];
    let mut chunk_index = 0u64;
    let mut total_frames = 0u64;

    loop {
        tokio::select! {
            _ = stop_token.cancelled() => {
                tracing::debug!("decode_and_emit_track: stop requested");
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

                for tx in output {
                    tx.send(segment.clone())
                        .await
                        .map_err(|_| AudioError::ChildDied)?;
                }

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
            let segment = bytes_to_segment(&pending, &stream_info, frames, chunk_index, timestamp_sec)?;
            for tx in output {
                tx.send(segment.clone())
                    .await
                    .map_err(|_| AudioError::ChildDied)?;
            }
        }
    }

    // Attendre la fin du décodage
    stream
        .wait()
        .await
        .map_err(|e| AudioError::ProcessingError(format!("Decode task failed: {}", e)))?;

    Ok(())
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
        _ => {
            return Err(AudioError::ProcessingError(format!(
                "Unsupported bit depth: {}",
                info.bits_per_sample
            )))
        }
    };

    Ok(Arc::new(AudioSegment {
        order,
        timestamp_sec,
        segment: pmoaudio::_AudioSegment::Chunk(Arc::new(chunk)),
    }))
}

// ═══════════════════════════════════════════════════════════════════════════
// WRAPPER PlaylistSource - Délègue à Node<PlaylistSourceLogic>
// ═══════════════════════════════════════════════════════════════════════════

/// PlaylistSource - Lit une playlist et publie des `AudioSegment`
///
/// Cette source utilise une playlist (`ReadHandle`) et le cache audio pour
/// décoder les pistes en continu. Le format de sortie (sample_rate et bit_depth)
/// est **hétérogène** et dépend des fichiers sources.
///
/// Voir la documentation du module pour plus de détails et exemples d'usage.
pub struct PlaylistSource {
    inner: Node<PlaylistSourceLogic>,
}

impl PlaylistSource {
    /// Crée une nouvelle source de playlist avec paramètres par défaut
    ///
    /// * `playlist_handle` - Handle de lecture sur la playlist
    /// * `cache` - Cache audio contenant les fichiers
    ///
    /// Paramètres par défaut :
    /// - `chunk_frames` : 0 (auto-calculé pour 50ms)
    /// - `poll_interval_ms` : 100ms
    pub fn new(playlist_handle: ReadHandle, cache: Arc<AudioCache>) -> Self {
        Self::with_config(playlist_handle, cache, 0, 100)
    }

    /// Crée une nouvelle source de playlist avec configuration personnalisée
    ///
    /// * `playlist_handle` - Handle de lecture sur la playlist
    /// * `cache` - Cache audio contenant les fichiers
    /// * `chunk_frames` - Nombre de frames par chunk (0 = auto)
    /// * `poll_interval_ms` - Intervalle de polling si playlist vide
    pub fn with_config(
        playlist_handle: ReadHandle,
        cache: Arc<AudioCache>,
        chunk_frames: usize,
        poll_interval_ms: u64,
    ) -> Self {
        let logic = PlaylistSourceLogic::new(playlist_handle, cache, chunk_frames, poll_interval_ms);
        Self {
            inner: Node::new_source(logic),
        }
    }
}

#[async_trait::async_trait]
impl AudioPipelineNode for PlaylistSource {
    fn get_tx(&self) -> Option<mpsc::Sender<Arc<AudioSegment>>> {
        self.inner.get_tx()
    }

    fn register(&mut self, child: Box<dyn AudioPipelineNode>) {
        self.inner.register(child)
    }

    async fn run(
        self: Box<Self>,
        stop_token: CancellationToken,
    ) -> Result<(), AudioError> {
        Box::new(self.inner).run(stop_token).await
    }
}

impl TypedAudioNode for PlaylistSource {
    fn input_type(&self) -> Option<TypeRequirement> {
        None // Source n'a pas d'entrée
    }

    fn output_type(&self) -> Option<TypeRequirement> {
        // Format hétérogène - accepte tout
        Some(TypeRequirement::any())
    }
}
