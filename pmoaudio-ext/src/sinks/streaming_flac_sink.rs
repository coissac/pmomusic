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
//! timed_broadcast::channel<Bytes> (FLAC bytes)
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
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use super::{
    broadcast_pacing::BroadcastPacer,
    flac_frame_utils,
    timed_broadcast::{self, SendError, TryRecvError},
};
use async_trait::async_trait;
use bytes::Bytes;
use pmoaudio::{
    pipeline::{AudioPipelineNode, Node, NodeLogic, PipelineHandle, StopReason},
    AudioError, AudioSegment, SyncMarker, TypeRequirement, TypedAudioNode, _AudioSegment,
};
use pmoflac::{EncoderOptions, FlacEncodedStream};
use tokio::io::{AsyncRead, AsyncReadExt, ReadBuf};
use tokio::sync::{mpsc, RwLock};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, trace, warn};

use crate::byte_stream_reader::{PcmChunk};
use crate::chunk_to_pcm::chunk_to_pcm_bytes;
use crate::sinks::streaming_sink_common::{
    MetadataSnapshot, SharedClientStream, SharedSinkContext, SharedStreamHandleInner,
};
use crate::sinks::timed_broadcast::{
    calculate_broadcast_capacity, DEFAULT_BROADCAST_MAX_LEAD_TIME,
};
use crate::streaming_icyflac_sink::IcyClientStream;

/// Default ICY metadata interval (bytes of audio between metadata blocks).
/// Standard value used by most streaming servers.
const DEFAULT_ICY_METAINT: usize = 16000;

/// Handle for accessing the FLAC stream and metadata from HTTP handlers.
#[derive(Clone)]
pub struct StreamHandle {
    inner: Arc<SharedStreamHandleInner>,
}

impl StreamHandle {
    pub fn new(inner: Arc<SharedStreamHandleInner>) -> Self {
        Self { inner }
    }

    pub fn subscribe_flac(&self) -> FlacClientStream {
        let total = self.inner.client_connected();
        let rx = self.inner.register_client();
        debug!("New FLAC client subscribed (total: {})", total);
        FlacClientStream::new(rx, self.inner.clone())
    }

    pub fn subscribe_icy(&self) -> IcyClientStream {
        self.subscribe_icy_with_interval(DEFAULT_ICY_METAINT)
    }

    pub fn subscribe_icy_with_interval(&self, metaint: usize) -> IcyClientStream {
        let total = self.inner.client_connected();
        let rx = self.inner.register_client();
        debug!(
            "New ICY client subscribed (total: {}, metaint: {})",
            total, metaint
        );

        IcyClientStream::new(rx, self.inner.clone(), metaint)
    }

    pub async fn get_metadata(&self) -> MetadataSnapshot {
        self.inner.metadata.read().await.clone()
    }

    pub fn active_client_count(&self) -> usize {
        self.inner.active_clients.load(Ordering::SeqCst)
    }

    pub fn should_stop(&self) -> bool {
        self.inner.active_clients.load(Ordering::SeqCst) == 0
    }

    pub fn set_auto_stop(&self, enabled: bool) {
        self.inner.auto_stop.store(enabled, Ordering::SeqCst);
    }
}


pub struct FlacClientStream {
    inner: SharedClientStream,
}

impl FlacClientStream {
    fn new(rx: timed_broadcast::Receiver<Bytes>, handle: Arc<SharedStreamHandleInner>) -> Self {
        Self {
            inner: SharedClientStream::new(rx, handle),
        }
    }

    pub fn current_epoch(&self) -> u64 {
        self.inner.current_epoch()
    }
}

impl AsyncRead for FlacClientStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

impl Drop for FlacClientStream {
    fn drop(&mut self) {
        let remaining = self.inner.handle().client_disconnected();
        debug!("FLAC client disconnected (remaining: {})", remaining);
    }
}

struct StreamingFlacSinkLogic {
    ctx: SharedSinkContext,
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

        debug!("StreamingFlacSink started");

        // We'll initialize the encoder lazily when we get the first chunk
        // For now, just process segments

        loop {
            tokio::select! {
                _ = stop_token.cancelled() => {
                    debug!("StreamingFlacSink stopped by cancellation");
                    break;
                }

                segment = input.recv() => {
                    match segment {
                        Some(seg) => {
                            match &seg.segment {
                                _AudioSegment::Chunk(chunk) => {
                                    if !self.ctx.first_chunk_timestamp_checked {
                                        self.ctx.first_chunk_timestamp_checked = true;
                                        if seg.timestamp_sec.abs() > 1e-6 {
                                            warn!(
                                                "StreamingFlacSink: first chunk timestamp is {:.6}s (expected 0.0)",
                                                seg.timestamp_sec
                                            );
                                        } else {
                                            trace!("StreamingFlacSink: first chunk timestamp verified at 0.0s");
                                        }
                                    }

                                    // Detect sample rate from first chunk and initialize encoder
                                    if self.ctx.sample_rate.is_none() {
                                        let sample_rate = chunk.sample_rate();
                                        self.ctx.sample_rate = Some(sample_rate);
                                        debug!("Detected sample rate: {} Hz", sample_rate);

                                        // Initialize the FLAC encoder now (first track starts at 0.0)
                                        self.ctx
                                            .initialize_encoder(
                                                sample_rate,
                                                0.0,
                                                |flac_stream,
                                                 broadcast,
                                                 header,
                                                 current_timestamp,
                                                 current_duration,
                                                 max_lead,
                                                 sample_rate,
                                                 timestamp_offset_sec| {
                                                    broadcast_flac_stream(
                                                        flac_stream,
                                                        broadcast,
                                                        header,
                                                        current_timestamp,
                                                        current_duration,
                                                        max_lead,
                                                        sample_rate,
                                                        timestamp_offset_sec,
                                                    )
                                                },
                                            )
                                            .await?;
                                    }

                                    // Convert chunk to PCM bytes
                                    let pcm_bytes = chunk_to_pcm_bytes(&chunk, self.ctx.bits_per_sample)?;

                                    // Calculate exact duration from samples and sample rate
                                    let sample_rate = self.ctx.sample_rate
                                        .expect("sample_rate should be initialized");
                                    let duration_sec = chunk.len() as f64 / sample_rate as f64;

                                    trace!(
                                        "Sending PCM chunk: {} bytes, {} samples @ {:.2}s (duration={:.3}s)",
                                        pcm_bytes.len(),
                                        chunk.len(),
                                        seg.timestamp_sec,
                                        duration_sec
                                    );

                                    // Send to FLAC encoder with timestamp and duration
                                    let pcm_chunk = PcmChunk {
                                        bytes: pcm_bytes,
                                        timestamp_sec: seg.timestamp_sec,
                                        duration_sec,
                                    };
                                    let send_start = std::time::Instant::now();

                                    // Get the sender (it should always be Some after initialization)
                                    let pcm_tx = match &self.ctx.pcm_tx {
                                        Some(tx) => tx,
                                        None => {
                                            error!("PCM sender not initialized");
                                            break;
                                        }
                                    };

                                    if let Err(e) = pcm_tx.send(pcm_chunk).await {
                                        warn!("Failed to send PCM data to encoder: {}", e);
                                        break;
                                    }
                                    let send_duration = send_start.elapsed();
                                    if send_duration.as_millis() >= 50 {
                    trace!(
                        "StreamingFlacSink: pcm_tx send blocked for {:.3}s (ts={:.3}s)",
                        send_duration.as_secs_f64(),
                        seg.timestamp_sec
                    );
                                    }
                                }

                                _AudioSegment::Sync(marker) => {
                                    match marker.as_ref() {
                                        SyncMarker::TrackBoundary { metadata } => {
                                            // Only restart encoder if it's already initialized (not the first track)
                                            if self.ctx.sample_rate.is_some() && self.ctx.encoder_state.is_some() {
                                                // Restart encoder to emit new header and reset timestamps
                                                if let Err(e) = self
                                                    .ctx
                                                    .restart_encoder_for_new_track(
                                                        |flac_stream,
                                                         broadcast,
                                                         header,
                                                         current_timestamp,
                                                         current_duration,
                                                         max_lead,
                                                         sample_rate,
                                                         timestamp_offset_sec| {
                                                            broadcast_flac_stream(
                                                                flac_stream,
                                                                broadcast,
                                                                header,
                                                                current_timestamp,
                                                                current_duration,
                                                                max_lead,
                                                                sample_rate,
                                                                timestamp_offset_sec,
                                                            )
                                                        },
                                                    )
                                                    .await
                                                {
                                                    error!("Failed to restart encoder for new track: {}", e);
                                                    break;
                                                }
                                            } else {
                                                trace!("Skipping encoder restart for first track (encoder not yet initialized)");
                                            }

                                            // Update metadata for the new track
                                            if let Err(e) = self.ctx.update_metadata(metadata, seg.timestamp_sec).await {
                                                error!("Failed to update metadata: {}", e);
                                            }
                                        }

                                        SyncMarker::EndOfStream => {
                                            debug!("End of stream marker received");
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
                            debug!("Input channel closed");
                            break;
                        }
                    }
                }
            }
        }

        debug!("StreamingFlacSink processing complete");
        Ok(())
    }

    async fn cleanup(&mut self, reason: StopReason) -> Result<(), AudioError> {
        debug!("StreamingFlacSink cleanup: {:?}", reason);
        Ok(())
    }
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
    pub fn new(encoder_options: EncoderOptions, bits_per_sample: u8) -> (Self, StreamHandle) {
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
    ) -> (Self, StreamHandle) {
        // Validate bit depth
        if ![16, 24, 32].contains(&bits_per_sample) {
            panic!("bits_per_sample must be 16, 24, or 32");
        }

        // Create PCM channel (bounded for backpressure)
        let (pcm_tx, pcm_rx) = mpsc::channel::<PcmChunk>(16);

        // Shared metadata
        let metadata = Arc::new(RwLock::new(MetadataSnapshot::default()));

        // Capacity calculated from max_lead_time to ensure enough buffering
        let broadcast_capacity = calculate_broadcast_capacity(broadcast_max_lead_time);
        debug!(
            "Streaming Sink: using broadcast capacity of {} items (max_lead_time={:.1}s)",
            broadcast_capacity, broadcast_max_lead_time
        );

        // Broadcast channel for FLAC bytes
        let (broadcast, _) = timed_broadcast::channel("Flac", broadcast_capacity);

        // FLAC header cache
        let header = Arc::new(RwLock::new(None));

        // Stop token and client counter
        let stop_token = CancellationToken::new();
        let auto_stop = Arc::new(AtomicBool::new(true));

        let shared_handle = Arc::new(SharedStreamHandleInner::new(
            broadcast.clone(),
            metadata.clone(),
            stop_token.clone(),
            header.clone(),
            auto_stop.clone(),
        ));

        let handle = StreamHandle::new(shared_handle.clone());

        let logic = StreamingFlacSinkLogic {
            ctx: SharedSinkContext {
                encoder_options,
                bits_per_sample,
                pcm_tx: Some(pcm_tx),
                pcm_rx: Some(pcm_rx),
                metadata,
                broadcast,
                header,
                encoder_state: None,
                sample_rate: None,
                broadcast_max_lead_time: broadcast_max_lead_time.max(0.0),
                first_chunk_timestamp_checked: false,
                timestamp_offset_sec: 0.0,
                current_timestamp: Arc::new(RwLock::new(0.0)),
            },
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

/// Broadcaster task: reads FLAC bytes from encoder and broadcasts to all clients.
/// Implements precise real-time pacing based on audio timestamps.
/// Ensures data is sent at FLAC frame boundaries to prevent sync errors in strict decoders like FFPlay.
async fn broadcast_flac_stream(
    mut flac_stream: FlacEncodedStream,
    broadcast_tx: timed_broadcast::Sender<Bytes>,
    header_cache: Arc<RwLock<Option<Bytes>>>,
    current_timestamp: Arc<RwLock<f64>>,
    current_duration: Arc<RwLock<f64>>,
    broadcast_max_lead_time: f64,
    sample_rate: u32,
    timestamp_offset_sec: f64,
) -> Result<(), AudioError> {
    trace!(
        "Broadcaster task started with FLAC frame boundary detection (max_lead={:.3}s)",
        broadcast_max_lead_time
    );

    // Use larger read buffer (16KB) to reduce syscalls and accumulator for frame boundary detection
    // The accumulator is necessary to ensure we only send complete FLAC frames
    let mut read_buffer = vec![0u8; 16384];
    let mut accumulator = Vec::with_capacity(32768); // Pre-allocate to reduce reallocations
    let mut total_bytes = 0u64;
    let mut header_captured = false;
    let mut pacer = BroadcastPacer::new(broadcast_max_lead_time, "FLAC");
    let mut stats_last_log = std::time::Instant::now();

    // Timing instrumentation for burst detection
    let mut last_broadcast_time = std::time::Instant::now();
    let mut broadcast_count = 0u64;
    let mut total_read_time = 0.0f64;
    let mut read_count = 0u64;
    let sample_rate_f64 = sample_rate as f64;

    // Sample counter for calculating accurate timestamps (reset on new headers)
    let mut encoded_samples = 0u64;

    loop {
        let read_start = std::time::Instant::now();
        match flac_stream.read(&mut read_buffer).await {
            Ok(0) => {
                // EOF - send any remaining data
                if !accumulator.is_empty() {
                    let bytes = Bytes::from(std::mem::take(&mut accumulator));
                    let audio_ts = *current_timestamp.read().await;
                    let segment_dur = *current_duration.read().await;
                    match broadcast_tx
                        .send(bytes.clone(), audio_ts, segment_dur)
                        .await
                    {
                        Ok(_) => {}
                        Err(SendError::Expired(_)) => {
                            trace!("Broadcast expired before sending final FLAC data");
                        }
                        Err(SendError::Closed(_)) => {
                            trace!("Broadcast closed before sending final FLAC data");
                        }
                    }
                }
                trace!("FLAC encoder stream ended, total bytes: {}", total_bytes);
                break;
            }
            Ok(n) => {
                let read_duration = read_start.elapsed().as_secs_f64();
                read_count += 1;
                total_read_time += read_duration;

                if read_duration > 0.01 {
                    trace!(
                        "FLAC: flac_stream.read() took {:.3}s for {} bytes (avg: {:.3}s over {} reads)",
                        read_duration,
                        n,
                        total_read_time / read_count as f64,
                        read_count
                    );
                }

                total_bytes += n as u64;
                if total_bytes % 100000 == 0 || total_bytes < 10000 {
                    trace!(
                        "Read {} bytes from FLAC encoder (total: {})",
                        n,
                        total_bytes
                    );
                }

                // Append to accumulator
                accumulator.extend_from_slice(&read_buffer[..n]);

                trace!(
                    "FLAC: accumulator now {} bytes after reading {} bytes",
                    accumulator.len(),
                    n
                );

                // Locate complete audio frames and total samples
                // Since encoder restarts on TrackBoundary, we only see one header per encoder instance
                let (boundary, total_samples) =
                    flac_frame_utils::find_complete_frames_with_samples(&accumulator);

                trace!(
                    "Buffer state: accumulator={} bytes, boundary={} bytes, total_samples={}, will_send={}",
                    accumulator.len(),
                    boundary,
                    total_samples,
                    boundary >= 1024 && total_samples > 0
                );

                // Only broadcast if we have at least one complete frame (keep 1KB minimum to avoid tiny sends)
                if boundary >= 1024 && total_samples > 0 {
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

                    // Calculer le timestamp de cette FLAC frame (avec offset pour continuité entre tracks)
                    let frame_start_samples = encoded_samples;
                    encoded_samples = encoded_samples.saturating_add(total_samples);
                    let audio_timestamp =
                        timestamp_offset_sec + (frame_start_samples as f64 / sample_rate_f64);
                    let segment_duration = total_samples as f64 / sample_rate_f64;

                    if stats_last_log.elapsed() >= Duration::from_secs(1) {
                        trace!(
                            "Broadcaster pacing snapshot: audio_ts={:.3}s buffer_bytes={} samples={} ",
                            audio_timestamp,
                            accumulator.len(),
                            total_samples
                        );
                        stats_last_log = std::time::Instant::now();
                    }

                    // Check timing et apply pacing (skip si en retard)
                    if pacer.check_and_pace(audio_timestamp).await.is_err() {
                        // Chunk en retard : vider l'accumulator et continuer
                        accumulator.clear();
                        continue;
                    }

                    if let Ok(mut ts) = current_timestamp.try_write() {
                        *ts = audio_timestamp;
                    }
                    if let Ok(mut dur) = current_duration.try_write() {
                        *dur = segment_duration;
                    }

                    // Split at boundary to avoid copying - extract prefix, keep suffix
                    let remaining = accumulator.split_off(boundary);
                    let to_send = std::mem::replace(&mut accumulator, remaining);
                    let bytes = Bytes::from(to_send);

                    // Measure broadcast interval for burst detection
                    let broadcast_interval = last_broadcast_time.elapsed().as_secs_f64();
                    last_broadcast_time = std::time::Instant::now();
                    broadcast_count += 1;

                    // Log if interval is unusual (too short = burst, too long = stall)
                    if broadcast_interval < 0.01 || broadcast_interval > 0.1 {
                        trace!(
                            "FLAC: broadcast interval {:.3}s ({}ms) - size={} bytes (count={})",
                            broadcast_interval,
                            (broadcast_interval * 1000.0) as u32,
                            bytes.len(),
                            broadcast_count
                        );
                    }

                    // Periodic stats
                    if broadcast_count % 100 == 0 {
                        trace!(
                            "FLAC: {} broadcasts sent, accumulator={} bytes remaining",
                            broadcast_count,
                            accumulator.len()
                        );
                    }

                    // Cache FLAC header "fLaC" for late-joining clients
                    // Each encoder instance emits exactly one header at the start
                    if !header_captured && bytes.len() >= 4 && &bytes[0..4] == b"fLaC" {
                        header_captured = true;
                        *header_cache.write().await = Some(bytes.clone());
                        trace!(
                            "FLAC header captured and cached ({} bytes) for late-joining clients",
                            bytes.len()
                        );
                    }

                    let num_receivers = broadcast_tx.receiver_count();
                    match broadcast_tx
                        .send(bytes.clone(), audio_timestamp, segment_duration)
                        .await
                    {
                        Ok(_) => {
                            if num_receivers > 0 {
                                trace!(
                                    "Broadcasted {} bytes to {} receivers (ts={:.3}s, dur={:.3}s)",
                                    bytes.len(),
                                    num_receivers,
                                    audio_timestamp,
                                    segment_duration
                                );
                            }
                        }
                        Err(SendError::Expired(_)) => {
                            trace!(
                                "FLAC broadcast dropped expired packet (ts={:.3}s, dur={:.3}s)",
                                audio_timestamp,
                                segment_duration
                            );
                            continue;
                        }
                        Err(SendError::Closed(_)) => {
                            trace!("No active receivers for FLAC broadcast, terminating");
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

    trace!("Broadcaster task completed successfully");
    Ok(())
}
