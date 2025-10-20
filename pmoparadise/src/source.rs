//! Music source implementation for Radio Paradise
//!
//! This module implements the [`pmosource::MusicSource`] trait for Radio Paradise,
//! providing a complete music source with FIFO playlist support, browsing, and caching.

use crate::client::RadioParadiseClient;
use crate::models::{Block, Song};
use anyhow::anyhow;
use pmoaudiocache::Cache as AudioCache;
use pmocovers::Cache as CoverCache;
use pmodidl::{Container, Item};
use pmoplaylist::{FifoPlaylist, Track};
use pmosource::SourceCacheManager;
use pmosource::{async_trait, pmodidl, BrowseResult, MusicSource, MusicSourceError, Result};
use std::collections::{HashMap, HashSet};
use std::io::Cursor;
use std::sync::Arc;
use std::time::SystemTime;
use tokio::sync::{Mutex, RwLock};
use url::Url;

/// Default image for Radio Paradise (300x300 WebP, embedded in binary)
const DEFAULT_IMAGE: &[u8] = include_bytes!("../assets/default.webp");

/// Default FIFO capacity (number of recent tracks to keep)
const DEFAULT_FIFO_CAPACITY: usize = 50;

#[derive(Clone, Copy)]
struct ChannelDescriptor {
    id: u8,
    name: &'static str,
    description: &'static str,
}

const CHANNELS: [ChannelDescriptor; 4] = [
    ChannelDescriptor {
        id: 0,
        name: "Main Mix",
        description: "Eclectic mix of rock, world, electronica, and more",
    },
    ChannelDescriptor {
        id: 1,
        name: "Mellow Mix",
        description: "Mellower, less aggressive music",
    },
    ChannelDescriptor {
        id: 2,
        name: "Rock Mix",
        description: "Heavier, more guitar-driven music",
    },
    ChannelDescriptor {
        id: 3,
        name: "World Mix",
        description: "Global beats and world music",
    },
];

fn channel_collection_id(channel_id: u8) -> String {
    format!("radio-paradise:{}", channel_id)
}

fn channel_container_id(channel_id: u8) -> String {
    format!("radio-paradise:channel:{}", channel_id)
}

fn channel_playlist_id(channel_id: u8) -> String {
    channel_container_id(channel_id)
}

fn parse_channel_container_id(object_id: &str) -> Option<u8> {
    let mut parts = object_id.split(':');
    match (parts.next(), parts.next(), parts.next(), parts.next()) {
        (Some("radio-paradise"), Some("channel"), Some(id_str), None) => id_str.parse().ok(),
        _ => None,
    }
}

fn track_identifier(channel_id: u8, event: u64, song_index: usize) -> String {
    format!("rp:{}:{}:{}", channel_id, event, song_index)
}

fn parse_track_identifier(track_id: &str) -> Option<(u8, u64, usize)> {
    let mut parts = track_id.split(':');
    match (
        parts.next(),
        parts.next(),
        parts.next(),
        parts.next(),
        parts.next(),
    ) {
        (Some("rp"), Some(channel_str), Some(event_str), Some(index_str), None) => {
            let channel = channel_str.parse().ok()?;
            let event = event_str.parse().ok()?;
            let idx = index_str.parse().ok()?;
            Some((channel, event, idx))
        }
        _ => None,
    }
}

fn resolve_cover_url(
    image_base: Option<&str>,
    client: &RadioParadiseClient,
    cover: &str,
) -> anyhow::Result<Url> {
    if cover.starts_with("http://") || cover.starts_with("https://") {
        return Url::parse(cover).map_err(|e| anyhow!("Invalid cover URL '{}': {}", cover, e));
    }

    if cover.starts_with("//") {
        let url = format!("https:{}", cover);
        return Url::parse(&url).map_err(|e| anyhow!("Invalid cover URL '{}': {}", cover, e));
    }

    if let Some(base) = image_base {
        match Url::parse(base).and_then(|base_url| base_url.join(cover)) {
            Ok(url) => return Ok(url),
            Err(err) => {
                tracing::debug!(
                    "Failed to join cover '{}' with image base '{}': {}",
                    cover,
                    base,
                    err
                );
            }
        }
    }

    client
        .cover_url(cover)
        .map_err(|e| anyhow!("Invalid cover URL '{}': {}", cover, e))
}

/// Radio Paradise music source with full MusicSource trait implementation
///
/// This struct combines a [`RadioParadiseClient`] for API access with a FIFO playlist
/// for dynamic track management, implementing the complete [`MusicSource`] trait.
///
/// # Features
///
/// - **FIFO Playlist**: Dynamic track management with configurable capacity
/// - **API Integration**: Fetches blocks and metadata from Radio Paradise
/// - **URI Resolution**: Resolves track URIs with optional cache support
/// - **Change Tracking**: Tracks update_id and last_change for UPnP notifications
/// - **DIDL-Lite Export**: Converts tracks and blocks to UPnP-compatible formats
///
/// # Examples
///
/// ```no_run
/// use pmoparadise::{RadioParadiseClient, RadioParadiseSource};
/// use pmosource::MusicSource;
/// use std::sync::Arc;
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let client = RadioParadiseClient::new().await?;
///
///     let base_dir = std::env::temp_dir().join("pmoparadise_doc_source");
///     let cover_dir = base_dir.join("covers");
///     let audio_dir = base_dir.join("audio");
///     std::fs::create_dir_all(&cover_dir)?;
///     std::fs::create_dir_all(&audio_dir)?;
///
///     let cover_dir_str = cover_dir.to_string_lossy().into_owned();
///     let audio_dir_str = audio_dir.to_string_lossy().into_owned();
///     let cover_cache = Arc::new(pmocovers::cache::new_cache(&cover_dir_str, 32)?);
///     let audio_cache = Arc::new(pmoaudiocache::cache::new_cache(&audio_dir_str, 32)?);
///
///     let source = RadioParadiseSource::new(client, 50, cover_cache, audio_cache);
///
///     println!("Source: {}", source.name());
///     println!("Supports FIFO: {}", source.supports_fifo());
///
///     // Start streaming and the FIFO will be populated
///     Ok(())
/// }
/// ```
#[derive(Clone)]
pub struct RadioParadiseSource {
    inner: Arc<RadioParadiseSourceInner>,
}

struct ChannelState {
    descriptor: ChannelDescriptor,
    client: RadioParadiseClient,
    playlist: FifoPlaylist,
    cache_manager: SourceCacheManager,
    processed_blocks: RwLock<HashSet<u64>>,
    ingest_lock: Mutex<()>,
}

struct DecodedBlock {
    samples: Vec<i32>,
    channels: usize,
    sample_rate: u32,
    bits_per_sample: u32,
}

struct RadioParadiseSourceInner {
    channels: HashMap<u8, Arc<ChannelState>>,
}

impl std::fmt::Debug for RadioParadiseSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RadioParadiseSource").finish()
    }
}

impl RadioParadiseSource {
    /// Create a new Radio Paradise source from the cache registry
    ///
    /// This is the recommended way to create a source when using the UPnP server.
    /// The caches are automatically retrieved from the global registry.
    ///
    /// # Arguments
    ///
    /// * `client` - Radio Paradise API client
    /// * `fifo_capacity` - Maximum number of tracks in the FIFO
    ///
    /// # Errors
    ///
    /// Returns an error if the caches are not initialized in the registry
    #[cfg(feature = "server")]
    pub fn from_registry(client: RadioParadiseClient, fifo_capacity: usize) -> Result<Self> {
        let mut channels = HashMap::new();

        for descriptor in CHANNELS.iter() {
            let channel_id = descriptor.id;
            let channel_client = client.clone_with_channel(channel_id);
            let playlist = FifoPlaylist::new(
                channel_playlist_id(channel_id),
                descriptor.name.to_string(),
                fifo_capacity,
                DEFAULT_IMAGE,
            );

            let cache_manager =
                SourceCacheManager::from_registry(channel_collection_id(channel_id))?;

            channels.insert(
                channel_id,
                Arc::new(ChannelState {
                    descriptor: *descriptor,
                    client: channel_client,
                    playlist,
                    cache_manager,
                    processed_blocks: RwLock::new(HashSet::new()),
                    ingest_lock: Mutex::new(()),
                }),
            );
        }

        Ok(Self {
            inner: Arc::new(RadioParadiseSourceInner { channels }),
        })
    }

    /// Create with default FIFO capacity from the cache registry
    #[cfg(feature = "server")]
    pub fn from_registry_default(client: RadioParadiseClient) -> Result<Self> {
        Self::from_registry(client, DEFAULT_FIFO_CAPACITY)
    }

    /// Create a new Radio Paradise source with explicit caches (for tests)
    ///
    /// # Arguments
    ///
    /// * `client` - Radio Paradise API client
    /// * `fifo_capacity` - Maximum number of tracks in the FIFO
    /// * `cover_cache` - Cover image cache (required)
    /// * `audio_cache` - Audio cache (required)
    pub fn new(
        client: RadioParadiseClient,
        fifo_capacity: usize,
        cover_cache: Arc<CoverCache>,
        audio_cache: Arc<AudioCache>,
    ) -> Self {
        let mut channels = HashMap::new();

        for descriptor in CHANNELS.iter() {
            let channel_id = descriptor.id;
            let channel_client = client.clone_with_channel(channel_id);
            let playlist = FifoPlaylist::new(
                channel_playlist_id(channel_id),
                descriptor.name.to_string(),
                fifo_capacity,
                DEFAULT_IMAGE,
            );

            let cache_manager = SourceCacheManager::new(
                channel_collection_id(channel_id),
                Arc::clone(&cover_cache),
                Arc::clone(&audio_cache),
            );

            channels.insert(
                channel_id,
                Arc::new(ChannelState {
                    descriptor: *descriptor,
                    client: channel_client,
                    playlist,
                    cache_manager,
                    processed_blocks: RwLock::new(HashSet::new()),
                    ingest_lock: Mutex::new(()),
                }),
            );
        }

        Self {
            inner: Arc::new(RadioParadiseSourceInner { channels }),
        }
    }

    /// Create with default FIFO capacity (for tests)
    pub fn new_default(
        client: RadioParadiseClient,
        cover_cache: Arc<CoverCache>,
        audio_cache: Arc<AudioCache>,
    ) -> Self {
        Self::new(client, DEFAULT_FIFO_CAPACITY, cover_cache, audio_cache)
    }

    /// Get the Radio Paradise client for a given channel
    pub fn client_for_channel(&self, channel: u8) -> Option<RadioParadiseClient> {
        self.inner
            .channels
            .get(&channel)
            .map(|state| state.client.clone())
    }

    fn channel_state(&self, channel_id: u8) -> Option<Arc<ChannelState>> {
        self.inner.channels.get(&channel_id).cloned()
    }

    fn build_root_container(&self) -> Container {
        Container {
            id: "radio-paradise".to_string(),
            parent_id: "0".to_string(),
            restricted: Some("1".to_string()),
            child_count: Some(CHANNELS.len().to_string()),
            title: "Radio Paradise".to_string(),
            class: "object.container".to_string(),
            containers: vec![],
            items: vec![],
        }
    }

    async fn build_channel_containers(&self) -> Vec<Container> {
        let mut containers = Vec::new();
        for descriptor in CHANNELS.iter() {
            if let Some(channel) = self.channel_state(descriptor.id) {
                let child_count = channel.playlist.len().await;
                containers.push(Container {
                    id: channel_container_id(descriptor.id),
                    parent_id: "radio-paradise".to_string(),
                    restricted: Some("1".to_string()),
                    child_count: Some(child_count.to_string()),
                    title: descriptor.name.to_string(),
                    class: "object.container.playlistContainer".to_string(),
                    containers: vec![],
                    items: vec![],
                });
            }
        }
        containers
    }

    async fn ensure_channel_ready(&self, channel: Arc<ChannelState>) -> Result<()> {
        if channel.playlist.len().await > 0 {
            return Ok(());
        }

        let guard = channel.ingest_lock.lock().await;
        if channel.playlist.len().await == 0 {
            drop(guard);
            self.populate_channel_locked(channel.clone()).await?;
        } else {
            drop(guard);
        }

        Ok(())
    }

    async fn prepare_initial_track(
        &self,
        channel: Arc<ChannelState>,
        block: Arc<Block>,
    ) -> Result<()> {
        let ordered_songs = block.songs_ordered();
        let (song_index, song) = match ordered_songs.first() {
            Some(entry) => entry,
            None => return Ok(()),
        };

        let track_id = track_identifier(channel.descriptor.id, block.event, *song_index);

        if channel.playlist.has_track(&track_id).await {
            return Ok(());
        }

        let placeholder_uri = format!("{}#{}", block.url, *song_index);

        channel
            .cache_manager
            .update_metadata(
                track_id.clone(),
                pmosource::TrackMetadata {
                    original_uri: placeholder_uri.clone(),
                    cached_audio_pk: None,
                    cached_cover_pk: None,
                },
            )
            .await;

        let mut track = Track::new(
            track_id.clone(),
            song.title.clone(),
            placeholder_uri.clone(),
        );

        if !song.artist.is_empty() {
            track = track.with_artist(song.artist.clone());
        }

        if let Some(ref album) = song.album {
            if !album.is_empty() {
                track = track.with_album(album.clone());
            }
        }

        let duration_ms = song_duration_ms(&block, &ordered_songs, 0);
        if duration_ms > 0 {
            track = track.with_duration((duration_ms / 1000) as u32);
        }

        if let Some(ref cover) = song.cover {
            if let Ok(url) = resolve_cover_url(block.image_base.as_deref(), &channel.client, cover)
            {
                track = track.with_image(url.to_string());
            }
        }

        channel.playlist.append_track(track).await;

        Ok(())
    }

    async fn populate_channel_locked(&self, channel: Arc<ChannelState>) -> Result<()> {
        tracing::info!(
            "📻 Fetching Radio Paradise block for channel {}",
            channel.descriptor.name
        );

        let now_playing = channel
            .client
            .now_playing()
            .await
            .map_err(|e| MusicSourceError::SourceUnavailable(e.to_string()))?;

        let block = Arc::new(now_playing.block);
        self.prepare_initial_track(channel.clone(), block.clone())
            .await?;

        let source_clone = self.clone();
        tokio::spawn(async move {
            if let Err(e) = source_clone
                .ingest_block(channel.clone(), block.clone())
                .await
            {
                tracing::error!(
                    "Failed to ingest block {} on channel {}: {}",
                    block.event,
                    channel.descriptor.name,
                    e
                );
            }
        });

        Ok(())
    }

    async fn ingest_block(&self, channel: Arc<ChannelState>, block: Arc<Block>) -> Result<()> {
        {
            let mut processed = channel.processed_blocks.write().await;
            if !processed.insert(block.event) {
                tracing::debug!(
                    "Channel {} already processed block {}",
                    channel.descriptor.name,
                    block.event
                );
                return Ok(());
            }
        }

        let block_url = Url::parse(&block.url)
            .map_err(|e| MusicSourceError::BrowseError(format!("Invalid block URL: {}", e)))?;

        let block_bytes = channel
            .client
            .download_block(&block_url)
            .await
            .map_err(|e| {
                MusicSourceError::BrowseError(format!("Failed to download block: {}", e))
            })?;

        let decoded = decode_block_audio(block_bytes.to_vec())
            .map_err(|e| MusicSourceError::BrowseError(e.to_string()))?;

        let ordered_songs = block.songs_ordered();
        let total_frames = decoded.samples.len() / decoded.channels;

        for (position, (song_index, song)) in ordered_songs.iter().enumerate() {
            let track_id = track_identifier(channel.descriptor.id, block.event, *song_index);
            let placeholder_uri = format!("{}#{}", block.url, *song_index);

            let existing_metadata = channel.cache_manager.get_metadata(&track_id).await;
            if let Some(ref metadata) = existing_metadata {
                if metadata.cached_audio_pk.is_some() {
                    continue;
                }
            } else {
                channel
                    .cache_manager
                    .update_metadata(
                        track_id.clone(),
                        pmosource::TrackMetadata {
                            original_uri: placeholder_uri.clone(),
                            cached_audio_pk: None,
                            cached_cover_pk: None,
                        },
                    )
                    .await;
            }

            let duration_ms = song_duration_ms(&block, &ordered_songs, position);
            if duration_ms == 0 {
                tracing::debug!(
                    "Skipping track {} with zero duration on channel {}",
                    track_id,
                    channel.descriptor.name
                );
                continue;
            }

            let start_frame = ms_to_frames(song.elapsed, decoded.sample_rate);
            let end_frame =
                ms_to_frames(song.elapsed + duration_ms, decoded.sample_rate).min(total_frames);

            if start_frame >= end_frame {
                tracing::debug!(
                    "Invalid frame range for track {} (start {} >= end {})",
                    track_id,
                    start_frame,
                    end_frame
                );
                continue;
            }

            let start_index = start_frame * decoded.channels;
            let end_index = end_frame * decoded.channels;
            let song_samples = decoded.samples[start_index..end_index].to_vec();

            let flac_data = encode_samples_to_flac(
                song_samples,
                decoded.channels,
                decoded.sample_rate,
                decoded.bits_per_sample,
            )
            .await
            .map_err(|e| MusicSourceError::CacheError(e.to_string()))?;

            let audio_source_uri = format!("{}#{}", block.url, song_index);
            let data_len = flac_data.len() as u64;
            let reader = Cursor::new(flac_data);
            let audio_pk: String = channel
                .cache_manager
                .cache_audio_from_reader(&audio_source_uri, reader, Some(data_len))
                .await?;

            let resolved_cover_url =
                song.cover.as_ref().and_then(|cover| {
                    match resolve_cover_url(block.image_base.as_deref(), &channel.client, cover) {
                        Ok(url) => Some(url.to_string()),
                        Err(e) => {
                            tracing::warn!(
                                "Failed to resolve cover '{}' for channel {}: {}",
                                cover,
                                channel.descriptor.name,
                                e
                            );
                            None
                        }
                    }
                });

            let cached_cover_pk = if let Some(ref cover_url) = resolved_cover_url {
                match channel.cache_manager.cache_cover(cover_url).await {
                    Ok(pk) => Some(pk),
                    Err(e) => {
                        tracing::warn!(
                            "Failed to cache cover {} on channel {}: {}",
                            cover_url,
                            channel.descriptor.name,
                            e
                        );
                        None
                    }
                }
            } else {
                None
            };

            let metadata_cover_pk = cached_cover_pk.clone();
            channel
                .cache_manager
                .update_metadata(
                    track_id.clone(),
                    pmosource::TrackMetadata {
                        original_uri: existing_metadata
                            .and_then(|m| {
                                if m.original_uri.is_empty() {
                                    None
                                } else {
                                    Some(m.original_uri)
                                }
                            })
                            .unwrap_or_else(|| placeholder_uri.clone()),
                        cached_audio_pk: Some(audio_pk.clone()),
                        cached_cover_pk: metadata_cover_pk,
                    },
                )
                .await;

            let playback_url = channel.cache_manager.resolve_uri(&track_id).await?;

            let mut track = Track::new(track_id.clone(), song.title.clone(), playback_url);

            if !song.artist.is_empty() {
                track = track.with_artist(song.artist.clone());
            }

            if let Some(ref album) = song.album {
                if !album.is_empty() {
                    track = track.with_album(album.clone());
                }
            }

            track = track.with_duration((duration_ms / 1000) as u32);

            if let Some(ref cover_pk) = cached_cover_pk {
                if let Ok(url) = channel.cache_manager.cover_url(cover_pk, None) {
                    track = track.with_image(url);
                }
            } else if let Some(ref cover_url) = resolved_cover_url {
                track = track.with_image(cover_url.clone());
            }

            let updated = channel
                .playlist
                .update_track(&track_id, |existing| {
                    existing.title = track.title.clone();
                    existing.artist = track.artist.clone();
                    existing.album = track.album.clone();
                    existing.duration = track.duration;
                    existing.uri = track.uri.clone();
                    existing.image = track.image.clone();
                })
                .await;

            if !updated {
                channel.playlist.append_track(track).await;
            }

            let channel_for_wait = channel.clone();
            let track_id_for_wait = track_id.clone();
            let audio_pk_for_wait = audio_pk.clone();
            tokio::spawn(async move {
                if let Err(e) = channel_for_wait
                    .cache_manager
                    .wait_audio_ready(&audio_pk_for_wait)
                    .await
                {
                    tracing::error!(
                        "Failed to finalize audio {} on channel {}: {}",
                        track_id_for_wait,
                        channel_for_wait.descriptor.name,
                        e
                    );
                    channel_for_wait
                        .cache_manager
                        .remove_track(&track_id_for_wait)
                        .await;
                    channel_for_wait
                        .playlist
                        .remove_by_id(&track_id_for_wait)
                        .await;
                }
            });
        }

        tracing::info!(
            "Channel {} now has {} tracks",
            channel.descriptor.name,
            channel.playlist.len().await
        );

        Ok(())
    }
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

    let cursor = Cursor::new(data);
    let mss = MediaSourceStream::new(Box::new(cursor), Default::default());

    let hint = Hint::new();
    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map_err(|e| anyhow!("Failed to probe format: {}", e))?;

    let mut format = probed.format;

    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        .ok_or_else(|| anyhow!("No audio track found"))?;

    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|e| anyhow!("Failed to create decoder: {}", e))?;

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
            Err(e) => return Err(anyhow!("Decode error: {}", e)),
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
            Err(e) => return Err(anyhow!("Decode error: {}", e)),
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
            .map_err(|e| anyhow!("FLAC config error: {:?}", e))?;

        let source = flacenc::source::MemSource::from_samples(
            &samples,
            channels,
            bits_per_sample as usize,
            sample_rate as usize,
        );

        let flac_stream = flacenc::encode_with_fixed_block_size(&config, source, config.block_size)
            .map_err(|e| anyhow!("FLAC encode error: {:?}", e))?;

        let mut sink = ByteSink::new();
        flac_stream
            .write(&mut sink)
            .map_err(|e| anyhow!("FLAC write error: {:?}", e))?;

        Ok::<_, anyhow::Error>(sink.into_inner())
    })
    .await?
}

#[async_trait]
impl MusicSource for RadioParadiseSource {
    fn name(&self) -> &str {
        "Radio Paradise"
    }

    fn id(&self) -> &str {
        "radio-paradise"
    }

    fn default_image(&self) -> &[u8] {
        DEFAULT_IMAGE
    }

    async fn root_container(&self) -> Result<Container> {
        Ok(self.build_root_container())
    }

    async fn browse(&self, object_id: &str) -> Result<BrowseResult> {
        match object_id {
            "0" => Ok(BrowseResult::Containers(vec![self.build_root_container()])),
            "radio-paradise" => {
                let containers = self.build_channel_containers().await;
                Ok(BrowseResult::Containers(containers))
            }
            _ => {
                if let Some(channel_id) = parse_channel_container_id(object_id) {
                    let channel = self
                        .channel_state(channel_id)
                        .ok_or_else(|| MusicSourceError::ObjectNotFound(object_id.to_string()))?;
                    self.ensure_channel_ready(channel.clone()).await?;
                    let len = channel.playlist.len().await;
                    let items = channel.playlist.as_objects(0, len, None).await;
                    Ok(BrowseResult::Items(items))
                } else {
                    Err(MusicSourceError::ObjectNotFound(object_id.to_string()))
                }
            }
        }
    }

    async fn resolve_uri(&self, object_id: &str) -> Result<String> {
        let (channel_id, _, _) = parse_track_identifier(object_id)
            .ok_or_else(|| MusicSourceError::ObjectNotFound(object_id.to_string()))?;
        let channel = self
            .channel_state(channel_id)
            .ok_or_else(|| MusicSourceError::ObjectNotFound(object_id.to_string()))?;
        channel.cache_manager.resolve_uri(object_id).await
    }

    fn supports_fifo(&self) -> bool {
        false
    }

    async fn append_track(&self, _track: Item) -> Result<()> {
        Err(MusicSourceError::FifoNotSupported)
    }

    async fn remove_oldest(&self) -> Result<Option<Item>> {
        Err(MusicSourceError::FifoNotSupported)
    }

    async fn update_id(&self) -> u32 {
        let mut max_id = 0;
        for descriptor in CHANNELS.iter() {
            if let Some(channel) = self.channel_state(descriptor.id) {
                let id = channel.playlist.update_id().await;
                max_id = max_id.max(id);
            }
        }
        max_id
    }

    async fn last_change(&self) -> Option<SystemTime> {
        let mut latest: Option<SystemTime> = None;
        for descriptor in CHANNELS.iter() {
            if let Some(channel) = self.channel_state(descriptor.id) {
                let change = channel.playlist.last_change().await;
                latest = Some(match latest {
                    Some(current) if change <= current => current,
                    _ => change,
                });
            }
        }
        latest
    }

    async fn get_items(&self, offset: usize, count: usize) -> Result<Vec<Item>> {
        let mut all_items = Vec::new();
        for descriptor in CHANNELS.iter() {
            if let Some(channel) = self.channel_state(descriptor.id) {
                self.ensure_channel_ready(channel.clone()).await?;
                let len = channel.playlist.len().await;
                let mut items = channel.playlist.as_objects(0, len, None).await;
                all_items.append(&mut items);
            }
        }

        let total = all_items.len();
        if offset >= total {
            return Ok(Vec::new());
        }

        let end = if count == 0 {
            total
        } else {
            (offset + count).min(total)
        };

        Ok(all_items
            .into_iter()
            .skip(offset)
            .take(end - offset)
            .collect())
    }

    async fn search(&self, _query: &str) -> Result<BrowseResult> {
        Err(MusicSourceError::SearchNotSupported)
    }

    fn capabilities(&self) -> pmosource::SourceCapabilities {
        pmosource::SourceCapabilities {
            supports_fifo: false,
            supports_search: false,
            supports_favorites: false,
            supports_playlists: false,
            supports_user_content: false,
            supports_high_res_audio: true,
            max_sample_rate: Some(96_000),
            supports_multiple_formats: true,
            supports_advanced_search: false,
            supports_pagination: true,
        }
    }

    async fn get_available_formats(&self, _object_id: &str) -> Result<Vec<pmosource::AudioFormat>> {
        use pmosource::AudioFormat;

        Ok(vec![
            AudioFormat {
                format_id: "mp3-128".to_string(),
                mime_type: "audio/mpeg".to_string(),
                sample_rate: Some(44100),
                bit_depth: None,
                bitrate: Some(128),
                channels: Some(2),
            },
            AudioFormat {
                format_id: "aac-64".to_string(),
                mime_type: "audio/aac".to_string(),
                sample_rate: Some(44100),
                bit_depth: None,
                bitrate: Some(64),
                channels: Some(2),
            },
            AudioFormat {
                format_id: "aac-128".to_string(),
                mime_type: "audio/aac".to_string(),
                sample_rate: Some(44100),
                bit_depth: None,
                bitrate: Some(128),
                channels: Some(2),
            },
            AudioFormat {
                format_id: "aac-320".to_string(),
                mime_type: "audio/aac".to_string(),
                sample_rate: Some(44100),
                bit_depth: None,
                bitrate: Some(320),
                channels: Some(2),
            },
            AudioFormat {
                format_id: "flac".to_string(),
                mime_type: "audio/flac".to_string(),
                sample_rate: Some(44100),
                bit_depth: Some(16),
                bitrate: None,
                channels: Some(2),
            },
        ])
    }

    async fn get_cache_status(&self, object_id: &str) -> Result<pmosource::CacheStatus> {
        let (channel_id, _, _) = parse_track_identifier(object_id)
            .ok_or_else(|| MusicSourceError::ObjectNotFound(object_id.to_string()))?;
        let channel = self
            .channel_state(channel_id)
            .ok_or_else(|| MusicSourceError::ObjectNotFound(object_id.to_string()))?;
        channel.cache_manager.get_cache_status(object_id).await
    }

    async fn cache_item(&self, object_id: &str) -> Result<pmosource::CacheStatus> {
        let (channel_id, _, _) = parse_track_identifier(object_id)
            .ok_or_else(|| MusicSourceError::ObjectNotFound(object_id.to_string()))?;
        let channel = self
            .channel_state(channel_id)
            .ok_or_else(|| MusicSourceError::ObjectNotFound(object_id.to_string()))?;
        self.ensure_channel_ready(channel.clone()).await?;
        channel.cache_manager.get_cache_status(object_id).await
    }

    async fn browse_paginated(
        &self,
        object_id: &str,
        offset: usize,
        limit: usize,
    ) -> Result<BrowseResult> {
        match object_id {
            "0" => {
                if offset == 0 {
                    Ok(BrowseResult::Containers(vec![self.build_root_container()]))
                } else {
                    Ok(BrowseResult::Containers(Vec::new()))
                }
            }
            "radio-paradise" => {
                let containers = self.build_channel_containers().await;
                let total = containers.len();
                if offset >= total {
                    return Ok(BrowseResult::Containers(Vec::new()));
                }
                let end = if limit == 0 {
                    total
                } else {
                    (offset + limit).min(total)
                };
                Ok(BrowseResult::Containers(
                    containers
                        .into_iter()
                        .skip(offset)
                        .take(end - offset)
                        .collect(),
                ))
            }
            _ => {
                if let Some(channel_id) = parse_channel_container_id(object_id) {
                    let channel = self
                        .channel_state(channel_id)
                        .ok_or_else(|| MusicSourceError::ObjectNotFound(object_id.to_string()))?;
                    self.ensure_channel_ready(channel.clone()).await?;
                    let len = channel.playlist.len().await;
                    if offset >= len {
                        return Ok(BrowseResult::Items(Vec::new()));
                    }
                    let count = if limit == 0 {
                        len - offset
                    } else {
                        limit.min(len - offset)
                    };
                    let items = channel.playlist.as_objects(offset, count, None).await;
                    Ok(BrowseResult::Items(items))
                } else {
                    Err(MusicSourceError::ObjectNotFound(object_id.to_string()))
                }
            }
        }
    }

    async fn get_item_count(&self, object_id: &str) -> Result<usize> {
        match object_id {
            "0" => Ok(1),
            "radio-paradise" => Ok(CHANNELS.len()),
            _ => {
                if let Some(channel_id) = parse_channel_container_id(object_id) {
                    let channel = self
                        .channel_state(channel_id)
                        .ok_or_else(|| MusicSourceError::ObjectNotFound(object_id.to_string()))?;
                    self.ensure_channel_ready(channel.clone()).await?;
                    Ok(channel.playlist.len().await)
                } else {
                    Err(MusicSourceError::ObjectNotFound(object_id.to_string()))
                }
            }
        }
    }

    async fn statistics(&self) -> Result<pmosource::SourceStatistics> {
        let mut total_items = 0usize;
        let mut cached_items = 0usize;

        for descriptor in CHANNELS.iter() {
            if let Some(channel) = self.channel_state(descriptor.id) {
                total_items += channel.playlist.len().await;
                let stats = channel.cache_manager.statistics().await;
                cached_items += stats.cached_tracks;
            }
        }

        Ok(pmosource::SourceStatistics {
            total_items: Some(total_items),
            total_containers: Some(CHANNELS.len() + 1),
            cached_items: Some(cached_items),
            cache_size_bytes: None,
        })
    }
}
