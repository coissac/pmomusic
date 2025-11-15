//! Streaming OGG-FLAC sink for multi-track radio-style streaming over HTTP.
//!
//! This sink encodes incoming audio segments into OGG-FLAC format with proper
//! OGG chaining for track boundaries. Unlike pure FLAC, OGG-FLAC supports
//! embedded metadata via Vorbis Comments that update with each track.
//!
//! # Architecture
//!
//! ```text
//! AudioSegment Pipeline
//!        ↓
//! StreamingOggFlacSink
//!        ↓
//! [TrackBoundary detection]
//!        ↓
//! [Convert AudioChunk → PCM bytes]
//!        ↓
//! ByteStreamReader (AsyncRead)
//!        ↓
//! pmoflac::encode_flac_stream()
//!        ↓
//! [OGG Wrapper Task] - wraps FLAC frames in OGG pages
//!        ↓
//! timed_broadcast::channel<Bytes> (OGG-FLAC bytes)
//!        ↓
//! Multiple HTTP clients
//! ```
//!
//! # OGG Chaining
//!
//! When a `TrackBoundary` marker is received:
//! 1. Flush current FLAC encoder
//! 2. Write OGG page with EOS flag (End of Stream)
//! 3. Extract metadata from TrackBoundary
//! 4. Start new logical bitstream with BOS flag (Beginning of Stream)
//! 5. Write new OGG-FLAC headers with updated Vorbis Comments
//! 6. Continue encoding
//!
//! This allows seamless track changes with metadata updates.
//!
//! # 100% Streaming Guarantee
//!
//! - No track buffering: AudioChunks are converted to PCM immediately
//! - FLAC encoder produces frames as soon as it has enough samples
//! - OGG wrapper reads FLAC frames and creates pages on-the-fly
//! - Pages are broadcast immediately to connected clients
//! - TrackBoundary only triggers encoder flush (no data accumulation)

use std::collections::VecDeque;
use std::io;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll};

use super::{
    broadcast_pacing::BroadcastPacer,
    flac_frame_utils,
    timed_broadcast::{self, TimedPacket, TryRecvError},
};
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
use tokio::sync::{mpsc, RwLock};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, trace, warn};

/// Default maximum lead time for HTTP broadcast pacing (in seconds).
const DEFAULT_BROADCAST_MAX_LEAD_TIME: f64 = 0.0;

/// Calculate broadcast channel capacity based on max lead time.
///
/// Estimate: ~20 OGG pages per second (assuming 50ms chunks).
/// Minimum capacity: 100 items for buffering even with 0 lead time.
fn calculate_broadcast_capacity(max_lead_time: f64) -> usize {
    let estimated_items_per_second = 20.0;
    let capacity = (max_lead_time * estimated_items_per_second) as usize;
    capacity.max(100) // Minimum 100 items
}

/// PCM chunk with audio data and timestamp for precise pacing.
#[derive(Debug)]
struct PcmChunk {
    /// Raw PCM audio bytes
    bytes: Vec<u8>,
    /// Timestamp in seconds (from AudioSegment)
    timestamp_sec: f64,
}

/// Snapshot of track metadata (reuse from streaming_flac_sink)
pub use super::streaming_flac_sink::MetadataSnapshot;

/// Handle for accessing the OGG-FLAC stream and metadata from HTTP handlers.
#[derive(Clone)]
pub struct OggFlacStreamHandle {
    /// Broadcast sender for OGG-FLAC bytes
    ogg_broadcast: timed_broadcast::Sender<Bytes>,

    /// Current track metadata
    metadata: Arc<RwLock<MetadataSnapshot>>,

    /// Active client counter
    active_clients: Arc<AtomicUsize>,

    /// Stop token to signal pipeline shutdown
    stop_token: CancellationToken,

    /// Cached OGG-FLAC header (sent to new subscribers first)
    ogg_header: Arc<RwLock<Option<Bytes>>>,
}

impl OggFlacStreamHandle {
    /// Subscribe to the OGG-FLAC stream.
    ///
    /// Returns an `AsyncRead` stream suitable for use with `tokio_util::io::ReaderStream`.
    pub fn subscribe(&self) -> OggFlacClientStream {
        let count = self.active_clients.fetch_add(1, Ordering::SeqCst);
        debug!("New OGG-FLAC client subscribed (total: {})", count + 1);

        OggFlacClientStream {
            rx: self.ogg_broadcast.subscribe(),
            buffer: VecDeque::new(),
            finished: false,
            handle: self.clone(),
            state: OggFlacStreamState::SendingHeader,
            current_epoch: 0,
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
}

/// State for OGG-FLAC stream subscription.
enum OggFlacStreamState {
    SendingHeader,
    Streaming,
}

/// OGG-FLAC client stream (implements AsyncRead).
pub struct OggFlacClientStream {
    rx: timed_broadcast::Receiver<Bytes>,
    buffer: VecDeque<u8>,
    finished: bool,
    handle: OggFlacStreamHandle,
    state: OggFlacStreamState,
    current_epoch: u64,
}

impl OggFlacClientStream {
    /// Dernier epoch observé (incrémenté à chaque TopZeroSync).
    pub fn current_epoch(&self) -> u64 {
        self.current_epoch
    }
}

impl AsyncRead for OggFlacClientStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        loop {
            // If in header state, send the header first
            if matches!(self.state, OggFlacStreamState::SendingHeader) {
                let header_opt = if let Ok(guard) = self.handle.ogg_header.try_read() {
                    guard.clone()
                } else {
                    None
                };

                if let Some(header) = header_opt {
                    self.buffer.extend(header.iter());
                    info!(
                        "Sending cached OGG-FLAC header to new client ({} bytes)",
                        header.len()
                    );
                    self.state = OggFlacStreamState::Streaming;
                    continue; // Now copy header to output buffer
                } else {
                    // Header not yet captured, skip to streaming
                    self.state = OggFlacStreamState::Streaming;
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
                Ok(packet) => {
                    self.current_epoch = packet.epoch;
                    self.buffer.extend(packet.payload.iter());
                }
                Err(TryRecvError::Empty) => {
                    // No data available right now.
                    // Schedule a wakeup after a small delay to avoid busy-loop polling.
                    let waker = cx.waker().clone();
                    tokio::spawn(async move {
                        tokio::time::sleep(tokio::time::Duration::from_micros(100)).await;
                        waker.wake();
                    });
                    return Poll::Pending;
                }
                Err(TryRecvError::Lagged(skipped)) => {
                    warn!("OGG-FLAC client lagged, skipped {} messages", skipped);
                }
                Err(TryRecvError::Closed) => {
                    self.finished = true;
                    return Poll::Ready(Ok(()));
                }
            }
        }
    }
}

impl Drop for OggFlacClientStream {
    fn drop(&mut self) {
        let count = self.handle.active_clients.fetch_sub(1, Ordering::SeqCst);
        debug!("OGG-FLAC client disconnected (remaining: {})", count - 1);

        if count == 1 {
            info!("Last OGG-FLAC client disconnected, signaling pipeline stop");
            self.handle.stop_token.cancel();
        }
    }
}

/// Internal state for encoder initialization.
struct EncoderState {
    broadcaster_task: tokio::task::JoinHandle<()>,
}

/// Logic for the streaming OGG-FLAC sink.
struct StreamingOggFlacSinkLogic {
    encoder_options: EncoderOptions,
    bits_per_sample: u8,
    pcm_tx: mpsc::Sender<PcmChunk>,
    pcm_rx: Option<mpsc::Receiver<PcmChunk>>,
    metadata: Arc<RwLock<MetadataSnapshot>>,
    ogg_broadcast: timed_broadcast::Sender<Bytes>,
    ogg_header: Arc<RwLock<Option<Bytes>>>,
    encoder_state: Option<EncoderState>,
    sample_rate: Option<u32>,
    broadcast_max_lead_time: f64,
}

impl StreamingOggFlacSinkLogic {
    /// Initialize the FLAC encoder once we know the sample rate.
    async fn initialize_encoder(&mut self, sample_rate: u32) -> Result<(), AudioError> {
        if self.encoder_state.is_some() {
            return Ok(()); // Already initialized
        }

        info!(
            "Initializing OGG-FLAC encoder with sample rate: {} Hz",
            sample_rate
        );

        // Take the PCM receiver (we only initialize once)
        let pcm_rx = self
            .pcm_rx
            .take()
            .ok_or_else(|| AudioError::ProcessingError("PCM receiver already consumed".into()))?;

        // Create shared timestamp for pacing
        let current_timestamp = Arc::new(RwLock::new(0.0f64));

        // Create ByteStreamReader for the encoder
        let pcm_reader = ByteStreamReader::new(pcm_rx, current_timestamp.clone());

        // Create PCM format
        let pcm_format = PcmFormat {
            sample_rate,
            channels: 2,
            bits_per_sample: self.bits_per_sample,
        };

        // Start the FLAC encoder
        let flac_stream = encode_flac_stream(pcm_reader, pcm_format, self.encoder_options.clone())
            .await
            .map_err(|e| {
                AudioError::ProcessingError(format!("Failed to start FLAC encoder: {}", e))
            })?;

        info!("OGG-FLAC encoder initialized successfully");

        // Spawn OGG wrapper + broadcaster task with timestamp for pacing
        let ogg_broadcast = self.ogg_broadcast.clone();
        let ogg_header = self.ogg_header.clone();
        let max_lead = self.broadcast_max_lead_time;
        let broadcaster_task = tokio::spawn(async move {
            if let Err(e) = broadcast_ogg_flac_stream(
                flac_stream,
                ogg_broadcast,
                ogg_header,
                current_timestamp,
                max_lead,
            )
            .await
            {
                error!("OGG broadcaster task error: {}", e);
            }
        });

        self.encoder_state = Some(EncoderState { broadcaster_task });

        info!("OGG broadcaster task spawned");

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
            "OGG-FLAC metadata updated: v{} @ {:.2}s - {} - {}",
            snapshot.version,
            timestamp_sec,
            snapshot.artist.as_deref().unwrap_or("?"),
            snapshot.title.as_deref().unwrap_or("?")
        );

        Ok(())
    }
}

#[async_trait]
impl NodeLogic for StreamingOggFlacSinkLogic {
    async fn process(
        &mut self,
        input: Option<mpsc::Receiver<Arc<AudioSegment>>>,
        _output: Vec<mpsc::Sender<Arc<AudioSegment>>>,
        stop_token: CancellationToken,
    ) -> Result<(), AudioError> {
        let mut input = input.ok_or_else(|| {
            AudioError::ProcessingError("StreamingOggFlacSink requires an input".into())
        })?;

        info!("StreamingOggFlacSink started");

        // TODO: Implement OGG-FLAC encoding logic
        // For now, just process segments without encoding

        loop {
            tokio::select! {
                _ = stop_token.cancelled() => {
                    info!("StreamingOggFlacSink stopped by cancellation");
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

                                    // Send to FLAC encoder with timestamp
                                    let pcm_chunk = PcmChunk {
                                        bytes: pcm_bytes,
                                        timestamp_sec: seg.timestamp_sec,
                                    };
                                    if let Err(e) = self.pcm_tx.send(pcm_chunk).await {
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
                                            // TODO: Implement OGG chaining (EOS → new BOS)
                                        }

                                        SyncMarker::EndOfStream => {
                                            info!("End of stream marker received");
                                            break;
                                        }

                                        SyncMarker::TopZeroSync => {
                                            self.ogg_broadcast.mark_top_zero();
                                            trace!("TopZeroSync propagated to timed broadcast");
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

        info!("StreamingOggFlacSink processing complete");
        Ok(())
    }

    async fn cleanup(&mut self, reason: StopReason) -> Result<(), AudioError> {
        info!("StreamingOggFlacSink cleanup: {:?}", reason);
        Ok(())
    }
}

/// Streaming OGG-FLAC sink for multi-client HTTP streaming with track metadata.
pub struct StreamingOggFlacSink {
    inner: Node<StreamingOggFlacSinkLogic>,
}

impl StreamingOggFlacSink {
    /// Create a new streaming OGG-FLAC sink.
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
    ) -> (Self, OggFlacStreamHandle) {
        Self::with_max_broadcast_lead(
            encoder_options,
            bits_per_sample,
            DEFAULT_BROADCAST_MAX_LEAD_TIME,
        )
    }

    /// Create a sink with a custom broadcast pacing limit.
    pub fn with_max_broadcast_lead(
        encoder_options: EncoderOptions,
        bits_per_sample: u8,
        broadcast_max_lead_time: f64,
    ) -> (Self, OggFlacStreamHandle) {
        // Validate bit depth
        if ![16, 24, 32].contains(&bits_per_sample) {
            panic!("bits_per_sample must be 16, 24, or 32");
        }

        // Create PCM channel (bounded for backpressure)
        let (pcm_tx, pcm_rx) = mpsc::channel::<PcmChunk>(16);

        // Shared metadata
        let metadata = Arc::new(RwLock::new(MetadataSnapshot::default()));

        // Broadcast channel for OGG-FLAC bytes
        // Capacity calculated from max_lead_time to ensure enough buffering
        let broadcast_capacity = calculate_broadcast_capacity(broadcast_max_lead_time);
        tracing::debug!(
            "OGG-FLAC broadcast capacity: {} items (for {:.1}s max lead time)",
            broadcast_capacity,
            broadcast_max_lead_time
        );
        let (ogg_broadcast, _) = timed_broadcast::channel(broadcast_capacity);

        // OGG-FLAC header cache
        let ogg_header = Arc::new(RwLock::new(None));

        // Stop token and client counter
        let stop_token = CancellationToken::new();
        let active_clients = Arc::new(AtomicUsize::new(0));

        let handle = OggFlacStreamHandle {
            ogg_broadcast: ogg_broadcast.clone(),
            metadata: metadata.clone(),
            active_clients,
            stop_token: stop_token.clone(),
            ogg_header: ogg_header.clone(),
        };

        let logic = StreamingOggFlacSinkLogic {
            encoder_options,
            bits_per_sample,
            pcm_tx,
            pcm_rx: Some(pcm_rx),
            metadata,
            ogg_broadcast,
            ogg_header,
            encoder_state: None,
            sample_rate: None,
            broadcast_max_lead_time: broadcast_max_lead_time.max(0.0),
        };

        let sink = Self {
            inner: Node::new_with_input(logic, 16),
        };

        (sink, handle)
    }
}

#[async_trait]
impl AudioPipelineNode for StreamingOggFlacSink {
    fn get_tx(&self) -> Option<mpsc::Sender<Arc<AudioSegment>>> {
        self.inner.get_tx()
    }

    fn register(&mut self, _child: Box<dyn AudioPipelineNode>) {
        panic!("StreamingOggFlacSink is a terminal sink and cannot have children");
    }

    async fn run(self: Box<Self>, stop_token: CancellationToken) -> Result<(), AudioError> {
        Box::new(self.inner).run(stop_token).await
    }

    fn start(self: Box<Self>) -> PipelineHandle {
        Box::new(self.inner).start()
    }
}

impl TypedAudioNode for StreamingOggFlacSink {
    fn input_type(&self) -> Option<TypeRequirement> {
        Some(TypeRequirement::any_integer())
    }

    fn output_type(&self) -> Option<TypeRequirement> {
        None
    }
}

/// AsyncRead adapter for mpsc::Receiver<PcmChunk>.
/// Extracts bytes from PcmChunk and provides them to the FLAC encoder.
struct ByteStreamReader {
    rx: mpsc::Receiver<PcmChunk>,
    buffer: VecDeque<u8>,
    finished: bool,
    /// Shared timestamp for broadcaster pacing
    current_timestamp: Arc<RwLock<f64>>,
}

impl ByteStreamReader {
    fn new(rx: mpsc::Receiver<PcmChunk>, current_timestamp: Arc<RwLock<f64>>) -> Self {
        Self {
            rx,
            buffer: VecDeque::new(),
            finished: false,
            current_timestamp,
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
                Poll::Ready(Some(chunk)) => {
                    if chunk.bytes.is_empty() {
                        continue;
                    }
                    // Update shared timestamp for broadcaster pacing
                    if let Ok(mut ts) = self.current_timestamp.try_write() {
                        *ts = chunk.timestamp_sec;
                    }
                    self.buffer.extend(chunk.bytes);
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

/// Convert an AudioChunk to PCM bytes with specified bit depth.
fn chunk_to_pcm_bytes(chunk: &AudioChunk, bits_per_sample: u8) -> Result<Vec<u8>, AudioError> {
    match chunk {
        AudioChunk::F32(_) | AudioChunk::F64(_) => {
            return Err(AudioError::ProcessingError(
                "StreamingOggFlacSink only supports integer audio chunks".into(),
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

/// OGG wrapper + broadcaster task: reads FLAC bytes from encoder, wraps in OGG pages, and broadcasts.
/// Implements precise real-time pacing based on audio timestamps.
/// Ensures FLAC frames are only sent at frame boundaries to prevent sync errors in strict decoders like FFPlay.
async fn broadcast_ogg_flac_stream(
    mut flac_stream: FlacEncodedStream,
    broadcast_tx: timed_broadcast::Sender<Bytes>,
    header_cache: Arc<RwLock<Option<Bytes>>>,
    current_timestamp: Arc<RwLock<f64>>,
    broadcast_max_lead_time: f64,
) -> Result<(), AudioError> {
    info!(
        "OGG-FLAC broadcaster task started with FLAC frame boundary detection (max_lead={:.3}s)",
        broadcast_max_lead_time
    );

    let stream_serial = rand::random::<u32>();
    let mut ogg_writer = OggPageWriter::new(stream_serial);

    let mut total_ogg_bytes = 0u64;
    let mut header_captured = false;
    let mut pacer = BroadcastPacer::new(broadcast_max_lead_time, "OGG");
    let mut last_granule_update_time = 0.0f64;

    // Timing instrumentation for burst detection
    let mut last_broadcast_time = std::time::Instant::now();
    let mut broadcast_count = 0u64;
    let mut total_read_time = 0.0f64;
    let mut read_count = 0u64;

    // Step 1: Read FLAC header (fLaC + metadata blocks)
    let flac_header = read_flac_header(&mut flac_stream).await?;
    info!("Read FLAC header: {} bytes", flac_header.len());

    // Extract sample rate from STREAMINFO for granule position calculation
    let sample_rate = extract_sample_rate_from_streaminfo(&flac_header)?;
    info!("Extracted sample rate from STREAMINFO: {} Hz", sample_rate);

    // Step 2: Create OGG-FLAC identification packet (BOS)
    // Format according to https://xiph.org/flac/ogg_mapping.html
    let ogg_flac_id = create_ogg_flac_identification(&flac_header)?;
    info!(
        "Created OGG-FLAC identification packet: {} bytes",
        ogg_flac_id.len()
    );

    let bos_page = ogg_writer.create_page(&ogg_flac_id, true, false, false);
    let bos_bytes = Bytes::from(bos_page);

    // Step 3: Create Vorbis Comment page (empty for now, metadata comes from /metadata endpoint)
    let vorbis_comment = create_empty_vorbis_comment();
    let comment_page = ogg_writer.create_page(&vorbis_comment, false, false, false);
    let comment_bytes = Bytes::from(comment_page);

    // Cache the header (BOS + Comment pages)
    let mut cached_header = Vec::new();
    cached_header.extend_from_slice(&bos_bytes);
    cached_header.extend_from_slice(&comment_bytes);
    *header_cache.write().await = Some(Bytes::from(cached_header));
    header_captured = true;
    info!(
        "OGG-FLAC header cached ({} bytes: BOS + Vorbis Comment)",
        bos_bytes.len() + comment_bytes.len()
    );

    // Broadcast header
    if broadcast_tx.send(bos_bytes.clone(), 0.0).await.is_err() {
        trace!("No receivers for BOS page, terminating broadcast");
        return Ok(());
    }
    total_ogg_bytes += comment_bytes.len() as u64;
    if broadcast_tx.send(comment_bytes.clone(), 0.0).await.is_err() {
        trace!("No receivers for comment page, terminating broadcast");
        return Ok(());
    }

    // Step 4: Read FLAC stream and create OGG packets
    // Use larger read buffer (16KB) to reduce syscalls and accumulator for frame boundary detection
    // The accumulator is necessary to ensure we only send complete FLAC frames
    let mut read_buffer = vec![0u8; 16384];
    let mut flac_accumulator = Vec::with_capacity(32768);

    loop {
        let read_start = std::time::Instant::now();
        match flac_stream.read(&mut read_buffer).await {
            Ok(0) => {
                // EOF - create final page with EOS flag and any remaining data
                if !flac_accumulator.is_empty() {
                    let eos_page = ogg_writer.create_page(&flac_accumulator, false, true, false);
                    let eos_bytes = Bytes::from(eos_page);
                    total_ogg_bytes += eos_bytes.len() as u64;
                    let eos_ts = *current_timestamp.read().await;
                    if broadcast_tx.send(eos_bytes.clone(), eos_ts).await.is_err() {
                        trace!("Broadcast closed before sending final EOS page");
                        break;
                    }
                    info!(
                        "Sent final EOS page with {} bytes of data",
                        flac_accumulator.len()
                    );
                } else {
                    // Send empty EOS page
                    let eos_page = ogg_writer.create_page(&[], false, true, false);
                    let eos_bytes = Bytes::from(eos_page);
                    total_ogg_bytes += eos_bytes.len() as u64;
                    let eos_ts = *current_timestamp.read().await;
                    if broadcast_tx.send(eos_bytes.clone(), eos_ts).await.is_err() {
                        trace!("Broadcast closed before sending empty EOS page");
                        break;
                    }
                    info!("Sent empty EOS page");
                }

                info!(
                    "OGG-FLAC stream ended, total OGG bytes: {}",
                    total_ogg_bytes
                );
                break;
            }
            Ok(n) => {
                let read_duration = read_start.elapsed().as_secs_f64();
                read_count += 1;
                total_read_time += read_duration;

                if read_duration > 0.01 {
                    debug!(
                        "OGG: flac_stream.read() took {:.3}s for {} bytes (avg: {:.3}s over {} reads)",
                        read_duration,
                        n,
                        total_read_time / read_count as f64,
                        read_count
                    );
                }

                // Append to accumulator
                flac_accumulator.extend_from_slice(&read_buffer[..n]);

                trace!(
                    "OGG: accumulator now {} bytes after reading {} bytes",
                    flac_accumulator.len(),
                    n
                );

                // Process complete FLAC frames one at a time
                // OGG-FLAC spec requires: "Each audio data packet contains one complete FLAC frame"
                loop {
                    // Find all complete frames in the accumulator
                    if flac_accumulator.len() < 4 {
                        break; // Need at least 4 bytes for sync code check
                    }

                    // Find all sync positions with their sample counts
                    // Use CRC-8 validation to eliminate false positives
                    let mut sync_data = Vec::new();
                    for i in 0..flac_accumulator.len() - 1 {
                        let byte1 = flac_accumulator[i];
                        let byte2 = flac_accumulator[i + 1];

                        if byte1 == 0xFF && byte2 >= 0xF8 && byte2 <= 0xFE {
                            // Validate frame header with CRC-8 to avoid false positives
                            if flac_frame_utils::validate_frame_header_crc(&flac_accumulator, i) {
                                if let Some(samples) =
                                    flac_frame_utils::parse_flac_block_size(&flac_accumulator, i)
                                {
                                    sync_data.push((i, samples));
                                }
                            }
                        }
                    }

                    // Need at least 2 sync codes to identify one complete frame
                    if sync_data.len() < 2 {
                        break; // No complete frames yet
                    }

                    // Extract the first complete frame (from first sync to second sync)
                    let first_frame_start = sync_data[0].0;
                    let first_frame_samples = sync_data[0].1;
                    let second_frame_start = sync_data[1].0;

                    // Verify first frame starts at position 0 (otherwise we have garbage data)
                    if first_frame_start != 0 {
                        warn!(
                            "OGG-FLAC: Skipping {} bytes of garbage data before first frame",
                            first_frame_start
                        );
                        flac_accumulator.drain(0..first_frame_start);
                        continue;
                    }

                    // Extract just the first frame
                    let first_frame: Vec<u8> =
                        flac_accumulator.drain(0..second_frame_start).collect();

                    // ╔═══════════════════════════════════════════════════════════════╗
                    // ║ BACKPRESSURE INTELLIGENTE BASÉE SUR LE TIMING                 ║
                    // ║                                                               ║
                    // ║ BroadcastPacer gère :                                         ║
                    // ║ 1. Détection TopZeroSync (audio_ts < 0.1)                    ║
                    // ║ 2. Drop des chunks en retard (audio_ts < elapsed)            ║
                    // ║ 3. Pacing pour contrôler le débit (max_lead_time)            ║
                    // ║                                                               ║
                    // ║ Cela crée la backpressure vers TimerBufferNode tout en       ║
                    // ║ permettant de dropper les chunks vraiment périmés.           ║
                    // ╚═══════════════════════════════════════════════════════════════╝
                    let audio_timestamp = *current_timestamp.read().await;

                    // Check timing et apply pacing (skip si en retard)
                    if pacer.check_and_pace(audio_timestamp).await.is_err() {
                        continue; // Skip ce chunk (trop en retard)
                    }

                    // Update granule position (cumulative sample count)
                    ogg_writer.add_samples(first_frame_samples as u64);

                    // Wrap this single FLAC frame in ONE OGG page (per OGG-FLAC spec)
                    let ogg_page = ogg_writer.create_page(&first_frame, false, false, false);
                    let ogg_bytes = Bytes::from(ogg_page);
                    total_ogg_bytes += ogg_bytes.len() as u64;

                    // Measure broadcast interval for burst detection
                    let broadcast_interval = last_broadcast_time.elapsed().as_secs_f64();
                    last_broadcast_time = std::time::Instant::now();
                    broadcast_count += 1;

                    // Log if interval is unusual (too short = burst, too long = stall)
                    if broadcast_interval < 0.01 || broadcast_interval > 0.1 {
                        debug!(
                            "OGG: broadcast interval {:.3}s ({}ms) - frame_size={} bytes, samples={} (count={})",
                            broadcast_interval,
                            (broadcast_interval * 1000.0) as u32,
                            first_frame.len(),
                            first_frame_samples,
                            broadcast_count
                        );
                    }

                    // Periodic stats
                    if broadcast_count % 100 == 0 {
                        debug!(
                            "OGG: {} broadcasts sent, avg_interval={:.3}s, accumulator={} bytes",
                            broadcast_count,
                            last_broadcast_time.elapsed().as_secs_f64() / broadcast_count as f64,
                            flac_accumulator.len()
                        );
                    }

                    // Envoyer au broadcast
                    match broadcast_tx.send(ogg_bytes.clone(), audio_timestamp).await {
                        Ok(n) => {
                            trace!("Broadcasted OGG page with 1 FLAC frame ({} bytes), {} samples ({} bytes total with OGG overhead) to {} receivers", first_frame.len(), first_frame_samples, ogg_bytes.len(), n);
                        }
                        Err(_) => {
                            trace!("No active receivers for OGG-FLAC broadcast, terminating loop");
                            return Ok(());
                        }
                    }
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

    info!("OGG-FLAC broadcaster task completed successfully");
    Ok(())
}

/// Extract sample rate from STREAMINFO block in FLAC header
fn extract_sample_rate_from_streaminfo(flac_header: &[u8]) -> Result<u32, AudioError> {
    // Verify we have at least "fLaC" magic + STREAMINFO block header
    if flac_header.len() < 8 {
        return Err(AudioError::ProcessingError("FLAC header too short".into()));
    }

    if &flac_header[0..4] != b"fLaC" {
        return Err(AudioError::ProcessingError("Invalid FLAC magic".into()));
    }

    // First metadata block should be STREAMINFO (type 0)
    let block_type = flac_header[4] & 0x7F;
    if block_type != 0 {
        return Err(AudioError::ProcessingError(
            "First block is not STREAMINFO".into(),
        ));
    }

    // STREAMINFO data starts at offset 8 (after magic + block header)
    // Sample rate is at offset 10-12 of STREAMINFO data (bytes 18-20 of header)
    if flac_header.len() < 21 {
        return Err(AudioError::ProcessingError(
            "STREAMINFO block truncated".into(),
        ));
    }

    // Sample rate: 20 bits starting at byte 10 of STREAMINFO
    // Format: [byte10: SSSSSSSS] [byte11: SSSSSSSS] [byte12: SSSSCCCC]
    // S = sample rate bits, C = channels bits
    let byte10 = flac_header[18] as u32;
    let byte11 = flac_header[19] as u32;
    let byte12 = flac_header[20] as u32;

    // Extract 20 bits for sample rate (top 20 bits of 3 bytes)
    let sample_rate = (byte10 << 12) | (byte11 << 4) | (byte12 >> 4);

    if sample_rate == 0 {
        return Err(AudioError::ProcessingError(
            "Invalid sample rate (0)".into(),
        ));
    }

    Ok(sample_rate)
}

/// Read FLAC header (fLaC + all metadata blocks until first frame)
async fn read_flac_header(stream: &mut FlacEncodedStream) -> Result<Vec<u8>, AudioError> {
    let mut header = Vec::new();
    let mut buffer = [0u8; 4];

    // Read "fLaC" magic
    stream
        .read_exact(&mut buffer)
        .await
        .map_err(|e| AudioError::ProcessingError(format!("Failed to read FLAC magic: {}", e)))?;

    if &buffer != b"fLaC" {
        return Err(AudioError::ProcessingError(
            "Invalid FLAC stream: missing fLaC magic".into(),
        ));
    }

    header.extend_from_slice(&buffer);

    // Read metadata blocks
    loop {
        // Read metadata block header (1 byte type + 3 bytes length)
        let mut block_header = [0u8; 4];
        stream.read_exact(&mut block_header).await.map_err(|e| {
            AudioError::ProcessingError(format!("Failed to read metadata block header: {}", e))
        })?;

        let is_last = (block_header[0] & 0x80) != 0;
        let block_length =
            u32::from_be_bytes([0, block_header[1], block_header[2], block_header[3]]) as usize;

        header.extend_from_slice(&block_header);

        // Read metadata block data
        let mut block_data = vec![0u8; block_length];
        stream.read_exact(&mut block_data).await.map_err(|e| {
            AudioError::ProcessingError(format!("Failed to read metadata block data: {}", e))
        })?;

        header.extend_from_slice(&block_data);

        if is_last {
            break;
        }
    }

    Ok(header)
}

/// Create OGG-FLAC identification packet (first packet in BOS page)
/// Format: https://xiph.org/flac/ogg_mapping.html
fn create_ogg_flac_identification(flac_header: &[u8]) -> Result<Vec<u8>, AudioError> {
    // Verify we have at least "fLaC" magic
    if flac_header.len() < 4 || &flac_header[0..4] != b"fLaC" {
        return Err(AudioError::ProcessingError("Invalid FLAC header".into()));
    }

    // Extract STREAMINFO block (first metadata block)
    // Format: 1 byte type+flags, 3 bytes length, N bytes data
    if flac_header.len() < 8 {
        return Err(AudioError::ProcessingError("FLAC header too short".into()));
    }

    let first_block_type = flac_header[4] & 0x7F; // Remove last-metadata-block flag
    if first_block_type != 0 {
        return Err(AudioError::ProcessingError(
            "First FLAC metadata block is not STREAMINFO".into(),
        ));
    }

    // Extract block length (3 bytes big-endian after type byte)
    let block_length =
        u32::from_be_bytes([0, flac_header[5], flac_header[6], flac_header[7]]) as usize;

    info!("STREAMINFO block_length = {} bytes", block_length);

    // STREAMINFO should be exactly 34 bytes of data
    if block_length != 34 {
        warn!("STREAMINFO block length is {} (expected 34)", block_length);
    }

    // Total STREAMINFO block size = 1 (type) + 3 (length) + block_length
    let streaminfo_size = 4 + block_length;

    if flac_header.len() < 4 + streaminfo_size {
        return Err(AudioError::ProcessingError("FLAC header truncated".into()));
    }

    // Extract just the STREAMINFO block (type + length + data)
    let streaminfo = &flac_header[4..4 + streaminfo_size];

    info!(
        "Extracted STREAMINFO: {} bytes (type+length+data)",
        streaminfo.len()
    );

    let mut packet = Vec::new();

    // OGG-FLAC identification header
    packet.push(0x7F); // Byte 0: 0x7F
    packet.extend_from_slice(b"FLAC"); // Bytes 1-4: "FLAC"
    packet.push(0x01); // Byte 5: Major version
    packet.push(0x00); // Byte 6: Minor version
    packet.extend_from_slice(&1u16.to_be_bytes()); // Bytes 7-8: 1 header packet (Vorbis Comment)
    packet.extend_from_slice(b"fLaC"); // Bytes 9-12: Native FLAC signature
    packet.extend_from_slice(streaminfo); // Bytes 13+: STREAMINFO block only

    Ok(packet)
}

/// Create empty Vorbis Comment block as a proper FLAC metadata block
fn create_empty_vorbis_comment() -> Vec<u8> {
    let mut vorbis_data = Vec::new();

    // Vendor string (Vorbis Comment format)
    let vendor = "pmoaudio OGG-FLAC streamer";
    let vendor_bytes = vendor.as_bytes();
    vorbis_data.extend_from_slice(&(vendor_bytes.len() as u32).to_le_bytes());
    vorbis_data.extend_from_slice(vendor_bytes);

    // Number of comments (0 for now - metadata via /metadata endpoint)
    vorbis_data.extend_from_slice(&0u32.to_le_bytes());

    // Now wrap in FLAC metadata block format
    let mut block = Vec::new();

    // Byte 0: block type (4 = VORBIS_COMMENT) + last-metadata-block flag (bit 7 = 1)
    block.push(0x84); // 0x80 | 0x04 = last block + VORBIS_COMMENT type

    // Bytes 1-3: block length (24-bit big-endian)
    let length = vorbis_data.len() as u32;
    block.push((length >> 16) as u8);
    block.push((length >> 8) as u8);
    block.push(length as u8);

    // Block data
    block.extend_from_slice(&vorbis_data);

    block
}

/// OGG page writer with granule position tracking for FLAC
struct OggPageWriter {
    stream_serial: u32,
    page_sequence: u32,
    granule_position: u64,
}

impl OggPageWriter {
    fn new(stream_serial: u32) -> Self {
        Self {
            stream_serial,
            page_sequence: 0,
            granule_position: 0,
        }
    }

    /// Add samples to the granule position (for FLAC: cumulative PCM sample count)
    fn add_samples(&mut self, samples: u64) {
        self.granule_position += samples;
    }

    fn create_page(
        &mut self,
        packet_data: &[u8],
        is_bos: bool,
        is_eos: bool,
        is_continuation: bool,
    ) -> Vec<u8> {
        use std::io::Write;

        let mut segments = Vec::new();
        let mut remaining = packet_data.len();

        // Segment the packet into 255-byte chunks
        while remaining > 0 {
            let segment_size = remaining.min(255);
            segments.push(segment_size as u8);
            remaining -= segment_size;
        }

        // If packet ends exactly on a 255-byte boundary, add empty segment
        if !packet_data.is_empty() && packet_data.len() % 255 == 0 && !is_continuation {
            segments.push(0);
        }

        let segment_count = segments.len();
        let header_size = 27 + segment_count;
        let total_size = header_size + packet_data.len();

        let mut page = Vec::with_capacity(total_size);

        // OGG page header
        page.write_all(b"OggS").unwrap();
        page.write_all(&[0]).unwrap(); // Version

        // Header type
        let mut header_type = 0u8;
        if is_continuation {
            header_type |= 0x01;
        }
        if is_bos {
            header_type |= 0x02;
        }
        if is_eos {
            header_type |= 0x04;
        }
        page.write_all(&[header_type]).unwrap();

        // Granule position
        page.write_all(&self.granule_position.to_le_bytes())
            .unwrap();

        // Stream serial number
        page.write_all(&self.stream_serial.to_le_bytes()).unwrap();

        // Page sequence number
        page.write_all(&self.page_sequence.to_le_bytes()).unwrap();
        self.page_sequence += 1;

        // CRC checksum (zero for now, calculated later)
        let crc_offset = page.len();
        page.write_all(&[0, 0, 0, 0]).unwrap();

        // Number of segments
        page.write_all(&[segment_count as u8]).unwrap();

        // Segment table
        page.write_all(&segments).unwrap();

        // Packet data
        page.write_all(packet_data).unwrap();

        // Calculate and insert CRC32
        let crc = calculate_ogg_crc(&page);
        page[crc_offset..crc_offset + 4].copy_from_slice(&crc.to_le_bytes());

        page
    }
}

/// Calculate OGG CRC32 checksum
fn calculate_ogg_crc(data: &[u8]) -> u32 {
    const CRC_TABLE: [u32; 256] = generate_crc_table();

    let mut crc: u32 = 0;
    for &byte in data {
        crc = (crc << 8) ^ CRC_TABLE[((crc >> 24) ^ (byte as u32)) as usize];
    }
    crc
}

/// Generate CRC lookup table at compile time
const fn generate_crc_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let mut r = i << 24;
        let mut j = 0;
        while j < 8 {
            if (r & 0x80000000) != 0 {
                r = (r << 1) ^ 0x04c11db7;
            } else {
                r <<= 1;
            }
            j += 1;
        }
        table[i as usize] = r;
        i += 1;
    }
    table
}
