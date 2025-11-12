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
//! broadcast::channel<Bytes> (OGG-FLAC bytes)
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

/// Broadcast channel capacity for OGG-FLAC bytes.
/// Same as StreamingFlacSink for consistency.
const BROADCAST_CAPACITY: usize = 128;

/// Snapshot of track metadata (reuse from streaming_flac_sink)
pub use super::streaming_flac_sink::MetadataSnapshot;

/// Handle for accessing the OGG-FLAC stream and metadata from HTTP handlers.
#[derive(Clone)]
pub struct OggFlacStreamHandle {
    /// Broadcast sender for OGG-FLAC bytes
    ogg_broadcast: broadcast::Sender<Bytes>,

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
    rx: broadcast::Receiver<Bytes>,
    buffer: VecDeque<u8>,
    finished: bool,
    handle: OggFlacStreamHandle,
    state: OggFlacStreamState,
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
                    info!("Sending cached OGG-FLAC header to new client ({} bytes)", header.len());
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
                Ok(bytes) => {
                    self.buffer.extend(bytes.iter());
                }
                Err(broadcast::error::TryRecvError::Empty) => {
                    cx.waker().wake_by_ref();
                    return Poll::Pending;
                }
                Err(broadcast::error::TryRecvError::Lagged(skipped)) => {
                    warn!("OGG-FLAC client lagged, skipped {} messages", skipped);
                }
                Err(broadcast::error::TryRecvError::Closed) => {
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
    pcm_tx: mpsc::Sender<Vec<u8>>,
    pcm_rx: Option<mpsc::Receiver<Vec<u8>>>,
    metadata: Arc<RwLock<MetadataSnapshot>>,
    ogg_broadcast: broadcast::Sender<Bytes>,
    ogg_header: Arc<RwLock<Option<Bytes>>>,
    encoder_state: Option<EncoderState>,
    sample_rate: Option<u32>,
}

impl StreamingOggFlacSinkLogic {
    /// Initialize the FLAC encoder once we know the sample rate.
    async fn initialize_encoder(&mut self, sample_rate: u32) -> Result<(), AudioError> {
        if self.encoder_state.is_some() {
            return Ok(()); // Already initialized
        }

        info!("Initializing OGG-FLAC encoder with sample rate: {} Hz", sample_rate);

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

        info!("OGG-FLAC encoder initialized successfully");

        // Spawn OGG wrapper + broadcaster task
        let ogg_broadcast = self.ogg_broadcast.clone();
        let ogg_header = self.ogg_header.clone();
        let broadcaster_task = tokio::spawn(async move {
            if let Err(e) = broadcast_ogg_flac_stream(flac_stream, ogg_broadcast, ogg_header).await {
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
                                            // TODO: Implement OGG chaining (EOS → new BOS)
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
        // Validate bit depth
        if ![16, 24, 32].contains(&bits_per_sample) {
            panic!("bits_per_sample must be 16, 24, or 32");
        }

        // Create PCM channel (bounded for backpressure)
        let (pcm_tx, pcm_rx) = mpsc::channel::<Vec<u8>>(16);

        // Shared metadata
        let metadata = Arc::new(RwLock::new(MetadataSnapshot::default()));

        // Broadcast channel for OGG-FLAC bytes
        let (ogg_broadcast, _) = broadcast::channel(BROADCAST_CAPACITY);

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
///
/// For now, this is a simplified version that just passes through FLAC bytes without OGG wrapping.
/// TODO: Implement proper OGG page generation with BOS/EOS flags and Vorbis Comments.
async fn broadcast_ogg_flac_stream(
    mut flac_stream: FlacEncodedStream,
    broadcast_tx: broadcast::Sender<Bytes>,
    header_cache: Arc<RwLock<Option<Bytes>>>,
) -> Result<(), AudioError> {
    info!("OGG-FLAC broadcaster task started (FLAC passthrough mode - OGG wrapping TODO)");

    let mut buffer = vec![0u8; 8192]; // 8KB buffer for reading
    let mut total_bytes = 0u64;
    let mut header_captured = false;

    loop {
        match flac_stream.read(&mut buffer).await {
            Ok(0) => {
                // EOF
                info!("OGG-FLAC encoder stream ended, total bytes: {}", total_bytes);
                break;
            }
            Ok(n) => {
                total_bytes += n as u64;
                trace!("Read {} bytes from FLAC encoder (total: {})", n, total_bytes);

                // Broadcast to all clients (TODO: wrap in OGG pages)
                let bytes = Bytes::copy_from_slice(&buffer[..n]);

                // Capture first chunk as header if it contains "fLaC"
                if !header_captured && bytes.len() >= 4 && &bytes[0..4] == b"fLaC" {
                    *header_cache.write().await = Some(bytes.clone());
                    header_captured = true;
                    info!("FLAC header captured ({} bytes) - will be wrapped in OGG later", bytes.len());
                }

                if let Err(e) = broadcast_tx.send(bytes) {
                    // No receivers, but that's okay - clients may not be connected yet
                    trace!("No active receivers for OGG-FLAC broadcast: {}", e);
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
