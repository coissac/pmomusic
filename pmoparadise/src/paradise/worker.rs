//! Background worker for Radio Paradise channels.
//!
//! The worker handles API polling, block ingestion, caching and playlist
//! maintenance.  It keeps the channel state in sync with connected clients
//! and ensures fresh content is available according to the specification.

use super::channel::ChannelDescriptor;
use super::constants::*;
use super::history::HistoryBackend;
use super::playlist::{PlaylistEntry, SharedPlaylist};
use crate::client::RadioParadiseClient;
use crate::models::{Block, Song};
use anyhow::{anyhow, Context, Result};
use bytes::Bytes;
use chrono::Utc;
use futures::stream;
use pmosource::{SourceCacheManager, TrackMetadata};
use std::collections::{HashSet, VecDeque};
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time::{sleep, Duration};
use tokio_util::io::StreamReader;
use tracing::{debug, error, info, warn};
use url::Url;

/// Commands sent to the background worker.
#[derive(Debug)]
pub enum WorkerCommand {
    EnsureReady,
    ClientConnected { client_id: String },
    ClientDisconnected { client_id: String },
    RefreshBlock,
    Shutdown,
}

/// Handle to the spawned worker task.
pub struct ParadiseWorker {
    descriptor: ChannelDescriptor,
    join_handle: JoinHandle<()>,
}

impl ParadiseWorker {
    #[allow(clippy::too_many_arguments)]
    pub fn spawn(
        descriptor: ChannelDescriptor,
        client: RadioParadiseClient,
        history_max_tracks: usize,
        playlist: SharedPlaylist,
        history: Arc<dyn HistoryBackend>,
        cache_manager: Arc<SourceCacheManager>,
    ) -> (Self, mpsc::Sender<WorkerCommand>) {
        let (tx, mut rx) = mpsc::channel(32);

        let join_handle = tokio::spawn(async move {
            info!(channel = descriptor.slug, "Starting Radio Paradise worker");

            let mut state = WorkerState::new(
                descriptor,
                client,
                history_max_tracks,
                playlist,
                history,
                cache_manager,
            );

            loop {
                if let Some(task) = state.scheduled_task.as_mut() {
                    let kind = task.kind;
                    let mut pending_command: Option<Option<WorkerCommand>> = None;

                    tokio::select! {
                        cmd = rx.recv() => {
                            pending_command = Some(cmd);
                        }
                        _ = &mut task.sleep => {
                            state.scheduled_task = None;
                            if let Err(err) = state.handle_scheduled_task(kind).await {
                                error!(channel = state.descriptor.slug, "Worker scheduled task error: {err:?}");
                                state.on_error(err);
                            }
                        }
                    }

                    if let Some(Some(cmd)) = pending_command {
                        if let Err(err) = state.handle_command(cmd).await {
                            error!(
                                channel = state.descriptor.slug,
                                "Worker command error: {err:?}"
                            );
                            state.on_error(err);
                        }
                        if state.shutdown {
                            break;
                        }
                    } else if let Some(None) = pending_command {
                        // Command channel closed, terminate
                        break;
                    }
                } else {
                    match rx.recv().await {
                        Some(cmd) => {
                            if let Err(err) = state.handle_command(cmd).await {
                                error!(
                                    channel = state.descriptor.slug,
                                    "Worker command error: {err:?}"
                                );
                                state.on_error(err);
                            }
                            if state.shutdown {
                                break;
                            }
                        }
                        None => break,
                    }
                }
            }

            info!(channel = state.descriptor.slug, "Worker stopped");
        });

        (
            Self {
                descriptor,
                join_handle,
            },
            tx,
        )
    }

    pub async fn wait(self) -> Result<()> {
        if let Err(err) = self.join_handle.await {
            if err.is_cancelled() {
                warn!(
                    channel = self.descriptor.slug,
                    "Worker task cancelled: {err}"
                );
                return Ok(());
            }
            return Err(anyhow!("Worker join error: {}", err));
        }
        Ok(())
    }
}

struct WorkerState {
    descriptor: ChannelDescriptor,
    client: RadioParadiseClient,
    playlist: SharedPlaylist,
    history: Arc<dyn HistoryBackend>,
    cache_manager: Arc<SourceCacheManager>,
    active_clients: usize,
    status: ChannelLifecycle,
    processed_blocks: HashSet<u64>,
    processing_blocks: HashSet<u64>,
    recent_blocks: VecDeque<u64>,
    next_block_hint: Option<u64>,
    scheduled_task: Option<ScheduledTask>,
    backoff: BackoffState,
    shutdown: bool,
}

#[derive(Clone)]
struct SongTaskContext {
    cache_manager: Arc<SourceCacheManager>,
    playlist: SharedPlaylist,
    descriptor_id: u8,
    slug: &'static str,
}

impl WorkerState {
    fn song_task_context(&self) -> SongTaskContext {
        SongTaskContext {
            cache_manager: Arc::clone(&self.cache_manager),
            playlist: self.playlist.clone(),
            descriptor_id: self.descriptor.id,
            slug: self.descriptor.slug,
        }
    }

    fn new(
        descriptor: ChannelDescriptor,
        client: RadioParadiseClient,
        _history_max_tracks: usize,
        playlist: SharedPlaylist,
        history: Arc<dyn HistoryBackend>,
        cache_manager: Arc<SourceCacheManager>,
    ) -> Self {
        Self {
            descriptor,
            client,
            playlist,
            history,
            cache_manager,
            active_clients: 0,
            status: ChannelLifecycle::Idle,
            processed_blocks: HashSet::new(),
            processing_blocks: HashSet::new(),
            recent_blocks: VecDeque::new(),
            next_block_hint: None,
            scheduled_task: None,
            backoff: BackoffState::new(),
            shutdown: false,
        }
    }

    async fn handle_command(&mut self, cmd: WorkerCommand) -> Result<()> {
        debug!(channel = self.descriptor.slug, ?cmd, "Worker command");

        match cmd {
            WorkerCommand::EnsureReady => {
                self.ensure_ready().await?;
            }
            WorkerCommand::ClientConnected { .. } => {
                self.active_clients = self.active_clients.saturating_add(1);
                self.enter_active();
                self.ensure_ready().await?;
            }
            WorkerCommand::ClientDisconnected { .. } => {
                self.active_clients = self.active_clients.saturating_sub(1);
                if self.active_clients == 0 {
                    self.enter_cooling();
                }
            }
            WorkerCommand::RefreshBlock => {
                self.fetch_next_block().await?;
            }
            WorkerCommand::Shutdown => {
                self.shutdown = true;
                self.cancel_scheduled_task();
            }
        }

        if !self.shutdown {
            self.maybe_schedule_poll().await;
        }

        Ok(())
    }

    async fn handle_scheduled_task(&mut self, kind: ScheduledTaskKind) -> Result<()> {
        match kind {
            ScheduledTaskKind::Poll => {
                self.fetch_next_block().await?;
                self.maybe_schedule_poll().await;
            }
            ScheduledTaskKind::Cooling => {
                debug!(
                    channel = self.descriptor.slug,
                    "Cooling timeout reached -> idle"
                );
                self.status = ChannelLifecycle::Idle;
                self.next_block_hint = None;
                self.playlist.clear().await;
                self.processed_blocks.clear();
                self.recent_blocks.clear();
            }
        }
        Ok(())
    }

    fn on_error(&mut self, err: anyhow::Error) {
        warn!(channel = self.descriptor.slug, "Worker error: {err:?}");
        let delay = self.backoff.next_delay();
        self.schedule_task(ScheduledTaskKind::Poll, delay);
    }

    fn enter_active(&mut self) {
        if !matches!(self.status, ChannelLifecycle::Active) {
            debug!(
                channel = self.descriptor.slug,
                "Channel entering Active state"
            );
        }
        self.status = ChannelLifecycle::Active;
        if matches!(self.scheduled_task_kind(), Some(ScheduledTaskKind::Cooling)) {
            self.cancel_scheduled_task();
        }
        self.backoff.reset();
    }

    fn enter_cooling(&mut self) {
        if matches!(self.status, ChannelLifecycle::Idle) {
            return;
        }
        debug!(
            channel = self.descriptor.slug,
            "Channel entering Cooling state"
        );
        self.status = ChannelLifecycle::Cooling;
        let duration = Duration::from_secs(COOLING_TIMEOUT_SECONDS.max(1));
        self.schedule_task(ScheduledTaskKind::Cooling, duration);
    }

    async fn ensure_ready(&mut self) -> Result<()> {
        if !matches!(self.status, ChannelLifecycle::Active) {
            self.enter_active();
        }

        let has_tracks = self.playlist.active_len().await > 0;

        if !has_tracks {
            debug!(
                channel = self.descriptor.slug,
                "Playlist empty – fetching now playing"
            );
            let now_playing = self.client.now_playing().await?;
            self.process_block(now_playing.block).await?;
        }

        Ok(())
    }

    async fn fetch_next_block(&mut self) -> Result<()> {
        if !matches!(self.status, ChannelLifecycle::Active) {
            debug!(
                channel = self.descriptor.slug,
                "Skipping poll while not active"
            );
            return Ok(());
        }

        let event_id = self.next_block_hint;
        let block = self.client.get_block(event_id).await?;
        self.process_block(block).await?;
        Ok(())
    }

    async fn process_block(&mut self, block: Block) -> Result<()> {
        // Check if we just processed this block (songs are already in playlist)
        if self.is_recent_block(block.event) {
            debug!(
                channel = self.descriptor.slug,
                event = block.event,
                "Skipping already processed block (songs already in playlist)"
            );
            self.next_block_hint = Some(block.end_event);
            return Ok(());
        }

        // Check if this block is currently being processed by another task
        // This prevents race conditions when the same block is requested multiple times
        if self.processing_blocks.contains(&block.event) {
            warn!(
                channel = self.descriptor.slug,
                event = block.event,
                "Block is already being processed, skipping duplicate request"
            );
            return Ok(());
        }

        // Check if all songs from this block are in cache
        // If yes, restore from cache instead of downloading
        if self.check_all_songs_cached(&block).await {
            info!(
                channel = self.descriptor.slug,
                event = block.event,
                "Block found in cache, restoring without download"
            );
            self.restore_from_cache(&block).await?;
            self.record_processed_block(block.event);
            self.next_block_hint = Some(block.end_event);
            self.backoff.reset();
            return Ok(());
        }

        // Mark block as being processed
        self.processing_blocks.insert(block.event);
        let event = block.event; // Save for cleanup

        // Process the block and ensure cleanup even on error
        let result = self.process_block_inner(block).await;

        // Always remove from processing set, whether success or error
        self.processing_blocks.remove(&event);

        result
    }

    async fn process_block_inner(&mut self, block: Block) -> Result<()> {
        info!(
            channel = self.descriptor.slug,
            event = block.event,
            "Processing Radio Paradise block with progressive streaming"
        );

        let _ = &self.history;

        // Start streaming the block
        let block_url = Url::parse(&block.url)?;
        let http_stream = self
            .client
            .stream_block(&block_url)
            .await
            .context("Failed to start block stream")?;

        let ordered_songs = block.songs_ordered();

        // Decode in streaming mode using spawn_blocking
        let (tx, mut rx) = mpsc::channel::<crate::streaming::PCMChunk>(16);

        let decode_handle = tokio::task::spawn_blocking(move || -> Result<()> {
            use crate::streaming::StreamingPCMDecoder;

            let mut decoder = StreamingPCMDecoder::new(http_stream)
                .context("Failed to create streaming decoder")?;

            info!(
                "Streaming decoder initialized: {}Hz, {} channels, {} bits",
                decoder.sample_rate(),
                decoder.channels(),
                decoder.bits_per_sample()
            );

            // Decode chunks and send them
            while let Some(chunk) = decoder.decode_chunk()? {
                if tx.blocking_send(chunk).is_err() {
                    // Receiver dropped, stop decoding
                    break;
                }
            }

            Ok(())
        });

        // Process songs as chunks arrive
        let mut accumulated_pcm = Vec::new();
        let mut current_song_idx = 0;
        let mut sample_rate = 0u32;
        let mut channels = 0u32;
        let mut bits_per_sample = 0u32;

        while let Some(chunk) = rx.recv().await {
            // Store metadata from first chunk
            if sample_rate == 0 {
                sample_rate = chunk.sample_rate;
                channels = chunk.channels;
                bits_per_sample = 16; // Normalized to 16-bit by decoder
            }

            accumulated_pcm.extend_from_slice(&chunk.samples);
            let current_position_ms = chunk.position_ms;

            // Check if we've completed any songs
            while current_song_idx < ordered_songs.len() {
                let (song_index, song) = ordered_songs[current_song_idx];

                // Calculate song boundaries
                let song_start_ms = song.elapsed;
                let song_end_ms = if current_song_idx + 1 < ordered_songs.len() {
                    ordered_songs[current_song_idx + 1].1.elapsed
                } else {
                    u64::MAX // Last song goes to end of block
                };

                // Check if we have enough PCM for this song
                if current_position_ms >= song_end_ms {
                    // Extract song samples
                    let start_frame = crate::streaming::ms_to_frames(song_start_ms, sample_rate);
                    let end_frame = crate::streaming::ms_to_frames(song_end_ms, sample_rate);

                    let start_sample = start_frame * channels as usize;
                    let end_sample = end_frame * channels as usize;

                    if end_sample <= accumulated_pcm.len() {
                        let track_samples = accumulated_pcm[start_sample..end_sample].to_vec();

                        info!(
                            channel = self.descriptor.slug,
                            song_index = song_index,
                            position_ms = current_position_ms,
                            "✅ Song '{}' ready for encoding ({} samples)",
                            song.title,
                            track_samples.len()
                        );

                        let context = self.song_task_context();
                        spawn_song_processing(
                            context,
                            block.clone(),
                            song_index,
                            song.clone(),
                            track_samples,
                            sample_rate,
                            channels as usize,
                            bits_per_sample,
                            self.active_clients,
                            song.duration,
                            current_position_ms,
                        );

                        current_song_idx += 1;
                    } else {
                        // Not enough samples yet, wait for more chunks
                        break;
                    }
                } else {
                    // Haven't reached this song's end yet
                    break;
                }
            }
        }

        // Wait for decoder to finish
        decode_handle.await??;

        // Process any remaining songs (last song in block)
        if current_song_idx < ordered_songs.len() {
            let (song_index, song) = ordered_songs[current_song_idx];
            let song_start_ms = song.elapsed;
            let start_frame = crate::streaming::ms_to_frames(song_start_ms, sample_rate);
            let start_sample = start_frame * channels as usize;

            if start_sample < accumulated_pcm.len() {
                let track_samples = accumulated_pcm[start_sample..].to_vec();

                info!(
                    channel = self.descriptor.slug,
                    song_index = song_index,
                    "Processing last song '{}' ({} samples)",
                    song.title,
                    track_samples.len()
                );

                let context = self.song_task_context();
                spawn_song_processing(
                    context,
                    block.clone(),
                    song_index,
                    song.clone(),
                    track_samples,
                    sample_rate,
                    channels as usize,
                    bits_per_sample,
                    self.active_clients,
                    song.duration,
                    song_start_ms,
                );
            }
        }

        self.record_processed_block(block.event);
        self.next_block_hint = Some(block.end_event);
        self.backoff.reset();

        Ok(())
    }

    async fn process_song(
        &self,
        block: &Block,
        song_index: &usize,
        song: &Song,
        position: usize,
        ordered_songs: &[(usize, &Song)],
        total_frames: usize,
        decoded: &DecodedBlock,
    ) -> Result<Arc<PlaylistEntry>> {
        let duration_ms = song_duration_ms(block, ordered_songs, position);
        let start_frame = ms_to_frames(song.elapsed, decoded.sample_rate);
        let end_frame = if position + 1 < ordered_songs.len() {
            ms_to_frames(ordered_songs[position + 1].1.elapsed, decoded.sample_rate)
        } else {
            total_frames
        };

        if end_frame <= start_frame || end_frame > total_frames {
            warn!(
                channel = self.descriptor.slug,
                song_index = song_index,
                "Invalid frame range for song, skipping"
            );
            return Err(anyhow!("Invalid frame range"));
        }

        let channels = decoded.channels;
        let start = start_frame * channels;
        let end = end_frame * channels;
        let slice = decoded
            .samples
            .get(start..end)
            .ok_or_else(|| anyhow!("Sample slice out of bounds"))?;

        let track_samples = slice.to_vec();
        encode_song_to_cache(
            Arc::clone(&self.cache_manager),
            self.descriptor.id,
            self.descriptor.slug,
            block.clone(),
            *song_index,
            song.clone(),
            track_samples,
            decoded.sample_rate,
            decoded.channels,
            decoded.bits_per_sample,
            self.active_clients,
            duration_ms,
        )
        .await
    }

    /// Stocke les métadonnées Radio Paradise pour un fichier audio caché
    ///
    /// Cette fonction persiste toutes les métadonnées RP dans la base de données
    /// du cache audio, permettant leur récupération future sans dépendance aux
    /// données en mémoire.
    fn compute_track_id(&self, block: &Block, song_index: usize) -> String {
        compute_track_id_for_descriptor(self.descriptor.id, block, song_index)
    }

    async fn maybe_schedule_poll(&mut self) {
        if !matches!(self.status, ChannelLifecycle::Active) {
            return;
        }

        let buffer_len = self.playlist.active_len().await;

        let interval = if buffer_len > 3 {
            polling_high_interval()
        } else if buffer_len >= 2 {
            polling_medium_interval()
        } else {
            polling_low_interval()
        };

        self.schedule_task(ScheduledTaskKind::Poll, interval);
    }

    fn schedule_task(&mut self, kind: ScheduledTaskKind, duration: Duration) {
        self.scheduled_task = Some(ScheduledTask {
            kind,
            sleep: Box::pin(sleep(duration)),
        });
    }

    fn cancel_scheduled_task(&mut self) {
        self.scheduled_task = None;
    }

    fn scheduled_task_kind(&self) -> Option<ScheduledTaskKind> {
        self.scheduled_task.as_ref().map(|task| task.kind)
    }

    fn record_processed_block(&mut self, event: u64) {
        self.processed_blocks.insert(event);
        self.recent_blocks.push_back(event);
        let max = MAX_BLOCKS_REMEMBERED.max(1);
        while self.recent_blocks.len() > max {
            if let Some(ev) = self.recent_blocks.pop_front() {
                self.processed_blocks.remove(&ev);
            }
        }
    }

    fn is_recent_block(&self, event: u64) -> bool {
        self.processed_blocks.contains(&event)
    }

    /// Check if all songs from a block are already cached
    async fn check_all_songs_cached(&self, block: &Block) -> bool {
        let ordered_songs = block.songs_ordered();

        for (song_index, _song) in &ordered_songs {
            let track_id = self.compute_track_id(block, *song_index);

            // Check if metadata exists
            let metadata = match self.cache_manager.get_metadata(&track_id).await {
                Some(m) => m,
                None => {
                    debug!(
                        channel = self.descriptor.slug,
                        event = block.event,
                        song_index = *song_index,
                        "Song not in cache: no metadata"
                    );
                    return false;
                }
            };

            // Check if audio is cached
            let audio_pk = match metadata.cached_audio_pk {
                Some(pk) => pk,
                None => {
                    debug!(
                        channel = self.descriptor.slug,
                        event = block.event,
                        song_index = *song_index,
                        "Song not in cache: no audio_pk"
                    );
                    return false;
                }
            };

            // Check if file exists
            if self
                .cache_manager
                .audio_file_path(&audio_pk)
                .await
                .is_none()
            {
                debug!(
                    channel = self.descriptor.slug,
                    event = block.event,
                    song_index = *song_index,
                    "Song not in cache: file not found"
                );
                return false;
            }
        }

        debug!(
            channel = self.descriptor.slug,
            event = block.event,
            "All {} songs are cached",
            ordered_songs.len()
        );
        true
    }

    /// Restore songs from cache and add them to the playlist
    async fn restore_from_cache(&mut self, block: &Block) -> Result<()> {
        info!(
            channel = self.descriptor.slug,
            event = block.event,
            "Restoring block from cache (no download needed)"
        );

        let ordered_songs = block.songs_ordered();

        for (song_index, song) in &ordered_songs {
            let track_id = self.compute_track_id(block, *song_index);

            // Get metadata (we already checked it exists in check_all_songs_cached)
            let metadata = self
                .cache_manager
                .get_metadata(&track_id)
                .await
                .ok_or_else(|| anyhow!("Metadata disappeared for track_id: {}", track_id))?;

            let audio_pk = metadata
                .cached_audio_pk
                .clone()
                .ok_or_else(|| anyhow!("Audio PK disappeared for track_id: {}", track_id))?;

            // Get cover PK if available
            let cover_pk = if let Some(ref cover_path) = song.cover {
                if let Some(cover_url) = block.cover_url(cover_path) {
                    match self.cache_manager.cache_cover(&cover_url).await {
                        Ok(pk) => Some(pk),
                        Err(err) => {
                            warn!(channel = self.descriptor.slug, "Cover cache error: {err}");
                            None
                        }
                    }
                } else {
                    None
                }
            } else {
                None
            };

            // Update metadata with cover if we just cached it
            if cover_pk.is_some() && metadata.cached_cover_pk.is_none() {
                let updated_metadata = TrackMetadata {
                    cached_cover_pk: cover_pk,
                    ..metadata.clone()
                };
                self.cache_manager
                    .update_metadata(track_id.clone(), updated_metadata)
                    .await;
            }

            let file_path = self
                .cache_manager
                .audio_file_path(&audio_pk)
                .await
                .ok_or_else(|| anyhow!("File disappeared for audio_pk: {}", audio_pk))?;

            let duration_ms = song.duration;

            let entry = Arc::new(PlaylistEntry::new(
                track_id,
                self.descriptor.id,
                Arc::new((*song).clone()),
                Utc::now(),
                duration_ms,
                Some(audio_pk),
                Some(file_path),
                self.active_clients,
            ));

            self.playlist.push_active(entry).await;

            info!(
                channel = self.descriptor.slug,
                song_index = *song_index,
                "🎵 Restored '{}' from cache",
                song.title
            );
        }

        info!(
            channel = self.descriptor.slug,
            event = block.event,
            "Block restored from cache: {} songs",
            ordered_songs.len()
        );

        Ok(())
    }
}

struct ScheduledTask {
    kind: ScheduledTaskKind,
    sleep: Pin<Box<tokio::time::Sleep>>,
}

#[derive(Clone, Copy)]
enum ScheduledTaskKind {
    Poll,
    Cooling,
}

#[derive(Clone, Copy, Debug)]
enum ChannelLifecycle {
    Idle,
    Cooling,
    Active,
}

struct BackoffState {
    current: Option<Duration>,
}

impl BackoffState {
    fn new() -> Self {
        Self { current: None }
    }

    fn reset(&mut self) {
        self.current = None;
    }

    fn next_delay(&mut self) -> Duration {
        let next = match self.current {
            Some(current) => {
                let multiplied = (current.as_secs_f32() * BACKOFF_MULTIPLIER).round() as u64;
                Duration::from_secs(multiplied.min(BACKOFF_MAX_SECONDS))
            }
            None => Duration::from_secs(BACKOFF_INITIAL_SECONDS),
        };
        self.current = Some(next);
        next
    }
}

struct DecodedBlock {
    samples: Vec<i32>,
    channels: usize,
    sample_rate: u32,
    bits_per_sample: u32,
}

fn song_duration_ms(block: &Block, ordered: &[(usize, &Song)], position: usize) -> u64 {
    let song = ordered[position].1;
    if song.duration > 0 {
        return song.duration;
    }

    if let Some((_, next_song)) = ordered.get(position + 1) {
        return next_song.elapsed.saturating_sub(song.elapsed);
    }

    block.length.saturating_sub(song.elapsed)
}

fn ms_to_frames(ms: u64, sample_rate: u32) -> usize {
    ((ms as u128 * sample_rate as u128) / 1000) as usize
}

fn decode_block_audio(data: Vec<u8>) -> anyhow::Result<DecodedBlock> {
    use symphonia::core::audio::SampleBuffer;
    use symphonia::core::codecs::{CODEC_TYPE_NULL, DecoderOptions};
    use symphonia::core::errors::Error as SymphoniaError;
    use symphonia::core::formats::FormatOptions;
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;
    use symphonia::core::probe::Hint;

    let cursor = std::io::Cursor::new(data);
    let mss = MediaSourceStream::new(Box::new(cursor), Default::default());

    let hint = Hint::new();
    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map_err(|e| anyhow!("Failed to probe format: {e}"))?;

    let mut format = probed.format;

    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        .ok_or_else(|| anyhow!("No audio track found"))?;

    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|e| anyhow!("Failed to create decoder: {e}"))?;

    let channels = track
        .codec_params
        .channels
        .ok_or_else(|| anyhow!("Missing channel info"))?
        .count();

    let sample_rate = track
        .codec_params
        .sample_rate
        .ok_or_else(|| anyhow!("Missing sample rate"))?;

    let bits_per_sample = track.codec_params.bits_per_sample.unwrap_or(16);

    let mut samples_i32 = Vec::new();
    let track_id = track.id;

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
            Err(e) => return Err(anyhow!("Decode error: {e}")),
        };

        if packet.track_id() != track_id {
            continue;
        }

        match decoder.decode(&packet) {
            Ok(decoded) => {
                let spec = *decoded.spec();
                let duration = decoded.capacity() as u64;
                let mut sample_buf = SampleBuffer::<i32>::new(duration, spec);
                sample_buf.copy_interleaved_ref(decoded);
                samples_i32.extend_from_slice(sample_buf.samples());
            }
            Err(SymphoniaError::DecodeError(_)) => continue,
            Err(e) => return Err(anyhow!("Decode error: {e}")),
        }
    }

    if samples_i32.is_empty() {
        return Err(anyhow!("No samples decoded"));
    }

    let (normalized_samples, target_bits): (Vec<i32>, u32) = match bits_per_sample {
        0..=16 => {
            let samples = samples_i32.iter().map(|&s| (s >> 16) as i32).collect();
            (samples, 16)
        }
        17..=24 => {
            let samples = samples_i32.iter().map(|&s| (s >> 8) as i32).collect();
            (samples, 24)
        }
        _ => (samples_i32, 32),
    };

    Ok(DecodedBlock {
        samples: normalized_samples,
        channels,
        sample_rate,
        bits_per_sample: target_bits,
    })
}

fn compute_track_id_for_descriptor(descriptor_id: u8, block: &Block, song_index: usize) -> String {
    format!(
        "rp:{}:event_{}_song_{}",
        descriptor_id, block.event, song_index
    )
}

async fn store_rp_metadata(
    cache_manager: &SourceCacheManager,
    audio_pk: &str,
    track_id: &str,
    channel_id: u8,
    song: &Song,
    duration_ms: u64,
    event: u64,
    cover_pk: Option<&str>,
) -> Result<()> {
    use serde_json::json;

    cache_manager.set_audio_metadata(audio_pk, "rp_title", json!(song.title))?;
    cache_manager.set_audio_metadata(audio_pk, "rp_artist", json!(song.artist))?;
    cache_manager.set_audio_metadata(audio_pk, "rp_album", json!(song.album))?;
    cache_manager.set_audio_metadata(audio_pk, "rp_year", json!(song.year))?;

    cache_manager.set_audio_metadata(audio_pk, "rp_duration_ms", json!(duration_ms))?;
    cache_manager.set_audio_metadata(audio_pk, "rp_elapsed_ms", json!(song.elapsed))?;

    cache_manager.set_audio_metadata(audio_pk, "rp_track_id", json!(track_id))?;
    cache_manager.set_audio_metadata(audio_pk, "rp_channel_id", json!(channel_id))?;
    cache_manager.set_audio_metadata(audio_pk, "rp_event", json!(event))?;

    cache_manager.set_audio_metadata(audio_pk, "rp_rating", json!(song.rating))?;
    cache_manager.set_audio_metadata(audio_pk, "rp_cover_url", json!(song.cover))?;
    cache_manager.set_audio_metadata(audio_pk, "rp_cover_pk", json!(cover_pk))?;

    Ok(())
}

async fn cache_cover_for_song(
    cache_manager: &SourceCacheManager,
    slug: &'static str,
    block: &Block,
    song: &Song,
) -> Result<Option<String>> {
    if let Some(ref cover_path) = song.cover {
        if let Some(cover_url) = block.cover_url(cover_path) {
            match cache_manager.cache_cover(&cover_url).await {
                Ok(pk) => return Ok(Some(pk)),
                Err(err) => {
                    warn!(channel = slug, "Cover cache error: {err}");
                }
            }
        } else {
            warn!(
                channel = slug,
                "Unable to resolve cover URL for {}", cover_path
            );
        }
    }
    Ok(None)
}

async fn encode_song_to_cache(
    cache_manager: Arc<SourceCacheManager>,
    descriptor_id: u8,
    slug: &'static str,
    block: Block,
    song_index: usize,
    song: Song,
    track_samples: Vec<i32>,
    sample_rate: u32,
    channels: usize,
    bits_per_sample: u32,
    active_clients: usize,
    duration_ms: u64,
) -> Result<Arc<PlaylistEntry>> {
    let flac_bytes = encode_samples_to_flac(track_samples, channels, sample_rate, bits_per_sample)
        .await
        .context("Failed to encode song to FLAC")?;

    let track_id = compute_track_id_for_descriptor(descriptor_id, &block, song_index);
    let placeholder_uri = format!("{}#{}", block.url, song_index);

    let mut metadata = TrackMetadata {
        original_uri: placeholder_uri.clone(),
        cached_audio_pk: None,
        cached_cover_pk: None,
    };

    if let Some(cover_pk) =
        cache_cover_for_song(cache_manager.as_ref(), slug, &block, &song).await?
    {
        metadata.cached_cover_pk = Some(cover_pk);
    }

    let flac_len = flac_bytes.len() as u64;
    let reader = StreamReader::new(stream::iter(vec![Ok::<_, std::io::Error>(Bytes::from(
        flac_bytes,
    ))]));

    let audio_pk = cache_manager
        .cache_audio_from_reader(&track_id, reader, Some(flac_len))
        .await
        .map_err(|e| anyhow!("Cache audio error: {e}"))?;

    cache_manager
        .wait_audio_ready(&audio_pk)
        .await
        .map_err(|e| anyhow!("Wait audio ready error: {e}"))?;

    metadata.cached_audio_pk = Some(audio_pk.clone());
    cache_manager
        .update_metadata(track_id.clone(), metadata.clone())
        .await;

    if let Err(e) = store_rp_metadata(
        cache_manager.as_ref(),
        &audio_pk,
        &track_id,
        descriptor_id,
        &song,
        duration_ms,
        block.event,
        metadata.cached_cover_pk.as_deref(),
    )
    .await
    {
        warn!(channel = slug, "Failed to store RP metadata: {e:?}");
    }

    let file_path = cache_manager.audio_file_path(&audio_pk).await;

    let entry = Arc::new(PlaylistEntry::new(
        track_id,
        descriptor_id,
        Arc::new(song.clone()),
        Utc::now(),
        duration_ms,
        Some(audio_pk),
        file_path,
        active_clients,
    ));

    Ok(entry)
}

fn spawn_song_processing(
    context: SongTaskContext,
    block: Block,
    song_index: usize,
    song: Song,
    track_samples: Vec<i32>,
    sample_rate: u32,
    channels: usize,
    bits_per_sample: u32,
    active_clients: usize,
    duration_ms: u64,
    position_ms: u64,
) {
    tokio::spawn(async move {
        let SongTaskContext {
            cache_manager,
            playlist,
            descriptor_id,
            slug,
        } = context;

        let song_title = song.title.clone();

        match encode_song_to_cache(
            cache_manager,
            descriptor_id,
            slug,
            block,
            song_index,
            song,
            track_samples,
            sample_rate,
            channels,
            bits_per_sample,
            active_clients,
            duration_ms,
        )
        .await
        {
            Ok(entry) => {
                playlist.push_active(entry).await;
                info!(
                    channel = slug,
                    song_index = song_index,
                    "🎵 Song '{}' available after {}ms (streaming mode)",
                    song_title,
                    position_ms
                );
            }
            Err(err) => {
                warn!(
                    channel = slug,
                    song_index = song_index,
                    "Failed to process song '{}' asynchronously: {err:?}",
                    song_title
                );
            }
        }
    });
}

async fn encode_samples_to_flac(
    samples: Vec<i32>,
    channels: usize,
    sample_rate: u32,
    bits_per_sample: u32,
) -> anyhow::Result<Vec<u8>> {
    tokio::task::spawn_blocking(move || {
        use flacenc::bitsink::ByteSink;
        use flacenc::component::BitRepr;
        use flacenc::error::Verify;

        // Note: Claxon retourne les samples dans leur résolution native
        // Un fichier FLAC 16 bits retourne des samples i32 avec des valeurs dans la plage i16
        // Pas besoin de normalisation supplémentaire
        let config = flacenc::config::Encoder::default()
            .into_verified()
            .map_err(|e| anyhow!("FLAC config error: {e:?}"))?;

        let source = flacenc::source::MemSource::from_samples(
            &samples,
            channels,
            bits_per_sample as usize,
            sample_rate as usize,
        );

        let flac_stream = flacenc::encode_with_fixed_block_size(&config, source, config.block_size)
            .map_err(|e| anyhow!("FLAC encode error: {e:?}"))?;

        let mut sink = ByteSink::new();
        flac_stream
            .write(&mut sink)
            .map_err(|e| anyhow!("FLAC write error: {e:?}"))?;

        Ok::<_, anyhow::Error>(sink.into_inner())
    })
    .await?
}

/// Métadonnées Radio Paradise récupérées depuis le cache
///
/// Cette structure contient toutes les métadonnées RP stockées de manière
/// persistante dans le cache audio.
#[derive(Debug, Clone)]
pub struct RadioParadiseMetadata {
    /// Titre de la chanson
    pub title: String,
    /// Artiste
    pub artist: String,
    /// Album (optionnel)
    pub album: Option<String>,
    /// Année de sortie (optionnelle)
    pub year: Option<u32>,
    /// Durée en millisecondes
    pub duration_ms: u64,
    /// Offset depuis le début du block en millisecondes
    pub elapsed_ms: u64,
    /// Identifiant unique de la piste
    pub track_id: String,
    /// ID du canal Radio Paradise (0-3)
    pub channel_id: u8,
    /// ID de l'événement (block)
    pub event: u64,
    /// Note de la chanson (0-10, optionnelle)
    pub rating: Option<f32>,
    /// URL de la couverture (optionnelle)
    pub cover_url: Option<String>,
    /// PK de la couverture dans le cache (optionnelle)
    pub cover_pk: Option<String>,
}

/// Charge les métadonnées Radio Paradise depuis le cache audio
///
/// Cette fonction lit toutes les métadonnées RP stockées pour un fichier
/// audio donné et les retourne dans une structure `RadioParadiseMetadata`.
///
/// # Arguments
///
/// * `cache_manager` - Le gestionnaire de cache source
/// * `audio_pk` - Clé primaire du fichier audio dans le cache
///
/// # Returns
///
/// Les métadonnées RP si elles existent et sont complètes, sinon une erreur.
///
/// # Erreurs
///
/// Cette fonction retourne une erreur si :
/// - Les métadonnées n'existent pas dans le cache
/// - Les métadonnées sont incomplètes ou corrompues
/// - Il y a une erreur de lecture du cache
pub async fn load_rp_metadata(
    cache_manager: &SourceCacheManager,
    audio_pk: &str,
) -> Result<RadioParadiseMetadata> {
    // Helper macro pour récupérer une métadonnée requise
    macro_rules! get_required {
        ($key:expr, $type:ty) => {{
            cache_manager
                .get_audio_metadata(audio_pk, $key)?
                .and_then(|v| serde_json::from_value::<$type>(v).ok())
                .ok_or_else(|| anyhow!("Missing or invalid metadata: {}", $key))?
        }};
    }

    // Helper macro pour récupérer une métadonnée optionnelle
    macro_rules! get_optional {
        ($key:expr, $type:ty) => {{
            cache_manager
                .get_audio_metadata(audio_pk, $key)?
                .and_then(|v| {
                    if v.is_null() {
                        None
                    } else {
                        serde_json::from_value::<$type>(v).ok()
                    }
                })
        }};
    }

    Ok(RadioParadiseMetadata {
        title: get_required!("rp_title", String),
        artist: get_required!("rp_artist", String),
        album: get_optional!("rp_album", String),
        year: get_optional!("rp_year", u32),
        duration_ms: get_required!("rp_duration_ms", u64),
        elapsed_ms: get_required!("rp_elapsed_ms", u64),
        track_id: get_required!("rp_track_id", String),
        channel_id: get_required!("rp_channel_id", u8),
        event: get_required!("rp_event", u64),
        rating: get_optional!("rp_rating", f32),
        cover_url: get_optional!("rp_cover_url", String),
        cover_pk: get_optional!("rp_cover_pk", String),
    })
}
