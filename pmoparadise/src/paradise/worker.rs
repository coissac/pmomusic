//! Background worker for Radio Paradise channels.
//!
//! The worker handles API polling, block ingestion, caching and playlist
//! maintenance.  It keeps the channel state in sync with connected clients
//! and ensures fresh content is available according to the specification.

use super::channel::ChannelDescriptor;
use super::config::RadioParadiseConfig;
use super::history::HistoryBackend;
use super::playlist::{PlaylistEntry, SharedPlaylist};
use crate::client::RadioParadiseClient;
use crate::models::{Block, Song};
use anyhow::{anyhow, Context, Result};
use bytes::Bytes;
use chrono::Utc;
use futures::stream;
use pmosource::{SourceCacheManager, TrackMetadata};
use sha2::{Digest, Sha256};
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
        config: Arc<RadioParadiseConfig>,
        playlist: SharedPlaylist,
        history: Arc<dyn HistoryBackend>,
        cache_manager: Arc<SourceCacheManager>,
    ) -> (Self, mpsc::Sender<WorkerCommand>) {
        let (tx, mut rx) = mpsc::channel(32);

        let join_handle = tokio::spawn(async move {
            info!(channel = descriptor.slug, "Starting Radio Paradise worker");

            let mut state =
                WorkerState::new(descriptor, client, config, playlist, history, cache_manager);

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
    config: Arc<RadioParadiseConfig>,
    playlist: SharedPlaylist,
    history: Arc<dyn HistoryBackend>,
    cache_manager: Arc<SourceCacheManager>,
    active_clients: usize,
    status: ChannelLifecycle,
    processed_blocks: HashSet<u64>,
    recent_blocks: VecDeque<u64>,
    next_block_hint: Option<u64>,
    scheduled_task: Option<ScheduledTask>,
    backoff: BackoffState,
    shutdown: bool,
}

impl WorkerState {
    fn new(
        descriptor: ChannelDescriptor,
        client: RadioParadiseClient,
        config: Arc<RadioParadiseConfig>,
        playlist: SharedPlaylist,
        history: Arc<dyn HistoryBackend>,
        cache_manager: Arc<SourceCacheManager>,
    ) -> Self {
        Self {
            descriptor,
            client,
            config,
            playlist,
            history,
            cache_manager,
            active_clients: 0,
            status: ChannelLifecycle::Idle,
            processed_blocks: HashSet::new(),
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
        let delay = self
            .backoff
            .next_delay(&self.config.polling.backoff_on_error);
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
        let duration = Duration::from_secs(self.config.activity.cooling_timeout_seconds.max(1));
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
        if self.is_recent_block(block.event) {
            debug!(
                channel = self.descriptor.slug,
                event = block.event,
                "Skipping already processed block"
            );
            self.next_block_hint = Some(block.end_event);
            return Ok(());
        }

        info!(
            channel = self.descriptor.slug,
            event = block.event,
            "Processing Radio Paradise block"
        );

        let _ = &self.history;

        let block_url = Url::parse(&block.url)?;
        let block_bytes = self
            .client
            .download_block(&block_url)
            .await
            .context("Failed to download block")?;

        let decoded = decode_block_audio(block_bytes.to_vec())?;
        let ordered_songs = block.songs_ordered();
        let total_frames = decoded.samples.len() / decoded.channels;

        for (position, (song_index, song)) in ordered_songs.iter().enumerate() {
            let track = self
                .process_song(
                    &block,
                    song_index,
                    song,
                    position,
                    &ordered_songs,
                    total_frames,
                    &decoded,
                )
                .await?;

            self.playlist.push_active(track.clone()).await;
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
        let flac_bytes = encode_samples_to_flac(
            track_samples,
            decoded.channels,
            decoded.sample_rate,
            decoded.bits_per_sample,
        )
        .await
        .context("Failed to encode song to FLAC")?;

        let track_id = self.compute_track_id(&flac_bytes);
        let placeholder_uri = format!("{}#{}", block.url, song_index);

        let mut metadata = TrackMetadata {
            original_uri: placeholder_uri.clone(),
            cached_audio_pk: None,
            cached_cover_pk: None,
        };

        if let Some(cover_pk) = self.cache_cover(block, song).await? {
            metadata.cached_cover_pk = Some(cover_pk);
        }

        let flac_len = flac_bytes.len() as u64;
        let reader = StreamReader::new(stream::iter(vec![Ok::<_, std::io::Error>(Bytes::from(
            flac_bytes,
        ))]));

        let audio_pk = self
            .cache_manager
            .cache_audio_from_reader(&track_id, reader, Some(flac_len))
            .await
            .map_err(|e| anyhow!("Cache audio error: {e}"))?;

        metadata.cached_audio_pk = Some(audio_pk.clone());
        self.cache_manager
            .update_metadata(track_id.clone(), metadata)
            .await;

        let file_path = self.cache_manager.audio_file_path(&audio_pk).await;

        let entry = Arc::new(PlaylistEntry::new(
            track_id,
            self.descriptor.id,
            Arc::new(song.clone()),
            Utc::now(),
            duration_ms,
            Some(audio_pk),
            file_path,
            self.active_clients,
        ));

        Ok(entry)
    }

    async fn cache_cover(&self, block: &Block, song: &Song) -> Result<Option<String>> {
        if let Some(ref cover_path) = song.cover {
            let cover_url =
                resolve_cover_url(block.image_base.as_deref(), &self.client, cover_path)
                    .context("Invalid cover URL")?;
            match self.cache_manager.cache_cover(cover_url.as_str()).await {
                Ok(pk) => return Ok(Some(pk)),
                Err(err) => {
                    warn!(channel = self.descriptor.slug, "Cover cache error: {err}");
                }
            }
        }
        Ok(None)
    }

    fn compute_track_id(&self, flac_bytes: &[u8]) -> String {
        let slice_len = flac_bytes.len().min(self.config.cache.track_id_hash_bytes);
        let mut hasher = Sha256::new();
        hasher.update(&flac_bytes[..slice_len]);
        let hash = hasher.finalize();
        format!("rp:{}:{}", self.descriptor.id, hex::encode(hash))
    }

    async fn maybe_schedule_poll(&mut self) {
        if !matches!(self.status, ChannelLifecycle::Active) {
            return;
        }

        let buffer_len = self.playlist.active_len().await;

        let interval = if buffer_len > 3 {
            self.config.polling.high_interval()
        } else if buffer_len >= 2 {
            self.config.polling.medium_interval()
        } else {
            self.config.polling.low_interval()
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
        let max = self.config.cache.max_blocks_remembered.max(1);
        while self.recent_blocks.len() > max {
            if let Some(ev) = self.recent_blocks.pop_front() {
                self.processed_blocks.remove(&ev);
            }
        }
    }

    fn is_recent_block(&self, event: u64) -> bool {
        self.processed_blocks.contains(&event)
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

    fn next_delay(&mut self, config: &super::config::PollingBackoffConfig) -> Duration {
        let next = match self.current {
            Some(current) => {
                let multiplied = (current.as_secs_f32() * config.multiplier).round() as u64;
                Duration::from_secs(multiplied.min(config.max))
            }
            None => Duration::from_secs(config.initial),
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
    use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
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

fn resolve_cover_url(
    image_base: Option<&str>,
    client: &RadioParadiseClient,
    cover: &str,
) -> Result<Url> {
    if cover.starts_with("http://") || cover.starts_with("https://") {
        return Url::parse(cover).map_err(|e| anyhow!("Invalid cover URL '{cover}': {e}"));
    }

    if cover.starts_with("//") {
        let url = format!("https:{cover}");
        return Url::parse(&url).map_err(|e| anyhow!("Invalid cover URL '{cover}': {e}"));
    }

    if let Some(base) = image_base {
        match Url::parse(base).and_then(|base_url| base_url.join(cover)) {
            Ok(url) => return Ok(url),
            Err(err) => {
                debug!("Failed to join cover '{cover}' with base '{base}': {err}");
            }
        }
    }

    client
        .cover_url(cover)
        .map_err(|e| anyhow!("Invalid cover URL '{cover}': {e}"))
}
