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
use pmoflac::{EncoderOptions, PcmFormat};
use pmometadata::TrackMetadata;
use tokio::io::{AsyncRead, ReadBuf};
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

/// Logic for the streaming OGG-FLAC sink.
struct StreamingOggFlacSinkLogic {
    encoder_options: EncoderOptions,
    bits_per_sample: u8,
    metadata: Arc<RwLock<MetadataSnapshot>>,
    ogg_broadcast: broadcast::Sender<Bytes>,
    ogg_header: Arc<RwLock<Option<Bytes>>>,
    sample_rate: Option<u32>,
}

impl StreamingOggFlacSinkLogic {
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
                                    // Detect sample rate from first chunk
                                    if self.sample_rate.is_none() {
                                        let sample_rate = chunk.sample_rate();
                                        self.sample_rate = Some(sample_rate);
                                        info!("Detected sample rate: {} Hz", sample_rate);
                                        // TODO: Initialize OGG-FLAC encoder
                                    }

                                    trace!(
                                        "Received chunk: {} samples @ {:.2}s",
                                        chunk.len(),
                                        seg.timestamp_sec
                                    );

                                    // TODO: Convert chunk to PCM and send to encoder
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
            metadata,
            ogg_broadcast,
            ogg_header,
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
