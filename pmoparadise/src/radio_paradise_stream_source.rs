//! RadioParadiseStreamSource - Node audio pmoaudio pour Radio Paradise
//!
//! Ce node télécharge et décode les blocs FLAC de Radio Paradise en streaming,
//! avec insertion automatique des TrackBoundary au bon timing.

use crate::{
    client::RadioParadiseClient,
    models::{Block, EventId, Song},
    node_stats::NodeStats,
};
use futures_util::StreamExt;
use pmoaudio::{
    nodes::{AudioError, TypedAudioNode, DEFAULT_CHUNK_DURATION_MS},
    pipeline::{Node, NodeLogic},
    type_constraints::TypeRequirement,
    AudioPipelineNode, AudioSegment, SyncMarker, I24,
};
use pmoflac::decode_audio_stream;
use pmometadata::{MemoryTrackMetadata, TrackMetadata};
use std::{
    collections::VecDeque,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::io::AsyncReadExt;
use tokio::sync::{mpsc, RwLock};
use tokio_util::{io::StreamReader, sync::CancellationToken};

/// Signal spécial pour indiquer qu'il n'y aura plus de blocs
/// Quand ce blockid est poussé dans la queue, le source termine proprement
/// après avoir fini de traiter le bloc en cours
pub const END_OF_BLOCKS_SIGNAL: EventId = EventId::MAX;

/// Nombre de blocs récents à mémoriser pour éviter les re-téléchargements
const RECENT_BLOCKS_CACHE_SIZE: usize = 10;

// ═══════════════════════════════════════════════════════════════════════════
// RadioParadiseStreamSourceLogic - Logique métier pure
// ═══════════════════════════════════════════════════════════════════════════

/// Logique pure de téléchargement et décodage des blocs Radio Paradise
pub struct RadioParadiseStreamSourceLogic {
    client: RadioParadiseClient,
    chunk_frames: usize,
    recent_blocks: VecDeque<EventId>,
    block_queue: VecDeque<EventId>,
    stats: Arc<NodeStats>,
}

impl RadioParadiseStreamSourceLogic {
    pub fn new(client: RadioParadiseClient, chunk_duration_ms: u32) -> Self {
        // Calculer chunk_frames pour la durée cible (on suppose 44.1kHz)
        let chunk_frames = ((chunk_duration_ms as f64 / 1000.0) * 44100.0) as usize;

        Self {
            client,
            chunk_frames,
            recent_blocks: VecDeque::with_capacity(RECENT_BLOCKS_CACHE_SIZE),
            block_queue: VecDeque::new(),
            stats: NodeStats::new("RadioParadiseStreamSource"),
        }
    }

    /// Ajoute un block ID à la file d'attente
    pub fn push_block_id(&mut self, event_id: EventId) {
        self.block_queue.push_back(event_id);
    }

    /// Vérifie si un bloc a été téléchargé récemment
    fn is_recent_block(&self, event_id: EventId) -> bool {
        self.recent_blocks.contains(&event_id)
    }

    /// Marque un bloc comme récemment téléchargé (FIFO)
    fn mark_block_downloaded(&mut self, event_id: EventId) {
        // Retirer tous les éléments excédentaires (garantit <= CACHE_SIZE)
        while self.recent_blocks.len() >= RECENT_BLOCKS_CACHE_SIZE {
            self.recent_blocks.pop_front();
        }

        // Puis ajouter le nouveau bloc
        self.recent_blocks.push_back(event_id);
    }

    /// Télécharge et décode un bloc FLAC
    /// Retourne (timestamp_final, instant_debut) pour permettre le timing correct
    async fn download_and_decode_block(
        &mut self,
        block: &Block,
        output: &[mpsc::Sender<Arc<AudioSegment>>],
        stop_token: &CancellationToken,
        order: &mut u64,
    ) -> Result<(f64, Instant), AudioError> {
        // Télécharger le FLAC
        tracing::info!(
            "Sending HTTP GET request for block FLAC (expected duration: {:.1}min, url: {})",
            block.length as f64 / 60000.0,
            block.url
        );
        let response = self.client.client
            .get(&block.url)
            .timeout(self.client.block_timeout)
            .send()
            .await
            .map_err(|e| AudioError::ProcessingError(format!("Block download failed: {}", e)))?;

        tracing::debug!("HTTP response received, status={}", response.status());
        if !response.status().is_success() {
            return Err(AudioError::ProcessingError(format!(
                "Block download returned status {}",
                response.status()
            )));
        }

        // Vérifier la taille du contenu si disponible
        if let Some(content_length) = response.content_length() {
            tracing::info!(
                "HTTP Content-Length: {} bytes ({:.1} MB)",
                content_length,
                content_length as f64 / 1_048_576.0
            );
        } else {
            tracing::warn!("HTTP response has no Content-Length header");
        }

        // Créer un stream reader
        tracing::debug!("Creating byte stream reader");
        let byte_stream = response.bytes_stream().map(|result| {
            result.map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
        });
        let stream_reader = StreamReader::new(byte_stream);
        tracing::debug!("Stream reader created");

        // Décoder le FLAC
        tracing::debug!("Decoding FLAC stream...");
        let mut decoder = decode_audio_stream(stream_reader)
            .await
            .map_err(|e| AudioError::ProcessingError(format!("FLAC decode failed: {}", e)))?;

        let stream_info = decoder.info().clone();
        let sample_rate = stream_info.sample_rate;
        let bits_per_sample = stream_info.bits_per_sample;
        tracing::debug!("FLAC decoder initialized: {}Hz, {} bits/sample", sample_rate, bits_per_sample);

        // Préparer les songs ordonnées pour tracking
        let songs = block.songs_ordered();
        let mut song_index = 0;
        let mut total_samples = 0u64;
        tracing::debug!("Block has {} songs", songs.len());

        // Noter l'instant de début AVANT d'envoyer TopZeroSync
        // Ceci permet de synchroniser la durée réelle du bloc
        let start_instant = Instant::now();

        // Envoyer TopZeroSync au début du bloc
        tracing::debug!("Sending TopZeroSync to {} outputs", output.len());
        let top_zero = Arc::new(AudioSegment {
            order: *order,
            timestamp_sec: 0.0,
            segment: pmoaudio::_AudioSegment::Sync(Arc::new(SyncMarker::TopZeroSync)),
        });
        self.send_to_children(output, top_zero).await?;
        tracing::debug!("TopZeroSync sent");

        // Envoyer TrackBoundary pour la première song AVANT le premier chunk audio
        // Même si son elapsed > 0, cela garantit que FlacCacheSink a des métadonnées
        // dès le début (sinon il attendrait indéfiniment un TrackBoundary)
        let mut next_song: Option<(usize, &Song)> = if let Some((idx, song)) = songs.get(0).copied() {
            tracing::debug!("Sending TrackBoundary for first song (idx={}, elapsed={}ms) at timestamp 0",
                idx, song.elapsed);
            let metadata = song_to_metadata(song, block).await;
            let track_boundary = AudioSegment::new_track_boundary(
                *order,
                0.0,  // timestamp = 0 au début du stream
                metadata,
            );
            self.send_to_children(output, track_boundary).await?;
            song_index = 1;
            // Le prochain TrackBoundary sera pour la deuxième song quand elapsed_ms >= song.elapsed
            songs.get(1).copied()
        } else {
            None
        };
        tracing::debug!("Starting audio chunk loop");


        // Buffer pour lecture
        let bytes_per_sample = (bits_per_sample / 8) as usize;
        let frame_bytes = bytes_per_sample * 2; // stereo
        let chunk_frames = self.chunk_frames;
        let chunk_byte_len = chunk_frames * frame_bytes;
        let mut read_buf = vec![0u8; chunk_byte_len * 2];
        let mut pending: Vec<u8> = Vec::with_capacity(chunk_byte_len * 2);

        // Traiter les chunks audio
        let mut chunk_count = 0;
        let mut total_bytes_decoded = 0u64;
        let expected_duration_sec = block.length as f64 / 1000.0;

        loop {
            // Vérifier stop_token
            if stop_token.is_cancelled() {
                // Retourner le timestamp actuel et start_instant si on est interrompu
                let current_timestamp = total_samples as f64 / sample_rate as f64;
                tracing::warn!(
                    "Block decode CANCELLED: sent {} chunks, {:.2}s duration ({:.1}% of expected {:.2}s), decoded {} bytes",
                    chunk_count, current_timestamp,
                    (current_timestamp / expected_duration_sec) * 100.0,
                    expected_duration_sec, total_bytes_decoded
                );
                return Ok((current_timestamp, start_instant));
            }

            // Remplir le buffer
            if pending.len() < chunk_byte_len {
                let read = decoder.read(&mut read_buf).await
                    .map_err(|e| AudioError::ProcessingError(format!("Read error: {}", e)))?;

                if read == 0 {
                    let actual_duration = total_samples as f64 / sample_rate as f64;
                    let percentage = (actual_duration / expected_duration_sec) * 100.0;

                    if percentage < 95.0 {
                        tracing::error!(
                            "FLAC decode EOF PREMATURE: sent {} chunks, {:.2}s actual vs {:.2}s expected ({:.1}%), decoded {} bytes",
                            chunk_count, actual_duration, expected_duration_sec, percentage, total_bytes_decoded
                        );
                    } else {
                        tracing::info!(
                            "FLAC decode EOF reached: sent {} chunks, {:.2}s duration ({:.1}% of expected), decoded {} bytes",
                            chunk_count, actual_duration, percentage, total_bytes_decoded
                        );
                    }
                    break; // EOF
                }
                total_bytes_decoded += read as u64;
                pending.extend_from_slice(&read_buf[..read]);
            }

            if pending.is_empty() {
                break;
            }

            // Extraire un chunk
            let frames_in_pending = pending.len() / frame_bytes;
            let frames_to_emit = frames_in_pending.min(chunk_frames);
            let take_bytes = frames_to_emit * frame_bytes;
            let pcm_data = pending.drain(..take_bytes).collect::<Vec<u8>>();

            // Calculer le nombre de frames (samples par canal)
            let bytes_per_sample = (bits_per_sample / 8) as usize;
            let chunk_len = (pcm_data.len() / (bytes_per_sample * 2)) as u64; // 2 = stereo

            // Vérifier si on doit insérer un TrackBoundary avant ce chunk
            if let Some((idx, song)) = next_song {
                let elapsed_ms = (total_samples * 1000) / sample_rate as u64;

                if elapsed_ms >= song.elapsed {
                    // Envoyer TrackBoundary AVANT le chunk (avec le même order)
                    tracing::debug!(
                        "Sending TrackBoundary for song {} at elapsed_ms={} (song.elapsed={}, timestamp_sec={:.2})",
                        idx, elapsed_ms, song.elapsed, (total_samples as f64 / sample_rate as f64)
                    );
                    let metadata = song_to_metadata(song, block).await;
                    let timestamp_sec = total_samples as f64 / sample_rate as f64;
                    let track_boundary = AudioSegment::new_track_boundary(
                        *order,
                        timestamp_sec,
                        metadata,
                    );
                    self.send_to_children(output, track_boundary).await?;

                    // Passer à la song suivante
                    song_index += 1;
                    next_song = songs.get(song_index).copied();
                    tracing::debug!("Moved to next song, song_index={}, next_song present={}", song_index, next_song.is_some());
                }
            }

            // Envoyer le chunk audio
            let timestamp_sec = total_samples as f64 / sample_rate as f64;
            let audio_segment = pcm_to_audio_segment(
                &pcm_data,
                *order,
                timestamp_sec,
                sample_rate,
                bits_per_sample,
            )?;
            self.send_to_children(output, audio_segment).await?;

            *order += 1;
            total_samples += chunk_len;
            chunk_count += 1;
        }

        // Retourner le timestamp du dernier chunk (durée totale du bloc) et l'instant de début
        let final_timestamp = total_samples as f64 / sample_rate as f64;
        tracing::debug!("Block decode complete: {} samples, {:.2}s duration", total_samples, final_timestamp);

        Ok((final_timestamp, start_instant))
    }

    /// Envoie un segment à tous les enfants
    async fn send_to_children(
        &self,
        output: &[mpsc::Sender<Arc<AudioSegment>>],
        segment: Arc<AudioSegment>,
    ) -> Result<(), AudioError> {
        self.stats.record_segment_received(segment.timestamp_sec);

        for (i, tx) in output.iter().enumerate() {
            let capacity_before = tx.capacity();
            tracing::trace!(
                "send_to_children: Sending to child {} (channel capacity={}, timestamp={:.3}s)",
                i, capacity_before, segment.timestamp_sec
            );

            let send_start = std::time::Instant::now();
            tx.send(segment.clone())
                .await
                .map_err(|_| AudioError::ChildDied)?;
            let send_duration = send_start.elapsed();

            if send_duration.as_millis() > 10 {
                let duration_ms = send_duration.as_millis() as u64;
                self.stats.record_backpressure(duration_ms);
                tracing::debug!(
                    "send_to_children: Send to child {} BLOCKED for {:.3}s (backpressure triggered, timestamp={:.3}s)",
                    i, send_duration.as_secs_f64(), segment.timestamp_sec
                );
            }

            // Estimer la taille du segment pour les stats (frames * 2 channels * bytes_per_sample)
            let segment_bytes = match &segment.segment {
                pmoaudio::_AudioSegment::Chunk(chunk) => {
                    // Approximation: frames * 2 (stereo) * 4 bytes (i32/f32)
                    chunk.len() * 2 * 4
                }
                _ => 0,
            };
            self.stats.record_segment_sent(segment_bytes);
        }
        Ok(())
    }
}

/// Convertit PCM bytes en AudioSegment
fn pcm_to_audio_segment(
    pcm_data: &[u8],
    order: u64,
    timestamp_sec: f64,
    sample_rate: u32,
    bits_per_sample: u8,
) -> Result<Arc<AudioSegment>, AudioError> {
    use pmoaudio::{AudioChunk, AudioChunkData, _AudioSegment};

    let bytes_per_sample = (bits_per_sample / 8) as usize;
    let channels = 2; // Stereo
    let frame_bytes = bytes_per_sample * channels;
    let frames = pcm_data.len() / frame_bytes;

    // Valider que la taille des données est correcte
    if pcm_data.len() % frame_bytes != 0 {
        return Err(AudioError::ProcessingError(format!(
            "Invalid PCM data size: {} bytes is not a multiple of frame size {} ({}bit, {} channels)",
            pcm_data.len(),
            frame_bytes,
            bits_per_sample,
            channels
        )));
    }

    let chunk = match bits_per_sample {
        16 => {
            // Type I16
            let mut stereo = Vec::with_capacity(frames);
            for frame_idx in 0..frames {
                let base = frame_idx * frame_bytes;
                let left = i16::from_le_bytes([pcm_data[base], pcm_data[base + 1]]);
                let right = i16::from_le_bytes([pcm_data[base + 2], pcm_data[base + 3]]);
                stereo.push([left, right]);
            }
            let chunk_data = AudioChunkData::new(stereo, sample_rate, 0.0);
            AudioChunk::I16(chunk_data)
        }
        24 => {
            // Type I24 avec sign extension correcte
            let mut stereo = Vec::with_capacity(frames);
            for frame_idx in 0..frames {
                let base = frame_idx * frame_bytes;

                // Left channel (bytes 0,1,2) avec sign extension
                let left_i32 = {
                    let mut buf = [0u8; 4];
                    buf[..3].copy_from_slice(&pcm_data[base..base + 3]);
                    // Sign extend si négatif
                    if pcm_data[base + 2] & 0x80 != 0 {
                        buf[3] = 0xFF;
                    }
                    i32::from_le_bytes(buf)
                };
                let left = I24::new(left_i32).ok_or_else(|| {
                    AudioError::ProcessingError(format!("Invalid I24 value: {}", left_i32))
                })?;

                // Right channel (bytes 3,4,5) avec sign extension
                let right_i32 = {
                    let mut buf = [0u8; 4];
                    buf[..3].copy_from_slice(&pcm_data[base + 3..base + 6]);
                    // Sign extend si négatif
                    if pcm_data[base + 5] & 0x80 != 0 {
                        buf[3] = 0xFF;
                    }
                    i32::from_le_bytes(buf)
                };
                let right = I24::new(right_i32).ok_or_else(|| {
                    AudioError::ProcessingError(format!("Invalid I24 value: {}", right_i32))
                })?;

                stereo.push([left, right]);
            }
            let chunk_data = AudioChunkData::new(stereo, sample_rate, 0.0);
            AudioChunk::I24(chunk_data)
        }
        32 => {
            // Type I32
            let mut stereo = Vec::with_capacity(frames);
            for frame_idx in 0..frames {
                let base = frame_idx * frame_bytes;
                let left = i32::from_le_bytes([
                    pcm_data[base],
                    pcm_data[base + 1],
                    pcm_data[base + 2],
                    pcm_data[base + 3],
                ]);
                let right = i32::from_le_bytes([
                    pcm_data[base + 4],
                    pcm_data[base + 5],
                    pcm_data[base + 6],
                    pcm_data[base + 7],
                ]);
                stereo.push([left, right]);
            }
            let chunk_data = AudioChunkData::new(stereo, sample_rate, 0.0);
            AudioChunk::I32(chunk_data)
        }
        _ => {
            return Err(AudioError::ProcessingError(format!(
                "Unsupported bit depth: {}",
                bits_per_sample
            )))
        }
    };

    Ok(Arc::new(AudioSegment {
        order,
        timestamp_sec,
        segment: _AudioSegment::Chunk(Arc::new(chunk)),
    }))
}

/// Convertit Song en TrackMetadata
///
/// Configure toutes les métadonnées de manière asynchrone et attend que la configuration
/// soit terminée avant de retourner, garantissant que les métadonnées (y compris cover_url)
/// sont disponibles immédiatement pour les nodes suivants
async fn song_to_metadata(song: &Song, block: &Block) -> Arc<RwLock<dyn TrackMetadata>> {
    let metadata = MemoryTrackMetadata::new();
    let metadata_arc = Arc::new(RwLock::new(metadata)) as Arc<RwLock<dyn TrackMetadata>>;

    // Cloner les données
    let title = song.title.clone();
    let artist = song.artist.clone();
    let album = song.album.clone();
    let year = song.year;
    let cover_url = song.cover.as_ref().and_then(|cover| block.cover_url(cover));

    // Configurer les métadonnées de manière synchrone (mais async await)
    {
        let mut meta = metadata_arc.write().await;

        // Ces méthodes peuvent échouer (retournent Result), donc on log les erreurs
        if let Err(e) = meta.set_title(Some(title)).await {
            tracing::warn!("Failed to set title: {}", e);
        }
        if let Err(e) = meta.set_artist(Some(artist)).await {
            tracing::warn!("Failed to set artist: {}", e);
        }
        if let Some(album) = album {
            if let Err(e) = meta.set_album(Some(album)).await {
                tracing::warn!("Failed to set album: {}", e);
            }
        }
        if let Some(year) = year {
            if let Err(e) = meta.set_year(Some(year)).await {
                tracing::warn!("Failed to set year: {}", e);
            }
        }
        if let Some(ref url) = cover_url {
            tracing::debug!("RadioParadiseStreamSource: Setting cover_url to: {}", url);
            if let Err(e) = meta.set_cover_url(Some(url.clone())).await {
                tracing::warn!("Failed to set cover_url: {}", e);
            } else {
                tracing::debug!("RadioParadiseStreamSource: Successfully set cover_url");
            }
        } else {
            tracing::debug!("RadioParadiseStreamSource: No cover URL available for song");
        }
    }

    metadata_arc
}

#[async_trait::async_trait]
impl NodeLogic for RadioParadiseStreamSourceLogic {
    async fn process(
        &mut self,
        _input: Option<mpsc::Receiver<Arc<AudioSegment>>>,
        output: Vec<mpsc::Sender<Arc<AudioSegment>>>,
        stop_token: CancellationToken,
    ) -> Result<(), AudioError> {
        tracing::debug!("RadioParadiseStreamSource::process() started, block_queue has {} items", self.block_queue.len());
        for (i, event_id) in self.block_queue.iter().enumerate() {
            tracing::debug!("  block_queue[{}] = {}", i, event_id);
        }

        let mut order = 0u64;
        let mut last_timestamp = 0.0;
        let mut last_start_instant: Option<Instant> = None;

        loop {
            // Attendre un block ID depuis la queue (pas de timeout - mode idle)
            tracing::debug!("Waiting for block_id from queue (idle mode, no timeout)...");
            let event_id = loop {
                // Vérifier d'abord le stop_token
                if stop_token.is_cancelled() {
                    tracing::info!("Stop token cancelled while waiting for block_id");
                    break None;
                }

                // Essayer de pop un event_id
                if let Some(id) = self.block_queue.pop_front() {
                    tracing::debug!("Got event_id {} from queue", id);

                    // Vérifier si c'est le signal de fin
                    if id == END_OF_BLOCKS_SIGNAL {
                        tracing::info!("Received END_OF_BLOCKS_SIGNAL, finishing after current block");
                        break None;
                    }

                    break Some(id);
                }

                // Queue vide, attendre un peu et réessayer
                tracing::trace!("block_queue is empty, sleeping 100ms...");
                tokio::time::sleep(Duration::from_millis(100)).await;
            };

            // Si on n'a pas d'event_id, on termine
            let event_id = match event_id {
                Some(id) => id,
                None => {
                    tracing::info!("No more blocks to process, exiting loop");
                    break;
                }
            };

            // Vérifier si déjà téléchargé récemment
            if self.is_recent_block(event_id) {
                tracing::debug!("Block {} was recently downloaded, skipping", event_id);
                continue;
            }

            // Récupérer les métadonnées du bloc
            tracing::debug!("Fetching block metadata for event_id {}...", event_id);
            let block = self.client
                .get_block(Some(event_id))
                .await
                .map_err(|e| AudioError::ProcessingError(format!("Failed to get block: {}", e)))?;
            tracing::debug!("Block metadata received: url={}", block.url);

            // Marquer comme téléchargé
            self.mark_block_downloaded(event_id);

            // Télécharger et décoder le bloc
            tracing::info!("Starting download and decode for block {}...", event_id);
            let (block_duration, start_instant) = self.download_and_decode_block(&block, &output, &stop_token, &mut order)
                .await?;
            last_timestamp = block_duration;
            last_start_instant = Some(start_instant);
            tracing::info!("Finished download and decode for block {} (duration: {:.2}s)", event_id, block_duration);
        }

        // Envoyer EndOfStream avec le timestamp du dernier chunk
        tracing::info!("Sending EndOfStream with timestamp {:.2}s to {} outputs", last_timestamp, output.len());
        let eos = AudioSegment::new_end_of_stream(order, last_timestamp);
        for tx in &output {
            tx.send(eos.clone())
                .await
                .map_err(|_| AudioError::ChildDied)?;
        }

        // IMPORTANT: Attendre que tous les channels soient fermés par les enfants
        // Cela garantit que tous les chunks (y compris ceux en attente dans les buffers MPSC)
        // ont été traités avant que nous ne fermions notre bout
        tracing::info!("Waiting for all child nodes to close their channels...");
        for (i, tx) in output.iter().enumerate() {
            tracing::debug!("Waiting for child {} to close channel...", i);
            tx.closed().await;
            tracing::debug!("Child {} channel closed", i);
        }
        tracing::info!("All child channels closed, pipeline complete");

        if let Some(start_instant) = last_start_instant {
            let total_elapsed = start_instant.elapsed().as_secs_f64();
            tracing::info!(
                "Block processing complete: duration={:.2}s, total_elapsed={:.2}s ({:.1}% of real-time)",
                last_timestamp, total_elapsed, (total_elapsed / last_timestamp) * 100.0
            );
        }

        // Log des statistiques finales
        tracing::info!("\n{}", self.stats.report());

        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// RadioParadiseStreamSource - Wrapper utilisant Node<RadioParadiseStreamSourceLogic>
// ═══════════════════════════════════════════════════════════════════════════

pub struct RadioParadiseStreamSource {
    inner: Node<RadioParadiseStreamSourceLogic>,
}

impl RadioParadiseStreamSource {
    /// Crée une nouvelle source Radio Paradise avec durée de chunk par défaut
    pub fn new(client: RadioParadiseClient) -> Self {
        Self::with_chunk_duration(client, DEFAULT_CHUNK_DURATION_MS as u32)
    }

    /// Crée une nouvelle source avec durée de chunk personnalisée
    pub fn with_chunk_duration(client: RadioParadiseClient, chunk_duration_ms: u32) -> Self {
        let logic = RadioParadiseStreamSourceLogic::new(client, chunk_duration_ms);
        Self {
            inner: Node::new_source(logic),
        }
    }

    /// Ajoute un block ID à la file d'attente de téléchargement
    pub fn push_block_id(&mut self, event_id: EventId) {
        self.inner.logic_mut().push_block_id(event_id);
    }
}

#[async_trait::async_trait]
impl AudioPipelineNode for RadioParadiseStreamSource {
    fn get_tx(&self) -> Option<mpsc::Sender<Arc<AudioSegment>>> {
        self.inner.get_tx()
    }

    fn register(&mut self, child: Box<dyn AudioPipelineNode>) {
        self.inner.register(child);
    }

    async fn run(self: Box<Self>, stop_token: CancellationToken) -> Result<(), AudioError> {
        Box::new(self.inner).run(stop_token).await
    }
}

impl TypedAudioNode for RadioParadiseStreamSource {
    fn input_type(&self) -> Option<TypeRequirement> {
        None // Source node
    }

    fn output_type(&self) -> Option<TypeRequirement> {
        // Radio Paradise FLAC peut être 16-bit, 24-bit, ou 32-bit
        // La profondeur est détectée automatiquement depuis le header FLAC
        Some(TypeRequirement::any_integer())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_client() -> RadioParadiseClient {
        RadioParadiseClient::with_client(reqwest::Client::new())
    }

    #[test]
    fn test_cache_fifo_basic() {
        let client = create_test_client();
        let mut logic = RadioParadiseStreamSourceLogic::new(client, DEFAULT_CHUNK_DURATION_MS as u32);

        // Ajouter 5 blocs
        for i in 1..=5 {
            logic.mark_block_downloaded(i);
        }

        // Vérifier que tous sont dans le cache
        for i in 1..=5 {
            assert!(logic.is_recent_block(i), "Block {} should be in cache", i);
        }
        assert_eq!(logic.recent_blocks.len(), 5);
    }

    #[test]
    fn test_cache_fifo_exactly_10_elements() {
        let client = create_test_client();
        let mut logic = RadioParadiseStreamSourceLogic::new(client, DEFAULT_CHUNK_DURATION_MS as u32);

        // Ajouter exactement 10 blocs
        for i in 1..=10 {
            logic.mark_block_downloaded(i);
        }

        // Vérifier qu'on a exactement 10 éléments
        assert_eq!(logic.recent_blocks.len(), 10, "Cache should have exactly 10 elements");

        // Tous devraient être dans le cache
        for i in 1..=10 {
            assert!(logic.is_recent_block(i), "Block {} should be in cache", i);
        }
    }

    #[test]
    fn test_cache_fifo_eviction_oldest() {
        let client = create_test_client();
        let mut logic = RadioParadiseStreamSourceLogic::new(client, DEFAULT_CHUNK_DURATION_MS as u32);

        // Remplir le cache avec 10 éléments (1..=10)
        for i in 1..=10 {
            logic.mark_block_downloaded(i);
        }

        // Ajouter un 11ème élément
        logic.mark_block_downloaded(11);

        // Le cache doit toujours avoir 10 éléments
        assert_eq!(logic.recent_blocks.len(), 10, "Cache should still have 10 elements");

        // Le premier (plus ancien) doit avoir été évincé
        assert!(!logic.is_recent_block(1), "Oldest block (1) should be evicted");

        // Les éléments 2..=11 doivent être présents
        for i in 2..=11 {
            assert!(logic.is_recent_block(i), "Block {} should be in cache", i);
        }
    }

    #[test]
    fn test_cache_fifo_multiple_evictions() {
        let client = create_test_client();
        let mut logic = RadioParadiseStreamSourceLogic::new(client, DEFAULT_CHUNK_DURATION_MS as u32);

        // Remplir avec 10 éléments
        for i in 1..=10 {
            logic.mark_block_downloaded(i);
        }

        // Ajouter 5 éléments supplémentaires
        for i in 11..=15 {
            logic.mark_block_downloaded(i);
        }

        // Toujours 10 éléments
        assert_eq!(logic.recent_blocks.len(), 10, "Cache should have 10 elements");

        // Les 5 premiers doivent avoir été évincés
        for i in 1..=5 {
            assert!(!logic.is_recent_block(i), "Block {} should be evicted", i);
        }

        // Les éléments 6..=15 doivent être présents
        for i in 6..=15 {
            assert!(logic.is_recent_block(i), "Block {} should be in cache", i);
        }
    }

    #[test]
    fn test_cache_never_exceeds_capacity() {
        let client = create_test_client();
        let mut logic = RadioParadiseStreamSourceLogic::new(client, DEFAULT_CHUNK_DURATION_MS as u32);

        // Vérifier la capacité pré-allouée
        assert_eq!(logic.recent_blocks.capacity(), RECENT_BLOCKS_CACHE_SIZE);

        // Ajouter beaucoup d'éléments
        for i in 1..=100 {
            logic.mark_block_downloaded(i);

            // À chaque itération, vérifier qu'on ne dépasse jamais 10
            assert!(
                logic.recent_blocks.len() <= RECENT_BLOCKS_CACHE_SIZE,
                "Cache size {} exceeded max {}",
                logic.recent_blocks.len(),
                RECENT_BLOCKS_CACHE_SIZE
            );
        }

        // Finalement, on doit avoir exactement 10 éléments
        assert_eq!(logic.recent_blocks.len(), 10);

        // Ce doivent être les 10 derniers (91..=100)
        for i in 91..=100 {
            assert!(logic.is_recent_block(i), "Block {} should be in cache", i);
        }
    }

    #[test]
    fn test_cache_fifo_order_preserved() {
        let client = create_test_client();
        let mut logic = RadioParadiseStreamSourceLogic::new(client, DEFAULT_CHUNK_DURATION_MS as u32);

        // Ajouter 10 éléments
        for i in 1..=10 {
            logic.mark_block_downloaded(i);
        }

        // Vérifier l'ordre dans la VecDeque (le front devrait être le plus ancien)
        let front = logic.recent_blocks.front().copied();
        assert_eq!(front, Some(1), "Front should be the oldest element");

        let back = logic.recent_blocks.back().copied();
        assert_eq!(back, Some(10), "Back should be the newest element");
    }

    #[test]
    fn test_block_queue_push() {
        let client = create_test_client();
        let mut logic = RadioParadiseStreamSourceLogic::new(client, DEFAULT_CHUNK_DURATION_MS as u32);

        // Tester push_block_id
        logic.push_block_id(100);
        logic.push_block_id(200);
        logic.push_block_id(300);

        assert_eq!(logic.block_queue.len(), 3);
        assert_eq!(logic.block_queue.front(), Some(&100));
        assert_eq!(logic.block_queue.back(), Some(&300));
    }
}
