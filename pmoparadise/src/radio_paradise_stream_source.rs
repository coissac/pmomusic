//! RadioParadiseStreamSource - Node audio pmoaudio pour Radio Paradise
//!
//! Ce node télécharge et décode les blocs FLAC de Radio Paradise en streaming,
//! avec insertion automatique des TrackBoundary au bon timing.

use crate::{
    client::RadioParadiseClient,
    models::{Block, EventId, Song},
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
    time::Duration,
};
use tokio::io::AsyncReadExt;
use tokio::sync::{mpsc, RwLock};
use tokio_util::{io::StreamReader, sync::CancellationToken};

/// Timeout pour attendre un nouveau block ID (radio en temps réel)
const BLOCK_ID_TIMEOUT_SECS: u64 = 3;

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
    async fn download_and_decode_block(
        &mut self,
        block: &Block,
        output: &[mpsc::Sender<Arc<AudioSegment>>],
        stop_token: &CancellationToken,
        order: &mut u64,
    ) -> Result<(), AudioError> {
        // Télécharger le FLAC
        let response = self.client.client
            .get(&block.url)
            .timeout(self.client.block_timeout)
            .send()
            .await
            .map_err(|e| AudioError::ProcessingError(format!("Block download failed: {}", e)))?;

        if !response.status().is_success() {
            return Err(AudioError::ProcessingError(format!(
                "Block download returned status {}",
                response.status()
            )));
        }

        // Créer un stream reader
        let byte_stream = response.bytes_stream().map(|result| {
            result.map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
        });
        let stream_reader = StreamReader::new(byte_stream);

        // Décoder le FLAC
        let mut decoder = decode_audio_stream(stream_reader)
            .await
            .map_err(|e| AudioError::ProcessingError(format!("FLAC decode failed: {}", e)))?;

        let stream_info = decoder.info().clone();
        let sample_rate = stream_info.sample_rate;
        let bits_per_sample = stream_info.bits_per_sample;

        // Préparer les songs ordonnées pour tracking
        let songs = block.songs_ordered();
        let mut song_index = 0;
        let mut next_song: Option<(usize, &Song)> = songs.get(0).copied();
        let mut total_samples = 0u64;

        // Envoyer TopZeroSync au début du bloc
        let top_zero = Arc::new(AudioSegment {
            order: *order,
            timestamp_sec: 0.0,
            segment: pmoaudio::_AudioSegment::Sync(Arc::new(SyncMarker::TopZeroSync)),
        });
        self.send_to_children(output, top_zero).await?;

        // Buffer pour lecture
        let bytes_per_sample = (bits_per_sample / 8) as usize;
        let frame_bytes = bytes_per_sample * 2; // stereo
        let chunk_frames = self.chunk_frames;
        let chunk_byte_len = chunk_frames * frame_bytes;
        let mut read_buf = vec![0u8; chunk_byte_len * 2];
        let mut pending: Vec<u8> = Vec::with_capacity(chunk_byte_len * 2);

        // Traiter les chunks audio
        loop {
            // Vérifier stop_token
            if stop_token.is_cancelled() {
                return Ok(());
            }

            // Remplir le buffer
            if pending.len() < chunk_byte_len {
                let read = decoder.read(&mut read_buf).await
                    .map_err(|e| AudioError::ProcessingError(format!("Read error: {}", e)))?;

                if read == 0 {
                    break; // EOF
                }
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
            if let Some((_idx, song)) = next_song {
                let elapsed_ms = (total_samples * 1000) / sample_rate as u64;

                if elapsed_ms >= song.elapsed {
                    // Envoyer TrackBoundary AVANT le chunk (avec le même order)
                    let metadata = song_to_metadata(song, block);
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
        }

        Ok(())
    }

    /// Envoie un segment à tous les enfants
    async fn send_to_children(
        &self,
        output: &[mpsc::Sender<Arc<AudioSegment>>],
        segment: Arc<AudioSegment>,
    ) -> Result<(), AudioError> {
        for tx in output {
            tx.send(segment.clone())
                .await
                .map_err(|_| AudioError::ChildDied)?;
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
/// Cette fonction est synchrone, donc on wrap la metadata dans Arc<RwLock<>>
/// et on spawn une tâche async pour la configurer
fn song_to_metadata(song: &Song, block: &Block) -> Arc<RwLock<dyn TrackMetadata>> {
    let metadata = MemoryTrackMetadata::new();
    let metadata_arc = Arc::new(RwLock::new(metadata)) as Arc<RwLock<dyn TrackMetadata>>;
    let metadata_clone = metadata_arc.clone();

    // Clone des données pour la task async
    let title = song.title.clone();
    let artist = song.artist.clone();
    let album = song.album.clone();
    let year = song.year;
    let cover_url = song.cover.as_ref().and_then(|cover| block.cover_url(cover));

    // Configurer les métadonnées de manière asynchrone
    tokio::spawn(async move {
        let mut meta = metadata_clone.write().await;

        // Ces méthodes peuvent échouer (retournent Result), donc on propage avec ?
        if let Err(e) = meta.set_title(Some(title)).await {
            eprintln!("Warning: Failed to set title: {}", e);
        }
        if let Err(e) = meta.set_artist(Some(artist)).await {
            eprintln!("Warning: Failed to set artist: {}", e);
        }
        if let Some(album) = album {
            if let Err(e) = meta.set_album(Some(album)).await {
                eprintln!("Warning: Failed to set album: {}", e);
            }
        }
        if let Some(year) = year {
            if let Err(e) = meta.set_year(Some(year)).await {
                eprintln!("Warning: Failed to set year: {}", e);
            }
        }
        if let Some(cover_url) = cover_url {
            if let Err(e) = meta.set_cover_url(Some(cover_url)).await {
                eprintln!("Warning: Failed to set cover_url: {}", e);
            }
        }
    });

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
        let mut order = 0u64;

        loop {
            // Attendre un block ID (timeout court pour une radio)
            let event_id = match tokio::time::timeout(
                Duration::from_secs(BLOCK_ID_TIMEOUT_SECS),
                async {
                    while self.block_queue.is_empty() {
                        tokio::time::sleep(Duration::from_millis(100)).await;

                        if stop_token.is_cancelled() {
                            return None;
                        }
                    }
                    self.block_queue.pop_front()
                }
            ).await {
                Ok(Some(id)) => id,
                Ok(None) => break, // Cancelled
                Err(_) => {
                    // Timeout - pas de nouveau bloc, on termine
                    break;
                }
            };

            // Vérifier si déjà téléchargé récemment
            if self.is_recent_block(event_id) {
                continue;
            }

            // Récupérer les métadonnées du bloc
            let block = self.client
                .get_block(Some(event_id))
                .await
                .map_err(|e| AudioError::ProcessingError(format!("Failed to get block: {}", e)))?;

            // Marquer comme téléchargé
            self.mark_block_downloaded(event_id);

            // Télécharger et décoder le bloc
            self.download_and_decode_block(&block, &output, &stop_token, &mut order)
                .await?;
        }

        // Envoyer EndOfStream
        let eos = AudioSegment::new_end_of_stream(order, 0.0);
        for tx in &output {
            tx.send(eos.clone())
                .await
                .map_err(|_| AudioError::ChildDied)?;
        }

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
        // Radio Paradise FLAC peut être 16-bit ou 24-bit
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
