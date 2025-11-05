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
    AudioChunk, AudioPipelineNode, AudioSegment, SyncMarker, I24,
};
use pmoflac::decode_audio_stream;
use pmometadata::{MemoryTrackMetadata, TrackMetadata};
use std::{
    collections::VecDeque,
    sync::Arc,
    time::Duration,
};
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

        let stream_info = decoder.stream_info();
        let sample_rate = stream_info.sample_rate;
        let bits_per_sample = stream_info.bits_per_sample;

        // Préparer les songs ordonnées pour tracking
        let songs = block.songs_ordered();
        let mut song_index = 0;
        let mut next_song: Option<(usize, &Song)> = songs.get(0).copied();
        let mut total_samples = 0u64;

        // Envoyer TopZeroSync au début du bloc
        self.send_to_children(
            output,
            Arc::new(AudioSegment::new_sync(
                *order,
                SyncMarker::TopZeroSync,
            )),
        ).await?;

        // Traiter les chunks audio
        while let Some(result) = decoder.next().await {
            // Vérifier stop_token
            if stop_token.is_cancelled() {
                return Ok(());
            }

            let pcm_data = result
                .map_err(|e| AudioError::ProcessingError(format!("Decode error: {}", e)))?;

            // Convertir en AudioChunk selon la profondeur de bit
            let chunk = pcm_to_audio_chunk(&pcm_data, sample_rate, bits_per_sample)?;
            let chunk_len = chunk.len() as u64;

            // Vérifier si on doit insérer un TrackBoundary avant ce chunk
            if let Some((idx, song)) = next_song {
                let elapsed_ms = (total_samples * 1000) / sample_rate as u64;

                if elapsed_ms >= song.elapsed {
                    // Envoyer TrackBoundary AVANT le chunk (avec le même order)
                    let metadata = song_to_metadata(song, block);
                    self.send_to_children(
                        output,
                        Arc::new(AudioSegment::new_sync(
                            *order,
                            SyncMarker::TrackBoundary {
                                metadata,
                                track_number: idx,
                            },
                        )),
                    ).await?;

                    // Passer à la song suivante
                    song_index += 1;
                    next_song = songs.get(song_index).copied();
                }
            }

            // Envoyer le chunk audio
            self.send_to_children(
                output,
                Arc::new(AudioSegment::new_chunk(*order, chunk)),
            ).await?;

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

/// Convertit PCM bytes en AudioChunk
fn pcm_to_audio_chunk(
    pcm_data: &[u8],
    sample_rate: u32,
    bits_per_sample: u8,
) -> Result<AudioChunk, AudioError> {
    match bits_per_sample {
        16 => {
            // Convertir bytes en i16
            let samples: Vec<i16> = pcm_data
                .chunks_exact(2)
                .map(|chunk| i16::from_le_bytes([chunk[0], chunk[1]]))
                .collect();

            Ok(AudioChunk::I16(pmoaudio::AudioChunkData::from_interleaved(
                &samples,
                sample_rate,
            )))
        }
        24 => {
            // Convertir bytes en I24
            let samples: Vec<I24> = pcm_data
                .chunks_exact(3)
                .map(|chunk| {
                    let value = i32::from_le_bytes([chunk[0], chunk[1], chunk[2], 0]) >> 8;
                    I24::from_i32(value)
                })
                .collect();

            Ok(AudioChunk::I24(pmoaudio::AudioChunkData::from_interleaved(
                &samples,
                sample_rate,
            )))
        }
        _ => Err(AudioError::ProcessingError(format!(
            "Unsupported bit depth: {}",
            bits_per_sample
        ))),
    }
}

/// Convertit Song en TrackMetadata
fn song_to_metadata(song: &Song, block: &Block) -> Arc<RwLock<dyn TrackMetadata>> {
    let mut metadata = MemoryTrackMetadata::new();

    metadata.set_title(Some(song.title.clone()));
    metadata.set_artist(Some(song.artist.clone()));

    if let Some(ref album) = song.album {
        metadata.set_album(Some(album.clone()));
    }

    if let Some(year) = song.year {
        metadata.set_year(Some(year));
    }

    // Cover URL si disponible
    if let Some(ref cover_path) = song.cover {
        if let Some(cover_url) = block.cover_url(cover_path) {
            let metadata_arc = Arc::new(RwLock::new(metadata)) as Arc<RwLock<dyn TrackMetadata>>;
            let metadata_clone = metadata_arc.clone();

            tokio::spawn(async move {
                if let Ok(mut meta) = metadata_clone.write().await {
                    let _ = meta.set_cover_url(Some(cover_url)).await;
                }
            });

            return metadata_arc;
        }
    }

    Arc::new(RwLock::new(metadata))
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
        for tx in &output {
            tx.send(Arc::new(AudioSegment::new_sync(
                order,
                SyncMarker::EndOfStream,
            )))
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
        Self::with_chunk_duration(client, DEFAULT_CHUNK_DURATION_MS)
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
    use crate::Channel;

    #[test]
    fn test_cache_fifo_basic() {
        let client = RadioParadiseClient::new(Channel::MainMix);
        let mut logic = RadioParadiseStreamSourceLogic::new(client, DEFAULT_CHUNK_DURATION_MS);

        // Ajouter 5 blocs
        for i in 1..=5 {
            logic.mark_block_downloaded(EventId(i));
        }

        // Vérifier que tous sont dans le cache
        for i in 1..=5 {
            assert!(logic.is_recent_block(EventId(i)), "Block {} should be in cache", i);
        }
        assert_eq!(logic.recent_blocks.len(), 5);
    }

    #[test]
    fn test_cache_fifo_exactly_10_elements() {
        let client = RadioParadiseClient::new(Channel::MainMix);
        let mut logic = RadioParadiseStreamSourceLogic::new(client, DEFAULT_CHUNK_DURATION_MS);

        // Ajouter exactement 10 blocs
        for i in 1..=10 {
            logic.mark_block_downloaded(EventId(i));
        }

        // Vérifier qu'on a exactement 10 éléments
        assert_eq!(logic.recent_blocks.len(), 10, "Cache should have exactly 10 elements");

        // Tous devraient être dans le cache
        for i in 1..=10 {
            assert!(logic.is_recent_block(EventId(i)), "Block {} should be in cache", i);
        }
    }

    #[test]
    fn test_cache_fifo_eviction_oldest() {
        let client = RadioParadiseClient::new(Channel::MainMix);
        let mut logic = RadioParadiseStreamSourceLogic::new(client, DEFAULT_CHUNK_DURATION_MS);

        // Remplir le cache avec 10 éléments (1..=10)
        for i in 1..=10 {
            logic.mark_block_downloaded(EventId(i));
        }

        // Ajouter un 11ème élément
        logic.mark_block_downloaded(EventId(11));

        // Le cache doit toujours avoir 10 éléments
        assert_eq!(logic.recent_blocks.len(), 10, "Cache should still have 10 elements");

        // Le premier (plus ancien) doit avoir été évincé
        assert!(!logic.is_recent_block(EventId(1)), "Oldest block (1) should be evicted");

        // Les éléments 2..=11 doivent être présents
        for i in 2..=11 {
            assert!(logic.is_recent_block(EventId(i)), "Block {} should be in cache", i);
        }
    }

    #[test]
    fn test_cache_fifo_multiple_evictions() {
        let client = RadioParadiseClient::new(Channel::MainMix);
        let mut logic = RadioParadiseStreamSourceLogic::new(client, DEFAULT_CHUNK_DURATION_MS);

        // Remplir avec 10 éléments
        for i in 1..=10 {
            logic.mark_block_downloaded(EventId(i));
        }

        // Ajouter 5 éléments supplémentaires
        for i in 11..=15 {
            logic.mark_block_downloaded(EventId(i));
        }

        // Toujours 10 éléments
        assert_eq!(logic.recent_blocks.len(), 10, "Cache should have 10 elements");

        // Les 5 premiers doivent avoir été évincés
        for i in 1..=5 {
            assert!(!logic.is_recent_block(EventId(i)), "Block {} should be evicted", i);
        }

        // Les éléments 6..=15 doivent être présents
        for i in 6..=15 {
            assert!(logic.is_recent_block(EventId(i)), "Block {} should be in cache", i);
        }
    }

    #[test]
    fn test_cache_never_exceeds_capacity() {
        let client = RadioParadiseClient::new(Channel::MainMix);
        let mut logic = RadioParadiseStreamSourceLogic::new(client, DEFAULT_CHUNK_DURATION_MS);

        // Vérifier la capacité pré-allouée
        assert_eq!(logic.recent_blocks.capacity(), RECENT_BLOCKS_CACHE_SIZE);

        // Ajouter beaucoup d'éléments
        for i in 1..=100 {
            logic.mark_block_downloaded(EventId(i));

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
            assert!(logic.is_recent_block(EventId(i)), "Block {} should be in cache", i);
        }
    }

    #[test]
    fn test_cache_fifo_order_preserved() {
        let client = RadioParadiseClient::new(Channel::MainMix);
        let mut logic = RadioParadiseStreamSourceLogic::new(client, DEFAULT_CHUNK_DURATION_MS);

        // Ajouter 10 éléments
        for i in 1..=10 {
            logic.mark_block_downloaded(EventId(i));
        }

        // Vérifier l'ordre dans la VecDeque (le front devrait être le plus ancien)
        let front = logic.recent_blocks.front().copied();
        assert_eq!(front, Some(EventId(1)), "Front should be the oldest element");

        let back = logic.recent_blocks.back().copied();
        assert_eq!(back, Some(EventId(10)), "Back should be the newest element");
    }

    #[test]
    fn test_block_queue_push() {
        let client = RadioParadiseClient::new(Channel::MainMix);
        let mut logic = RadioParadiseStreamSourceLogic::new(client, DEFAULT_CHUNK_DURATION_MS);

        // Tester push_block_id
        logic.push_block_id(EventId(100));
        logic.push_block_id(EventId(200));
        logic.push_block_id(EventId(300));

        assert_eq!(logic.block_queue.len(), 3);
        assert_eq!(logic.block_queue.front(), Some(&EventId(100)));
        assert_eq!(logic.block_queue.back(), Some(&EventId(300)));
    }
}
