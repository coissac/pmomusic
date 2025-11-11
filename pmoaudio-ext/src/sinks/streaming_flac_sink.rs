//! Streaming FLAC sink for multi-track radio-style streaming over HTTP.
//!
//! This sink encodes incoming audio segments into a continuous FLAC stream,
//! broadcasts it to multiple concurrent clients (UPnP renderers, web players, etc.),
//! and supports ICY metadata for "Now Playing" updates.
//!
//! # Architecture
//!
//! ```text
//! AudioSegment Pipeline
//!        ↓
//! StreamingFlacSink
//!        ↓
//! [Convert AudioChunk → PCM bytes]
//!        ↓
//! ByteStreamReader (AsyncRead)
//!        ↓
//! pmoflac::encode_flac_stream()
//!        ↓
//! [Broadcaster Task]
//!        ↓
//! broadcast::channel<Bytes> (FLAC bytes)
//!        ↓
//! Multiple clients via StreamHandle::subscribe()
//!   ├─ FLAC pure (for standard renderers)
//!   └─ ICY-wrapped FLAC (for metadata-aware clients)
//! ```
//!
//! # Usage Example
//!
//! ```no_run
//! use pmoaudio_ext::sinks::StreamingFlacSink;
//! use pmoflac::EncoderOptions;
//!
//! // Create the sink and get the handle for HTTP serving
//! let (sink, handle) = StreamingFlacSink::new(
//!     EncoderOptions::default(),
//!     16, // bits per sample
//! );
//!
//! // Add to audio pipeline
//! source.register(Box::new(sink));
//!
//! // In your HTTP handler (e.g., pmoparadise):
//! if headers.get("Icy-MetaData") == Some("1") {
//!     // ICY mode with metadata updates
//!     let stream = handle.subscribe_icy();
//!     response.header("icy-metaint", "16000");
//!     Body::from_stream(ReaderStream::new(stream))
//! } else {
//!     // Pure FLAC mode
//!     let stream = handle.subscribe_flac();
//!     Body::from_stream(ReaderStream::new(stream))
//! }
//! ```

use std::collections::VecDeque;
use std::io;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use async_trait::async_trait;
use bytes::Bytes;
use pmoaudio::{
    pipeline::{AudioPipelineNode, Node, NodeLogic, PipelineHandle, StopReason},
    AudioChunk, AudioError, AudioSegment, SyncMarker, TypeRequirement, TypedAudioNode,
    _AudioSegment,
};
use pmoflac::{encode_flac_stream, EncoderOptions, FlacEncodedStream, PcmFormat};
use pmometadata::TrackMetadata;
use tokio::io::{AsyncRead, AsyncReadExt, ReadBuf};
use tokio::sync::{broadcast, mpsc, RwLock};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, trace, warn};

/// Default ICY metadata interval (bytes of audio between metadata blocks).
/// Standard value used by most streaming servers.
const DEFAULT_ICY_METAINT: usize = 16000;

/// Broadcast channel capacity for FLAC bytes.
const BROADCAST_CAPACITY: usize = 512;

/// Snapshot of track metadata at a point in time.
///
/// This structure is shared between the sink and clients to provide
/// real-time metadata updates as tracks change in a continuous stream.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct MetadataSnapshot {
    /// Track title
    pub title: Option<String>,
    /// Artist name
    pub artist: Option<String>,
    /// Album name
    pub album: Option<String>,
    /// Track duration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration: Option<Duration>,
    /// Cover image URL (external/original)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cover_url: Option<String>,
    /// Cover primary key in local cache (for constructing server URL)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cover_pk: Option<String>,
    /// Track number
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track_number: Option<u32>,
    /// Album artist
    #[serde(skip_serializing_if = "Option::is_none")]
    pub album_artist: Option<String>,
    /// Genre
    #[serde(skip_serializing_if = "Option::is_none")]
    pub genre: Option<String>,
    /// Year
    #[serde(skip_serializing_if = "Option::is_none")]
    pub year: Option<u32>,
    /// Audio timestamp where this metadata became active (seconds)
    pub audio_timestamp_sec: f64,
    /// Version counter incremented on each update (for client-side change detection)
    pub version: u64,
}

/// Handle for accessing the FLAC stream and metadata from HTTP handlers.
///
/// This handle is designed to be cloned and used by multiple HTTP clients
/// simultaneously. Each client gets its own independent stream by subscribing.
#[derive(Clone)]
pub struct StreamHandle {
    /// Broadcast sender for FLAC bytes (pure mode)
    flac_broadcast: broadcast::Sender<Bytes>,

    /// Current track metadata (read-only for consumers)
    metadata: Arc<RwLock<MetadataSnapshot>>,

    /// Active client counter
    active_clients: Arc<AtomicUsize>,

    /// Stop token to signal pipeline shutdown
    stop_token: CancellationToken,

    /// Cached FLAC header (sent to new subscribers first)
    flac_header: Arc<RwLock<Option<Bytes>>>,
}

impl StreamHandle {
    /// Subscribe to the FLAC stream in pure mode (no ICY metadata).
    ///
    /// Returns an `AsyncRead` stream suitable for use with `tokio_util::io::ReaderStream`.
    pub fn subscribe_flac(&self) -> FlacClientStream {
        let count = self.active_clients.fetch_add(1, Ordering::SeqCst);
        debug!("New FLAC client subscribed (total: {})", count + 1);

        FlacClientStream {
            rx: self.flac_broadcast.subscribe(),
            buffer: VecDeque::new(),
            finished: false,
            handle: self.clone(),
            state: FlacStreamState::SendingHeader,
        }
    }

    /// Subscribe to the FLAC stream with ICY metadata injection.
    ///
    /// Returns an `AsyncRead` stream that injects ICY metadata blocks
    /// at regular intervals (default: every 16000 bytes).
    pub fn subscribe_icy(&self) -> IcyClientStream {
        self.subscribe_icy_with_interval(DEFAULT_ICY_METAINT)
    }

    /// Subscribe to the FLAC stream with custom ICY metadata interval.
    pub fn subscribe_icy_with_interval(&self, metaint: usize) -> IcyClientStream {
        let count = self.active_clients.fetch_add(1, Ordering::SeqCst);
        debug!("New ICY client subscribed (total: {}, metaint: {})", count + 1, metaint);

        IcyClientStream {
            rx: self.flac_broadcast.subscribe(),
            metadata: self.metadata.clone(),
            metaint,
            byte_count: 0,
            buffer: VecDeque::new(),
            current_metadata_version: 0,
            cached_icy_metadata: Bytes::new(),
            finished: false,
            handle: self.clone(),
            state: FlacStreamState::SendingHeader,
        }
    }

    /// Get the current metadata snapshot.
    pub async fn get_metadata(&self) -> MetadataSnapshot {
        self.metadata.read().await.clone()
    }

    /// Get the number of active clients.
    pub fn active_client_count(&self) -> usize {
        self.active_clients.load(Ordering::SeqCst)
    }

    /// Check if the stream should be stopped (no more clients).
    pub fn should_stop(&self) -> bool {
        self.active_clients.load(Ordering::SeqCst) == 0
    }
}

/// State for FLAC stream subscription.
enum FlacStreamState {
    SendingHeader,
    Streaming,
}

/// Pure FLAC client stream (implements AsyncRead).
pub struct FlacClientStream {
    rx: broadcast::Receiver<Bytes>,
    buffer: VecDeque<u8>,
    finished: bool,
    handle: StreamHandle,
    state: FlacStreamState,
}

impl AsyncRead for FlacClientStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        loop {
            // If in header state, send the header first
            if matches!(self.state, FlacStreamState::SendingHeader) {
                if let Ok(guard) = self.handle.flac_header.try_read() {
                    if let Some(header) = guard.as_ref() {
                        self.buffer.extend(header.iter());
                        info!("Sending cached FLAC header to new client ({} bytes)", header.len());
                        self.state = FlacStreamState::Streaming;
                        continue; // Now copy header to output buffer
                    } else {
                        // Header not yet captured, skip to streaming
                        self.state = FlacStreamState::Streaming;
                    }
                } else {
                    // Can't acquire lock, skip to streaming
                    self.state = FlacStreamState::Streaming;
                }
            }

            // If we have buffered data, copy it
            if !self.buffer.is_empty() {
                let to_copy = self.buffer.len().min(buf.remaining());
                if to_copy == 0 {
                    return Poll::Ready(Ok(()));
                }

                let slice = self.buffer.make_contiguous();
                buf.put_slice(&slice[..to_copy]);
                self.buffer.drain(..to_copy);
                return Poll::Ready(Ok(()));
            }

            if self.finished {
                return Poll::Ready(Ok(()));
            }

            // Try to receive more data
            match self.rx.try_recv() {
                Ok(bytes) => {
                    self.buffer.extend(bytes.iter());
                }
                Err(broadcast::error::TryRecvError::Empty) => {
                    // No data available, register waker and return pending
                    cx.waker().wake_by_ref();
                    return Poll::Pending;
                }
                Err(broadcast::error::TryRecvError::Lagged(skipped)) => {
                    warn!("FLAC client lagged, skipped {} messages", skipped);
                    // Continue to try receiving again
                }
                Err(broadcast::error::TryRecvError::Closed) => {
                    self.finished = true;
                    return Poll::Ready(Ok(()));
                }
            }
        }
    }
}

impl Drop for FlacClientStream {
    fn drop(&mut self) {
        let count = self.handle.active_clients.fetch_sub(1, Ordering::SeqCst);
        debug!("FLAC client disconnected (remaining: {})", count - 1);

        if count == 1 {
            info!("Last client disconnected, signaling pipeline stop");
            self.handle.stop_token.cancel();
        }
    }
}

/// ICY-wrapped FLAC client stream (implements AsyncRead).
///
/// This stream injects ICY metadata blocks at regular intervals,
/// allowing clients to display "Now Playing" information.
pub struct IcyClientStream {
    rx: broadcast::Receiver<Bytes>,
    metadata: Arc<RwLock<MetadataSnapshot>>,
    metaint: usize,
    byte_count: usize,
    buffer: VecDeque<u8>,
    current_metadata_version: u64,
    cached_icy_metadata: Bytes,
    finished: bool,
    handle: StreamHandle,
    state: FlacStreamState,
}

impl IcyClientStream {
    /// Format metadata as ICY metadata block.
    ///
    /// ICY format: StreamTitle='Artist - Title';StreamUrl='url';
    /// Padded to multiple of 16 bytes, prefixed with length byte.
    ///
    /// If cover_pk is available, constructs a URL for the cover image:
    /// - If pmoserver is initialized: http://server/covers/image/{pk}/256
    /// - Otherwise: relative URL /covers/image/{pk}/256
    fn format_icy_metadata(meta: &MetadataSnapshot) -> Bytes {
        let title = meta.title.as_deref().unwrap_or("Unknown");
        let artist = meta.artist.as_deref().unwrap_or("Unknown Artist");

        // Build ICY metadata string with cover URL if available
        let mut metadata_str = format!("StreamTitle='{} - {}';", artist, title);

        // Add cover URL if we have a cover_pk
        if let Some(pk) = &meta.cover_pk {
            // Use relative URL /covers/image/{pk}/256
            // This works when streaming from the same server that serves covers
            // VLC and other players will resolve relative URLs correctly
            metadata_str.push_str(&format!("StreamUrl='/covers/image/{}/256';", pk));
        } else if let Some(url) = &meta.cover_url {
            // Fallback to external cover URL if no local pk
            metadata_str.push_str(&format!("StreamUrl='{}';", url));
        }

        // ICY metadata is padded to multiple of 16 bytes
        let metadata_bytes = metadata_str.as_bytes();
        let length = metadata_bytes.len();
        let padded_length = ((length + 15) / 16) * 16;
        let length_byte = (padded_length / 16) as u8;

        let mut result = Vec::with_capacity(1 + padded_length);
        result.push(length_byte);
        result.extend_from_slice(metadata_bytes);
        result.resize(1 + padded_length, 0); // Pad with zeros

        Bytes::from(result)
    }

    /// Get metadata block if it needs to be inserted.
    async fn get_metadata_if_changed(&mut self) -> Option<Bytes> {
        let meta = self.metadata.read().await;
        if meta.version > self.current_metadata_version {
            self.current_metadata_version = meta.version;
            let icy_meta = Self::format_icy_metadata(&meta);
            self.cached_icy_metadata = icy_meta.clone();
            Some(icy_meta)
        } else if self.byte_count == 0 {
            // Always send metadata at the start
            Some(self.cached_icy_metadata.clone())
        } else {
            // No change, send empty metadata block
            Some(Bytes::from(vec![0u8]))
        }
    }
}

impl AsyncRead for IcyClientStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        loop {
            // If in header state, send the header first
            if matches!(self.state, FlacStreamState::SendingHeader) {
                if let Ok(guard) = self.handle.flac_header.try_read() {
                    if let Some(header) = guard.as_ref() {
                        self.buffer.extend(header.iter());
                        info!("Sending cached FLAC header to new ICY client ({} bytes)", header.len());
                        self.state = FlacStreamState::Streaming;
                        continue; // Now copy header to output buffer
                    } else {
                        // Header not yet captured, skip to streaming
                        self.state = FlacStreamState::Streaming;
                    }
                } else {
                    // Can't acquire lock, skip to streaming
                    self.state = FlacStreamState::Streaming;
                }
            }

            // If we have buffered data, copy it
            if !self.buffer.is_empty() {
                let to_copy = self.buffer.len().min(buf.remaining());
                if to_copy == 0 {
                    return Poll::Ready(Ok(()));
                }

                let slice = self.buffer.make_contiguous();
                buf.put_slice(&slice[..to_copy]);
                self.buffer.drain(..to_copy);
                return Poll::Ready(Ok(()));
            }

            if self.finished {
                return Poll::Ready(Ok(()));
            }

            // Check if we need to insert metadata
            if self.byte_count % self.metaint == 0 && self.byte_count > 0 {
                // Time to insert ICY metadata
                // Use try_read to avoid blocking in poll context
                let update = {
                    if let Ok(meta) = self.metadata.try_read() {
                        if meta.version > self.current_metadata_version {
                            Some((meta.version, Self::format_icy_metadata(&meta)))
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                };

                if let Some((new_version, new_metadata)) = update {
                    self.current_metadata_version = new_version;
                    self.cached_icy_metadata = new_metadata;
                }

                let icy_data = self.cached_icy_metadata.clone();
                self.buffer.extend(icy_data.iter());
                self.byte_count = 0; // Reset counter after metadata
                continue;
            }

            // Try to receive audio data
            match self.rx.try_recv() {
                Ok(bytes) => {
                    // Calculate how many bytes until next metadata block
                    let until_metadata = self.metaint - (self.byte_count % self.metaint);
                    let to_buffer = bytes.len().min(until_metadata);

                    self.buffer.extend(bytes[..to_buffer].iter());
                    self.byte_count += to_buffer;

                    // If we have more data, we'll process it in the next iteration
                    if to_buffer < bytes.len() {
                        // Save remaining for next iteration
                        // For now, we'll just drop it and get it again
                        // TODO: Improve this
                    }
                }
                Err(broadcast::error::TryRecvError::Empty) => {
                    cx.waker().wake_by_ref();
                    return Poll::Pending;
                }
                Err(broadcast::error::TryRecvError::Lagged(skipped)) => {
                    warn!("ICY client lagged, skipped {} messages", skipped);
                }
                Err(broadcast::error::TryRecvError::Closed) => {
                    self.finished = true;
                    return Poll::Ready(Ok(()));
                }
            }
        }
    }
}

impl Drop for IcyClientStream {
    fn drop(&mut self) {
        let count = self.handle.active_clients.fetch_sub(1, Ordering::SeqCst);
        debug!("ICY client disconnected (remaining: {})", count - 1);

        if count == 1 {
            info!("Last client disconnected, signaling pipeline stop");
            self.handle.stop_token.cancel();
        }
    }
}

/// Internal state for encoder initialization.
struct EncoderState {
    broadcaster_task: tokio::task::JoinHandle<()>,
}

/// Logic for the streaming FLAC sink.
struct StreamingFlacSinkLogic {
    encoder_options: EncoderOptions,
    bits_per_sample: u8,
    pcm_tx: mpsc::Sender<Vec<u8>>,
    pcm_rx: Option<mpsc::Receiver<Vec<u8>>>,
    metadata: Arc<RwLock<MetadataSnapshot>>,
    flac_broadcast: broadcast::Sender<Bytes>,
    flac_header: Arc<RwLock<Option<Bytes>>>,
    encoder_state: Option<EncoderState>,
    sample_rate: Option<u32>,
}

impl StreamingFlacSinkLogic {
    /// Initialize the FLAC encoder once we know the sample rate.
    async fn initialize_encoder(&mut self, sample_rate: u32) -> Result<(), AudioError> {
        if self.encoder_state.is_some() {
            return Ok(()); // Already initialized
        }

        info!("Initializing FLAC encoder with sample rate: {} Hz", sample_rate);

        // Take the PCM receiver (we only initialize once)
        let pcm_rx = self.pcm_rx.take().ok_or_else(|| {
            AudioError::ProcessingError("PCM receiver already consumed".into())
        })?;

        // Create ByteStreamReader for the encoder
        let pcm_reader = ByteStreamReader::new(pcm_rx);

        // Create PCM format
        let pcm_format = PcmFormat {
            sample_rate,
            channels: 2,
            bits_per_sample: self.bits_per_sample,
        };

        // Start the FLAC encoder
        let flac_stream = encode_flac_stream(pcm_reader, pcm_format, self.encoder_options.clone())
            .await
            .map_err(|e| AudioError::ProcessingError(format!("Failed to start FLAC encoder: {}", e)))?;

        info!("FLAC encoder initialized successfully");

        // Spawn broadcaster task
        let flac_broadcast = self.flac_broadcast.clone();
        let flac_header = self.flac_header.clone();
        let broadcaster_task = tokio::spawn(async move {
            if let Err(e) = broadcast_flac_stream(flac_stream, flac_broadcast, flac_header).await {
                error!("Broadcaster task error: {}", e);
            }
        });

        self.encoder_state = Some(EncoderState { broadcaster_task });

        info!("Broadcaster task spawned");

        Ok(())
    }

    /// Update metadata from a TrackBoundary marker.
    async fn update_metadata(
        &mut self,
        metadata_lock: &Arc<RwLock<dyn TrackMetadata>>,
        timestamp_sec: f64,
    ) -> Result<(), AudioError> {
        let metadata = metadata_lock.read().await;

        let mut snapshot = self.metadata.write().await;

        // Extract all metadata fields
        snapshot.title = metadata.get_title().await.ok().flatten();
        snapshot.artist = metadata.get_artist().await.ok().flatten();
        snapshot.album = metadata.get_album().await.ok().flatten();
        snapshot.duration = metadata.get_duration().await.ok().flatten();
        snapshot.cover_url = metadata.get_cover_url().await.ok().flatten();
        snapshot.cover_pk = metadata.get_cover_pk().await.ok().flatten();
        snapshot.year = metadata.get_year().await.ok().flatten();

        // Extract extra fields
        if let Ok(Some(extra)) = metadata.get_extra().await {
            snapshot.genre = extra.get("genre").cloned();
            snapshot.track_number = extra
                .get("track_number")
                .and_then(|s| s.parse::<u32>().ok());
        }

        snapshot.audio_timestamp_sec = timestamp_sec;
        snapshot.version += 1;

        debug!(
            "Metadata updated: v{} @ {:.2}s - {} - {} (cover_pk: {:?})",
            snapshot.version,
            timestamp_sec,
            snapshot.artist.as_deref().unwrap_or("?"),
            snapshot.title.as_deref().unwrap_or("?"),
            snapshot.cover_pk
        );

        Ok(())
    }
}

#[async_trait]
impl NodeLogic for StreamingFlacSinkLogic {
    async fn process(
        &mut self,
        input: Option<mpsc::Receiver<Arc<AudioSegment>>>,
        _output: Vec<mpsc::Sender<Arc<AudioSegment>>>,
        stop_token: CancellationToken,
    ) -> Result<(), AudioError> {
        let mut input = input.ok_or_else(|| {
            AudioError::ProcessingError("StreamingFlacSink requires an input".into())
        })?;

        info!("StreamingFlacSink started");

        // We'll initialize the encoder lazily when we get the first chunk
        // For now, just process segments

        loop {
            tokio::select! {
                _ = stop_token.cancelled() => {
                    info!("StreamingFlacSink stopped by cancellation");
                    break;
                }

                segment = input.recv() => {
                    match segment {
                        Some(seg) => {
                            match &seg.segment {
                                _AudioSegment::Chunk(chunk) => {
                                    // Detect sample rate from first chunk and initialize encoder
                                    if self.sample_rate.is_none() {
                                        let sample_rate = chunk.sample_rate();
                                        self.sample_rate = Some(sample_rate);
                                        info!("Detected sample rate: {} Hz", sample_rate);

                                        // Initialize the FLAC encoder now
                                        self.initialize_encoder(sample_rate).await?;
                                    }

                                    // Convert chunk to PCM bytes
                                    let pcm_bytes = chunk_to_pcm_bytes(&chunk, self.bits_per_sample)?;

                                    trace!(
                                        "Sending PCM chunk: {} bytes, {} samples @ {:.2}s",
                                        pcm_bytes.len(),
                                        chunk.len(),
                                        seg.timestamp_sec
                                    );

                                    // Send to FLAC encoder
                                    if let Err(e) = self.pcm_tx.send(pcm_bytes).await {
                                        warn!("Failed to send PCM data to encoder: {}", e);
                                        break;
                                    }
                                }

                                _AudioSegment::Sync(marker) => {
                                    match marker.as_ref() {
                                        SyncMarker::TrackBoundary { metadata } => {
                                            if let Err(e) = self.update_metadata(metadata, seg.timestamp_sec).await {
                                                error!("Failed to update metadata: {}", e);
                                            }
                                        }

                                        SyncMarker::EndOfStream => {
                                            info!("End of stream marker received");
                                            break;
                                        }

                                        _ => {
                                            trace!("Received other sync marker");
                                        }
                                    }
                                }
                            }
                        }

                        None => {
                            info!("Input channel closed");
                            break;
                        }
                    }
                }
            }
        }

        info!("StreamingFlacSink processing complete");
        Ok(())
    }

    async fn cleanup(&mut self, reason: StopReason) -> Result<(), AudioError> {
        info!("StreamingFlacSink cleanup: {:?}", reason);
        Ok(())
    }
}

/// Broadcaster task: reads FLAC bytes from encoder and broadcasts to all clients.
async fn broadcast_flac_stream(
    mut flac_stream: FlacEncodedStream,
    broadcast_tx: broadcast::Sender<Bytes>,
    header_cache: Arc<RwLock<Option<Bytes>>>,
) -> Result<(), AudioError> {
    info!("Broadcaster task started");

    let mut buffer = vec![0u8; 8192]; // 8KB buffer for reading
    let mut total_bytes = 0u64;
    let mut header_captured = false;

    loop {
        match flac_stream.read(&mut buffer).await {
            Ok(0) => {
                // EOF
                info!("FLAC encoder stream ended, total bytes: {}", total_bytes);
                break;
            }
            Ok(n) => {
                total_bytes += n as u64;
                trace!("Read {} bytes from FLAC encoder (total: {})", n, total_bytes);

                // Broadcast to all clients
                let bytes = Bytes::copy_from_slice(&buffer[..n]);

                // Capture first chunk as header if it contains "fLaC"
                if !header_captured && bytes.len() >= 4 && &bytes[0..4] == b"fLaC" {
                    *header_cache.write().await = Some(bytes.clone());
                    header_captured = true;
                    info!("FLAC header captured ({} bytes)", bytes.len());
                }

                if let Err(e) = broadcast_tx.send(bytes) {
                    // No receivers, but that's okay - clients may not be connected yet
                    trace!("No active receivers for FLAC broadcast: {}", e);
                }
            }
            Err(e) => {
                error!("Error reading from FLAC encoder: {}", e);
                return Err(AudioError::ProcessingError(format!(
                    "FLAC encoder read error: {}",
                    e
                )));
            }
        }
    }

    // Wait for the encoder to finish cleanly
    if let Err(e) = flac_stream.wait().await {
        error!("FLAC encoder error during cleanup: {}", e);
        return Err(AudioError::ProcessingError(format!(
            "FLAC encoder error: {}",
            e
        )));
    }

    info!("Broadcaster task completed successfully");
    Ok(())
}

/// Streaming FLAC sink for multi-client HTTP streaming.
pub struct StreamingFlacSink {
    inner: Node<StreamingFlacSinkLogic>,
}

impl StreamingFlacSink {
    /// Create a new streaming FLAC sink.
    ///
    /// # Arguments
    ///
    /// * `encoder_options` - FLAC encoder configuration
    /// * `bits_per_sample` - Target bit depth (16, 24, or 32)
    ///
    /// # Returns
    ///
    /// A tuple of `(sink, handle)` where:
    /// - `sink` is added to the audio pipeline
    /// - `handle` is used by HTTP handlers to serve streams
    pub fn new(
        encoder_options: EncoderOptions,
        bits_per_sample: u8,
    ) -> (Self, StreamHandle) {
        // Validate bit depth
        if ![16, 24, 32].contains(&bits_per_sample) {
            panic!("bits_per_sample must be 16, 24, or 32");
        }

        // Create PCM channel (bounded for backpressure)
        let (pcm_tx, pcm_rx) = mpsc::channel::<Vec<u8>>(16);

        // Shared metadata
        let metadata = Arc::new(RwLock::new(MetadataSnapshot::default()));

        // Broadcast channel for FLAC bytes
        let (flac_broadcast, _) = broadcast::channel(BROADCAST_CAPACITY);

        // FLAC header cache
        let flac_header = Arc::new(RwLock::new(None));

        // Stop token and client counter
        let stop_token = CancellationToken::new();
        let active_clients = Arc::new(AtomicUsize::new(0));

        let handle = StreamHandle {
            flac_broadcast: flac_broadcast.clone(),
            metadata: metadata.clone(),
            active_clients,
            stop_token: stop_token.clone(),
            flac_header: flac_header.clone(),
        };

        let logic = StreamingFlacSinkLogic {
            encoder_options,
            bits_per_sample,
            pcm_tx,
            pcm_rx: Some(pcm_rx),
            metadata,
            flac_broadcast,
            flac_header,
            encoder_state: None,
            sample_rate: None,
        };

        let sink = Self {
            inner: Node::new_with_input(logic, 16),
        };

        (sink, handle)
    }
}

#[async_trait]
impl AudioPipelineNode for StreamingFlacSink {
    fn get_tx(&self) -> Option<mpsc::Sender<Arc<AudioSegment>>> {
        self.inner.get_tx()
    }

    fn register(&mut self, _child: Box<dyn AudioPipelineNode>) {
        panic!("StreamingFlacSink is a terminal sink and cannot have children");
    }

    async fn run(self: Box<Self>, stop_token: CancellationToken) -> Result<(), AudioError> {
        Box::new(self.inner).run(stop_token).await
    }

    fn start(self: Box<Self>) -> PipelineHandle {
        Box::new(self.inner).start()
    }
}

impl TypedAudioNode for StreamingFlacSink {
    fn input_type(&self) -> Option<TypeRequirement> {
        Some(TypeRequirement::any_integer())
    }

    fn output_type(&self) -> Option<TypeRequirement> {
        None
    }
}

/// Convert an AudioChunk to PCM bytes with specified bit depth.
fn chunk_to_pcm_bytes(chunk: &AudioChunk, bits_per_sample: u8) -> Result<Vec<u8>, AudioError> {
    match chunk {
        AudioChunk::F32(_) | AudioChunk::F64(_) => {
            return Err(AudioError::ProcessingError(
                "StreamingFlacSink only supports integer audio chunks".into(),
            ));
        }
        _ => {}
    }

    let len = chunk.len();
    let bytes_per_frame = (bits_per_sample / 8) as usize * 2;
    let mut bytes = Vec::with_capacity(len * bytes_per_frame);

    match (chunk, bits_per_sample) {
        (AudioChunk::I16(data), 16) => {
            for frame in data.get_frames() {
                bytes.extend_from_slice(&frame[0].to_le_bytes());
                bytes.extend_from_slice(&frame[1].to_le_bytes());
            }
        }
        (AudioChunk::I16(data), 24) => {
            for frame in data.get_frames() {
                let left = (frame[0] as i32) << 8;
                let right = (frame[1] as i32) << 8;
                bytes.extend_from_slice(&left.to_le_bytes()[..3]);
                bytes.extend_from_slice(&right.to_le_bytes()[..3]);
            }
        }
        (AudioChunk::I16(data), 32) => {
            for frame in data.get_frames() {
                let left = (frame[0] as i32) << 16;
                let right = (frame[1] as i32) << 16;
                bytes.extend_from_slice(&left.to_le_bytes());
                bytes.extend_from_slice(&right.to_le_bytes());
            }
        }
        (AudioChunk::I24(data), 16) => {
            for frame in data.get_frames() {
                let left = (frame[0].as_i32() >> 8) as i16;
                let right = (frame[1].as_i32() >> 8) as i16;
                bytes.extend_from_slice(&left.to_le_bytes());
                bytes.extend_from_slice(&right.to_le_bytes());
            }
        }
        (AudioChunk::I24(data), 24) => {
            for frame in data.get_frames() {
                bytes.extend_from_slice(&frame[0].as_i32().to_le_bytes()[..3]);
                bytes.extend_from_slice(&frame[1].as_i32().to_le_bytes()[..3]);
            }
        }
        (AudioChunk::I24(data), 32) => {
            for frame in data.get_frames() {
                let left = frame[0].as_i32() << 8;
                let right = frame[1].as_i32() << 8;
                bytes.extend_from_slice(&left.to_le_bytes());
                bytes.extend_from_slice(&right.to_le_bytes());
            }
        }
        (AudioChunk::I32(data), 16) => {
            for frame in data.get_frames() {
                let left = (frame[0] >> 16) as i16;
                let right = (frame[1] >> 16) as i16;
                bytes.extend_from_slice(&left.to_le_bytes());
                bytes.extend_from_slice(&right.to_le_bytes());
            }
        }
        (AudioChunk::I32(data), 24) => {
            for frame in data.get_frames() {
                let left = frame[0] >> 8;
                let right = frame[1] >> 8;
                bytes.extend_from_slice(&left.to_le_bytes()[..3]);
                bytes.extend_from_slice(&right.to_le_bytes()[..3]);
            }
        }
        (AudioChunk::I32(data), 32) => {
            for frame in data.get_frames() {
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

/// AsyncRead adapter for mpsc::Receiver<Vec<u8>>.
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
