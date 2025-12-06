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

use std::io;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll};

use super::{
    broadcast_pacing::BroadcastPacer,
    flac_frame_utils,
    timed_broadcast::{self, SendError},
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
use tracing::{debug, error, trace, warn};

use crate::byte_stream_reader::PcmChunk;
use crate::chunk_to_pcm::chunk_to_pcm_bytes;
use crate::sinks::flac_frame_utils::{extract_sample_rate_from_streaminfo, read_flac_header};
use crate::sinks::streaming_sink_common::{
    MetadataSnapshot, SharedClientStream, SharedSinkContext, SharedStreamHandleInner,
    StreamingSinkOptions,
};
use crate::sinks::timed_broadcast::{
    calculate_broadcast_capacity, DEFAULT_BROADCAST_MAX_LEAD_TIME,
};

/// Handle for accessing the OGG-FLAC stream and metadata from HTTP handlers.
#[derive(Clone)]
pub struct OggFlacStreamHandle {
    inner: Arc<SharedStreamHandleInner>,
}

impl OggFlacStreamHandle {
    pub fn new(inner: Arc<SharedStreamHandleInner>) -> Self {
        Self { inner }
    }

    pub fn subscribe(&self) -> OggFlacClientStream {
        let total = self.inner.client_connected();
        let rx = self.inner.register_client();
        debug!("New OGG-FLAC client subscribed (total: {})", total);
        OggFlacClientStream::new(rx, self.inner.clone())
    }

    pub async fn get_metadata(&self) -> MetadataSnapshot {
        self.inner.metadata.read().await.clone()
    }

    pub fn active_client_count(&self) -> usize {
        self.inner.active_clients.load(Ordering::SeqCst)
    }

    pub fn set_auto_stop(&self, enabled: bool) {
        self.inner.auto_stop.store(enabled, Ordering::SeqCst);
    }
}

/// OGG-FLAC client stream (implements AsyncRead).
pub struct OggFlacClientStream {
    inner: SharedClientStream,
}

impl OggFlacClientStream {
    fn new(rx: timed_broadcast::Receiver<Bytes>, handle: Arc<SharedStreamHandleInner>) -> Self {
        Self {
            inner: SharedClientStream::new(rx, handle),
        }
    }

    pub fn current_epoch(&self) -> u64 {
        self.inner.current_epoch()
    }
}

impl AsyncRead for OggFlacClientStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

impl Drop for OggFlacClientStream {
    fn drop(&mut self) {
        let remaining = self.inner.handle().client_disconnected();
        debug!("OGG-FLAC client disconnected (remaining: {})", remaining);
    }
}
/// Logic for the streaming OGG-FLAC sink.
struct StreamingOggFlacSinkLogic {
    ctx: SharedSinkContext,
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

        debug!("StreamingOggFlacSink started");

        // TODO: Implement OGG-FLAC encoding logic
        // For now, just process segments without encoding

        loop {
            tokio::select! {
                _ = stop_token.cancelled() => {
                    debug!("StreamingOggFlacSink stopped by cancellation");
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

                                        // Populate total_samples if we already know the track duration.
                                        self.ctx.refresh_total_samples_with_sample_rate();

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
                                                 _sample_rate,
                                                 timestamp_offset_sec| {
                                                    broadcast_ogg_flac_stream(
                                                        flac_stream,
                                                        broadcast,
                                                        header,
                                                        current_timestamp,
                                                        current_duration,
                                                        max_lead,
                                                        timestamp_offset_sec,
                                                    )
                                                },
                                            )
                                            .await?;
                                    }

                                    // Convert chunk to PCM bytes
                                    let pcm_bytes = chunk_to_pcm_bytes(&chunk, self.ctx.bits_per_sample)?;

                                    // Calculate exact duration from samples and sample rate
                                    let sample_rate = self.ctx.sample_rate.expect("sample_rate should be initialized");
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

                                    // Get the sender (it should always be Some after initialization)
                                    let pcm_tx = match &self.ctx.pcm_tx {
                                        Some(tx) => tx,
                                        None => {
                                            error!("OGG PCM sender not initialized");
                                            break;
                                        }
                                    };

                                    if let Err(e) = pcm_tx.send(pcm_chunk).await {
                                        warn!("Failed to send PCM data to OGG encoder: {}", e);
                                        break;
                                    }
                                }

                                _AudioSegment::Sync(marker) => {
                                    match marker.as_ref() {
                                        SyncMarker::TrackBoundary { metadata } => {
                                            // Inject per-track metadata and duration into the next FLAC header.
                                            if let Err(e) =
                                                self.ctx.prepare_encoder_options_for_track(metadata).await
                                            {
                                                error!("Failed to prepare encoder options for new track: {}", e);
                                            }

                                            if self.ctx.restart_encoder_on_track_boundary {
                                                // Only restart encoder if it's already initialized (not the first track)
                                                if self.ctx.sample_rate.is_some()
                                                    && self.ctx.encoder_state.is_some()
                                                {
                                                    // Restart encoder to emit new OGG stream header and reset timestamps
                                                    if let Err(e) = self
                                                        .ctx
                                                        .restart_encoder_for_new_track(
                                                            |flac_stream,
                                                             broadcast,
                                                             header,
                                                             current_timestamp,
                                                             current_duration,
                                                             max_lead,
                                                             _sample_rate,
                                                             timestamp_offset_sec| {
                                                                broadcast_ogg_flac_stream(
                                                                    flac_stream,
                                                                    broadcast,
                                                                    header,
                                                                    current_timestamp,
                                                                    current_duration,
                                                                    max_lead,
                                                                    timestamp_offset_sec,
                                                                )
                                                            },
                                                        )
                                                        .await
                                                    {
                                                        error!("Failed to restart OGG encoder for new track: {}", e);
                                                        break;
                                                    }
                                                } else {
                                                    trace!("Skipping OGG encoder restart for first track (encoder not yet initialized)");
                                                }
                                            } else {
                                                trace!("StreamingOggFlacSink: restart disabled; continuing encoder across track boundary");
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

        debug!("StreamingOggFlacSink processing complete");
        Ok(())
    }

    async fn cleanup(&mut self, reason: StopReason) -> Result<(), AudioError> {
        debug!("StreamingOggFlacSink cleanup: {:?}", reason);
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
        Self::with_options(
            encoder_options,
            bits_per_sample,
            broadcast_max_lead_time,
            StreamingSinkOptions::ogg_defaults(),
        )
    }

    /// Create a sink with a custom broadcast pacing limit and options.
    pub fn with_options(
        mut encoder_options: EncoderOptions,
        bits_per_sample: u8,
        broadcast_max_lead_time: f64,
        options: StreamingSinkOptions,
    ) -> (Self, OggFlacStreamHandle) {
        // Validate bit depth
        if ![16, 24, 32].contains(&bits_per_sample) {
            panic!("bits_per_sample must be 16, 24, or 32");
        }

        // Transfer server_base_url from StreamingSinkOptions to EncoderOptions
        encoder_options.server_base_url = options.server_base_url.clone();

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
        let (broadcast, _) = timed_broadcast::channel("Ogg-Flac", broadcast_capacity);

        // OGG-FLAC header cache
        let header = Arc::new(RwLock::new(None));

        // Stop token and client control
        let stop_token = CancellationToken::new();
        let auto_stop = Arc::new(AtomicBool::new(true));

        let shared_handle = Arc::new(SharedStreamHandleInner::new(
            broadcast.clone(),
            metadata.clone(),
            stop_token.clone(),
            header.clone(),
            auto_stop.clone(),
        ));

        let handle = OggFlacStreamHandle::new(shared_handle.clone());

        let logic = StreamingOggFlacSinkLogic {
            ctx: SharedSinkContext {
                encoder_options,
                bits_per_sample,
                enable_total_samples: options.enable_total_samples,
                restart_encoder_on_track_boundary: options.restart_encoder_on_track_boundary,
                default_title: options.default_title.clone(),
                default_artist: options.default_artist.clone(),
                use_only_default_metadata: options.use_only_default_metadata,
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
                pending_track_duration: None,
                pending_total_samples: None,
            },
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

/// OGG wrapper + broadcaster task: reads FLAC bytes from encoder, wraps in OGG pages, and broadcasts.
/// Implements precise real-time pacing based on audio timestamps.
/// Ensures FLAC frames are only sent at frame boundaries to prevent sync errors in strict decoders like FFPlay.
async fn broadcast_ogg_flac_stream(
    mut flac_stream: FlacEncodedStream,
    broadcast_tx: timed_broadcast::Sender<Bytes>,
    header_cache: Arc<RwLock<Option<Bytes>>>,
    current_timestamp: Arc<RwLock<f64>>,
    current_duration: Arc<RwLock<f64>>,
    broadcast_max_lead_time: f64,
    timestamp_offset_sec: f64,
) -> Result<(), AudioError> {
    trace!(
        "Broadcaster task started with FLAC frame boundary detection (max_lead={:.3}s)",
        broadcast_max_lead_time
    );

    let stream_serial = rand::random::<u32>();
    let mut ogg_writer = OggPageWriter::new(stream_serial);

    let mut total_bytes = 0u64;
    let mut pacer = BroadcastPacer::new(broadcast_max_lead_time, "OGG");

    // Timing instrumentation for burst detection
    let mut last_broadcast_time = std::time::Instant::now();
    let mut broadcast_count = 0u64;
    let mut total_read_time = 0.0f64;
    let mut read_count = 0u64;

    // Step 1: Read FLAC header (fLaC + metadata blocks)
    let flac_header = read_flac_header(&mut flac_stream).await?;
    trace!("Read FLAC header: {} bytes", flac_header.len());

    // Extract sample rate from STREAMINFO for granule position calculation
    let sample_rate = extract_sample_rate_from_streaminfo(&flac_header)?;
    let sample_rate_f64 = sample_rate as f64;

    // Sample counter for calculating accurate timestamps (reset on new headers)
    let mut encoded_samples = 0u64;
    trace!("Extracted sample rate from STREAMINFO: {} Hz", sample_rate);

    // Step 2: Create OGG-FLAC identification packet (BOS)
    // Format according to https://xiph.org/flac/ogg_mapping.html
    let ogg_flac_id = create_ogg_flac_identification(&flac_header)?;
    trace!(
        "Created OGG-FLAC identification packet: {} bytes",
        ogg_flac_id.len()
    );

    let bos_page = ogg_writer.create_page(&ogg_flac_id, true, false, false);
    let bos_bytes = Bytes::from(bos_page);

    // Step 3: Create Vorbis Comment page (reuse FLAC metadata blocks when available)
    let vorbis_comment = extract_comment_packet_from_flac_header(&flac_header)
        .unwrap_or_else(create_empty_vorbis_comment);
    let comment_page = ogg_writer.create_page(&vorbis_comment, false, false, false);
    let comment_bytes = Bytes::from(comment_page);

    // Cache the header (BOS + Comment pages)
    let mut cached_header = Vec::new();
    cached_header.extend_from_slice(&bos_bytes);
    cached_header.extend_from_slice(&comment_bytes);
    *header_cache.write().await = Some(Bytes::from(cached_header));
    trace!(
        "OGG-FLAC header cached ({} bytes: BOS + Vorbis Comment)",
        bos_bytes.len() + comment_bytes.len()
    );

    // Broadcast header (BOS and comment are metadata, not audio, so duration=0.0)
    match broadcast_tx.send(bos_bytes.clone(), 0.0, 0.0).await {
        Ok(_) => {}
        Err(SendError::Expired(_)) => {
            trace!("Broadcast closed before sending BOS page (expired)");
            return Ok(());
        }
        Err(SendError::Closed(_)) => {
            trace!("No receivers for BOS page, terminating broadcast");
            return Ok(());
        }
    }
    total_bytes += comment_bytes.len() as u64;
    match broadcast_tx.send(comment_bytes.clone(), 0.0, 0.0).await {
        Ok(_) => {}
        Err(SendError::Expired(_)) => {
            trace!("Broadcast closed before sending comment page (expired)");
            return Ok(());
        }
        Err(SendError::Closed(_)) => {
            trace!("No receivers for comment page, terminating broadcast");
            return Ok(());
        }
    }

    // Step 4: Read FLAC stream and create OGG packets
    // Use larger read buffer (16KB) to reduce syscalls and accumulator for frame boundary detection
    // The accumulator is necessary to ensure we only send complete FLAC frames
    let mut read_buffer = vec![0u8; 16384];
    let mut accumulator = Vec::with_capacity(32768);

    loop {
        let read_start = std::time::Instant::now();
        match flac_stream.read(&mut read_buffer).await {
            Ok(0) => {
                // EOF - create final page with EOS flag and any remaining data
                if !accumulator.is_empty() {
                    let eos_page = ogg_writer.create_page(&accumulator, false, true, false);
                    let eos_bytes = Bytes::from(eos_page);
                    total_bytes += eos_bytes.len() as u64;
                    let eos_ts = *current_timestamp.read().await;
                    let eos_dur = *current_duration.read().await;
                    match broadcast_tx.send(eos_bytes.clone(), eos_ts, eos_dur).await {
                        Ok(_) => {}
                        Err(SendError::Expired(_)) => {
                            trace!("Broadcast closed before sending final EOS page (expired)");
                        }
                        Err(SendError::Closed(_)) => {
                            trace!("Broadcast closed before sending final EOS page");
                        }
                    }
                    trace!(
                        "Sent final EOS page with {} bytes of data",
                        accumulator.len()
                    );
                } else {
                    // Send empty EOS page (metadata page, duration=0.0)
                    let eos_page = ogg_writer.create_page(&[], false, true, false);
                    let eos_bytes = Bytes::from(eos_page);
                    total_bytes += eos_bytes.len() as u64;
                    let eos_ts = *current_timestamp.read().await;
                    match broadcast_tx.send(eos_bytes.clone(), eos_ts, 0.0).await {
                        Ok(_) => {}
                        Err(SendError::Expired(_)) => {
                            trace!("Broadcast closed before sending empty EOS page (expired)");
                        }
                        Err(SendError::Closed(_)) => {
                            trace!("Broadcast closed before sending empty EOS page");
                        }
                    }
                    trace!("Sent empty EOS page");
                }

                trace!("OGG-FLAC stream ended, total OGG bytes: {}", total_bytes);
                break;
            }
            Ok(n) => {
                let read_duration = read_start.elapsed().as_secs_f64();
                read_count += 1;
                total_read_time += read_duration;

                if read_duration > 0.01 {
                    trace!(
                        "OGG: flac_stream.read() took {:.3}s for {} bytes (avg: {:.3}s over {} reads)",
                        read_duration,
                        n,
                        total_read_time / read_count as f64,
                        read_count
                    );
                }

                // Append to accumulator
                accumulator.extend_from_slice(&read_buffer[..n]);

                trace!(
                    "OGG: accumulator now {} bytes after reading {} bytes",
                    accumulator.len(),
                    n
                );

                // Process complete FLAC frames one at a time
                // OGG-FLAC spec requires: "Each audio data packet contains one complete FLAC frame"
                loop {
                    // Find all complete frames in the accumulator
                    if accumulator.len() < 4 {
                        break; // Need at least 4 bytes for sync code check
                    }

                    // Find all sync positions with their sample counts
                    // Use CRC-8 validation to eliminate false positives
                    let mut sync_data = Vec::new();
                    for i in 0..accumulator.len() - 1 {
                        let byte1 = accumulator[i];
                        let byte2 = accumulator[i + 1];

                        if byte1 == 0xFF && byte2 >= 0xF8 && byte2 <= 0xFE {
                            // Validate frame header with CRC-8 to avoid false positives
                            if flac_frame_utils::validate_frame_header_crc(&accumulator, i) {
                                if let Some(samples) =
                                    flac_frame_utils::parse_flac_block_size(&accumulator, i)
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
                        accumulator.drain(0..first_frame_start);
                        continue;
                    }

                    // Extract just the first frame
                    let first_frame: Vec<u8> = accumulator.drain(0..second_frame_start).collect();

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

                    // Detect FLAC header "fLaC" in frame - indicates new track
                    if first_frame.len() >= 4 && &first_frame[0..4] == b"fLaC" {
                        // New track detected: reset sample counter
                        encoded_samples = 0;
                        trace!(
                            "New FLAC header detected in OGG stream ({} bytes), sample counter reset for new track",
                            first_frame.len()
                        );
                    }

                    // Calculer le timestamp de cette FLAC frame (avec offset pour continuité entre tracks)
                    let frame_start_samples = encoded_samples;
                    encoded_samples = encoded_samples.saturating_add(first_frame_samples as u64);
                    let audio_timestamp =
                        timestamp_offset_sec + (frame_start_samples as f64 / sample_rate_f64);
                    let segment_duration = first_frame_samples as f64 / sample_rate_f64;

                    // Check timing et apply pacing (skip si en retard)
                    if pacer.check_and_pace(audio_timestamp).await.is_err() {
                        continue; // Skip ce chunk (trop en retard)
                    }

                    // Update granule position (cumulative sample count)
                    ogg_writer.add_samples(first_frame_samples as u64);

                    // Wrap this single FLAC frame in ONE OGG page (per OGG-FLAC spec)
                    let ogg_page = ogg_writer.create_page(&first_frame, false, false, false);
                    let bytes = Bytes::from(ogg_page);
                    total_bytes += bytes.len() as u64;

                    // Measure broadcast interval for burst detection
                    let broadcast_interval = last_broadcast_time.elapsed().as_secs_f64();
                    last_broadcast_time = std::time::Instant::now();
                    broadcast_count += 1;

                    // Log if interval is unusual (too short = burst, too long = stall)
                    if broadcast_interval < 0.01 || broadcast_interval > 0.1 {
                        trace!(
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
                        trace!(
                            "OGG: {} broadcasts sent, avg_interval={:.3}s, accumulator={} bytes",
                            broadcast_count,
                            last_broadcast_time.elapsed().as_secs_f64() / broadcast_count as f64,
                            accumulator.len()
                        );
                    }

                    // Envoyer au broadcast
                    match broadcast_tx
                        .send(bytes.clone(), audio_timestamp, segment_duration)
                        .await
                    {
                        Ok(n) => {
                            trace!("Broadcasted OGG page with 1 FLAC frame ({} bytes), {} samples ({} bytes total with OGG overhead) to {} receivers (ts={:.3}s, dur={:.3}s)", first_frame.len(), first_frame_samples, bytes.len(), n, audio_timestamp, segment_duration);
                        }
                        Err(SendError::Expired(_)) => {
                            trace!(
                                "OGG-FLAC broadcast dropped expired page (ts={:.3}s, dur={:.3}s)",
                                audio_timestamp,
                                segment_duration
                            );
                            continue;
                        }
                        Err(SendError::Closed(_)) => {
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

    trace!("Broadcaster task completed successfully");
    Ok(())
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

    trace!("STREAMINFO block_length = {} bytes", block_length);

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

    trace!(
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

/// Extract the concatenated FLAC metadata blocks after STREAMINFO to use as the OGG comment packet.
/// Returns `None` if the FLAC header only contains STREAMINFO.
fn extract_comment_packet_from_flac_header(flac_header: &[u8]) -> Option<Vec<u8>> {
    if flac_header.len() < 8 {
        return None;
    }

    // STREAMINFO block length is stored in bytes 5-7 (after type byte at 4)
    let block_length =
        u32::from_be_bytes([0, flac_header[5], flac_header[6], flac_header[7]]) as usize;
    let streaminfo_total = 4 + block_length; // block header + data

    // Skip "fLaC" + STREAMINFO block.
    let offset = 4 + streaminfo_total;
    if flac_header.len() <= offset {
        return None;
    }

    Some(flac_header[offset..].to_vec())
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
