//! DirectOggFlacSink — nœud puits OGG-FLAC pour un seul client HTTP.
//!
//! # Architecture
//!
//! ```text
//! AudioSegment I24 @ 96 kHz
//!     ↓ NodeLogic::process()
//! chunk_to_pcm_bytes() → PCM 24-bit LE
//!     ↓ pcm_tx (cap=2)
//! ByteStreamReader → encode_flac_stream() → OGG pages (Bytes)
//!     ↓ ogg_tx  mpsc::Sender<Bytes>  (cap=8, backpressure naturelle)
//! Arc<Mutex<Receiver<Bytes>>> → DirectOggFlacStream (AsyncRead) → HTTP → Safari
//! ```
//!
//! # Backpressure
//!
//! Safari lent → ogg_rx plein → ogg_tx.send() bloque → encodeur bloque
//! → pcm_tx.send() bloque → pipeline audio bloque.
//!
//! # OGG chaining (TrackBoundary)
//!
//! Fermer pcm_tx → encodeur termine (EOS) → lancer nouvel encodeur.
//! Le même ogg_tx est réutilisé : le stream HTTP est continu.

use std::collections::VecDeque;
use std::io;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use async_trait::async_trait;
use bytes::Bytes;
use pmoaudio::{
    pipeline::{AudioPipelineNode, Node, NodeLogic, PipelineHandle, StopReason},
    AudioError, AudioSegment, SyncMarker, TypeRequirement, TypedAudioNode, _AudioSegment,
};
use pmoflac::{EncoderOptions, PcmFormat};
use tokio::io::{AsyncRead, ReadBuf};
use tokio::sync::{mpsc, watch, Mutex};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::{debug, trace, warn};

use crate::sinks::byte_stream_reader::{ByteStreamReader, PcmChunk};
use crate::sinks::chunk_to_pcm::chunk_to_pcm_bytes;
use crate::sinks::flac_frame_utils::{extract_sample_rate_from_streaminfo, read_flac_header};

/// Format de sortie fixe du sink.
pub const DIRECT_OGG_FLAC_SAMPLE_RATE: u32 = 96_000;
pub const DIRECT_OGG_FLAC_CHANNELS: u8 = 2;
pub const DIRECT_OGG_FLAC_BITS_PER_SAMPLE: u8 = 24;

/// Capacité du canal OGG → HTTP.
/// Capacité 1 : backpressure stricte, l'encodeur ne produit pas en avance.
const OGG_CHANNEL_CAPACITY: usize = 1;

// ─── Types partagés ───────────────────────────────────────────────────────────

type SharedPcmTx = Arc<Mutex<Option<mpsc::Sender<PcmChunk>>>>;
type SharedEncoderTask = Arc<Mutex<Option<JoinHandle<()>>>>;
type SharedOggRx = Arc<Mutex<mpsc::Receiver<Bytes>>>;

// ─── Handle public ────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct DirectOggFlacHandle {
    pcm_tx: SharedPcmTx,
    encoder_task: SharedEncoderTask,
    encoder_options: EncoderOptions,
    current_timestamp: Arc<tokio::sync::RwLock<f64>>,
    /// Canal OGG partagé avec le stream HTTP.
    ogg_tx: mpsc::Sender<Bytes>,
    ogg_rx: SharedOggRx,
    /// Notifié quand get_stream() est appelé (premier client).
    client_notify: Arc<tokio::sync::Notify>,
}

impl DirectOggFlacHandle {
    /// Retourne le stream OGG-FLAC pour le handler HTTP.
    /// Lance le premier encodeur au premier appel.
    pub fn get_stream(&self) -> DirectOggFlacStream {
        tracing::info!(target: "pmoaudio_ext::stream_connect", "get_stream() called — new HTTP client connecting");
        self.client_notify.notify_one();
        DirectOggFlacStream {
            ogg_rx: self.ogg_rx.clone(),
            buffer: VecDeque::new(),
        }
    }

    pub async fn current_position_sec(&self) -> f64 {
        *self.current_timestamp.read().await
    }
}

// ─── Stream public ────────────────────────────────────────────────────────────

/// AsyncRead sur le canal OGG-FLAC.
pub struct DirectOggFlacStream {
    ogg_rx: SharedOggRx,
    buffer: VecDeque<u8>,
}

impl Drop for DirectOggFlacStream {
    fn drop(&mut self) {
        tracing::info!(target: "pmoaudio_ext::stream_connect", "DirectOggFlacStream dropped — HTTP client disconnected");
    }
}

impl AsyncRead for DirectOggFlacStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        // Vider le buffer interne en premier
        if !self.buffer.is_empty() {
            let to_copy = self.buffer.len().min(buf.remaining());
            let chunk: Vec<u8> = self.buffer.drain(..to_copy).collect();
            buf.put_slice(&chunk);
            return Poll::Ready(Ok(()));
        }

        // Tenter de recevoir le prochain chunk OGG
        let mut guard = match self.ogg_rx.try_lock() {
            Ok(g) => g,
            Err(_) => {
                tracing::warn!(target: "pmoaudio_ext::stream_connect", "DirectOggFlacStream: try_lock() FAILED — concurrent access detected!");
                cx.waker().wake_by_ref();
                return Poll::Pending;
            }
        };

        match guard.poll_recv(cx) {
            Poll::Ready(Some(bytes)) => {
                drop(guard);
                tracing::info!(target: "pmoaudio_ext::stream_connect", "DirectOggFlacStream: sending {} bytes to HTTP client", bytes.len());
                let to_copy = bytes.len().min(buf.remaining());
                buf.put_slice(&bytes[..to_copy]);
                if to_copy < bytes.len() {
                    self.buffer.extend(&bytes[to_copy..]);
                }
                Poll::Ready(Ok(()))
            }
            Poll::Ready(None) => {
                tracing::info!(target: "pmoaudio_ext::stream_connect", "DirectOggFlacStream: ogg_rx closed → EOF sent to HTTP client");
                Poll::Ready(Ok(()))
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

// ─── Logique du nœud ─────────────────────────────────────────────────────────

struct DirectOggFlacSinkLogic {
    pcm_tx: SharedPcmTx,
    encoder_task: SharedEncoderTask,
    encoder_options: EncoderOptions,
    current_timestamp: Arc<tokio::sync::RwLock<f64>>,
    ogg_tx: mpsc::Sender<Bytes>,
    client_notify: Arc<tokio::sync::Notify>,
    has_encoded_frames: bool,
}

#[async_trait]
impl NodeLogic for DirectOggFlacSinkLogic {
    async fn process(
        &mut self,
        input: Option<mpsc::Receiver<Arc<AudioSegment>>>,
        _output: Vec<mpsc::Sender<Arc<AudioSegment>>>,
        stop_token: CancellationToken,
    ) -> Result<(), AudioError> {
        let mut input = input.ok_or_else(|| {
            AudioError::ProcessingError("DirectOggFlacSink requires an input".into())
        })?;

        // Attendre le premier client Safari avant de démarrer l'encodeur
        debug!("DirectOggFlacSink: waiting for first client...");
        tokio::select! {
            _ = stop_token.cancelled() => return Ok(()),
            _ = self.client_notify.notified() => {}
        }
        debug!("DirectOggFlacSink: first client connected, starting encoder");
        self.start_encoder().await;

        loop {
            tokio::select! {
                _ = stop_token.cancelled() => {
                    debug!("DirectOggFlacSink: cancelled");
                    break;
                }

                segment = input.recv() => {
                    match segment {
                        None => {
                            debug!("DirectOggFlacSink: input channel closed");
                            break;
                        }
                        Some(seg) => match &seg.segment {
                            _AudioSegment::Chunk(chunk) => {
                                let tx = self.pcm_tx.lock().await.clone();
                                let Some(tx) = tx else { continue; };

                                let pcm_bytes = chunk_to_pcm_bytes(chunk, DIRECT_OGG_FLAC_BITS_PER_SAMPLE)?;
                                let duration_sec = chunk.len() as f64 / DIRECT_OGG_FLAC_SAMPLE_RATE as f64;
                                let pcm_chunk = PcmChunk {
                                    bytes: pcm_bytes,
                                    timestamp_sec: seg.timestamp_sec,
                                    duration_sec,
                                };
                                if tx.send(pcm_chunk).await.is_err() {
                                    warn!("DirectOggFlacSink: encoder gone at {:.3}s", seg.timestamp_sec);
                                    *self.pcm_tx.lock().await = None;
                                    self.has_encoded_frames = false;
                                } else {
                                    self.has_encoded_frames = true;
                                }
                            }
                            _AudioSegment::Sync(marker) => match marker.as_ref() {
                                SyncMarker::TrackBoundary { .. } => {
                                    if self.has_encoded_frames {
                                        debug!("DirectOggFlacSink: TrackBoundary — OGG chaining");
                                        self.has_encoded_frames = false;
                                        self.do_track_boundary().await;
                                    } else {
                                        debug!("DirectOggFlacSink: TrackBoundary ignored (no frames)");
                                    }
                                }
                                SyncMarker::EndOfStream => {
                                    debug!("DirectOggFlacSink: EndOfStream");
                                    self.stop_encoder().await;
                                }
                                _ => {}
                            },
                        },
                    }
                }
            }
        }

        self.stop_encoder().await;
        Ok(())
    }

    async fn cleanup(&mut self, _reason: StopReason) -> Result<(), AudioError> {
        Ok(())
    }
}

impl DirectOggFlacSinkLogic {
    async fn start_encoder(&mut self) {
        let (pcm_tx, pcm_rx) = mpsc::channel::<PcmChunk>(2);
        let current_dur = Arc::new(tokio::sync::RwLock::new(0.0f64));
        let pcm_reader = ByteStreamReader::new(pcm_rx, self.current_timestamp.clone(), current_dur);
        *self.pcm_tx.lock().await = Some(pcm_tx);

        let ogg_tx = self.ogg_tx.clone();
        let options = self.encoder_options.clone();
        let current_timestamp = self.current_timestamp.clone();
        let handle = tokio::spawn(async move {
            debug!("DirectOggFlacSink: encoder task started");
            if let Err(e) = run_ogg_encoder(pcm_reader, ogg_tx, options, current_timestamp).await {
                debug!("DirectOggFlacSink: encoder stopped: {}", e);
            }
            debug!("DirectOggFlacSink: encoder task ended");
        });
        *self.encoder_task.lock().await = Some(handle);
    }

    async fn stop_encoder(&mut self) {
        *self.pcm_tx.lock().await = None;
        if let Some(handle) = self.encoder_task.lock().await.take() {
            let _ = handle.await;
        }
        self.has_encoded_frames = false;
    }

    async fn do_track_boundary(&mut self) {
        // Fermer pcm_tx → encodeur écrit EOS, se termine
        *self.pcm_tx.lock().await = None;
        if let Some(handle) = self.encoder_task.lock().await.take() {
            let _ = handle.await;
            debug!("DirectOggFlacSink: previous encoder joined");
        }
        // Démarrer le nouvel encodeur sur le même ogg_tx
        self.start_encoder().await;
        debug!("DirectOggFlacSink: OGG chaining complete");
    }
}

// ─── Encodeur FLAC + wrapper OGG ─────────────────────────────────────────────

async fn run_ogg_encoder(
    pcm_reader: ByteStreamReader,
    ogg_tx: mpsc::Sender<Bytes>,
    options: EncoderOptions,
    _current_timestamp: Arc<tokio::sync::RwLock<f64>>,
) -> Result<(), AudioError> {
    use tokio::io::AsyncReadExt;

    let format = PcmFormat {
        sample_rate: DIRECT_OGG_FLAC_SAMPLE_RATE,
        channels: DIRECT_OGG_FLAC_CHANNELS,
        bits_per_sample: DIRECT_OGG_FLAC_BITS_PER_SAMPLE,
    };

    let mut flac_stream = pmoflac::encode_flac_stream(pcm_reader, format, options)
        .await
        .map_err(|e| AudioError::ProcessingError(format!("FLAC encoder init: {}", e)))?;

    let flac_header = read_flac_header(&mut flac_stream).await?;
    let _sample_rate = extract_sample_rate_from_streaminfo(&flac_header)?;

    let stream_serial: u32 = rand::random();
    let mut ogg = OggPageWriter::new(stream_serial);

    let ogg_flac_id = create_ogg_flac_identification(&flac_header)?;
    let bos_page = ogg.create_page(&ogg_flac_id, true, false, false);
    let vorbis_comment = create_empty_vorbis_comment();
    let comment_page = ogg.create_page(&vorbis_comment, false, false, false);

    let mut header = Vec::new();
    header.extend_from_slice(&bos_page);
    header.extend_from_slice(&comment_page);
    if ogg_tx.send(Bytes::from(header)).await.is_err() {
        debug!("DirectOggFlacSink: ogg_tx closed on headers");
        return Ok(());
    }

    use crate::sinks::flac_frame_utils::{validate_frame_header_crc, parse_flac_block_size};

    let mut encoded_samples = 0u64;
    let mut read_buffer = vec![0u8; 65536];
    let mut accumulator: Vec<u8> = Vec::with_capacity(32768);

    loop {
        match flac_stream.read(&mut read_buffer).await {
            Ok(0) => {
                trace!(
                    "OggEncoder: EOF — accum={} B, encoded={:.3}s",
                    accumulator.len(),
                    encoded_samples as f64 / DIRECT_OGG_FLAC_SAMPLE_RATE as f64,
                );
                let eos_page = ogg.create_page(&accumulator, false, true, false);
                let _ = ogg_tx.send(Bytes::from(eos_page)).await;
                break;
            }
            Ok(n) => {
                accumulator.extend_from_slice(&read_buffer[..n]);

                loop {
                    if accumulator.len() < 4 { break; }

                    let mut sync_data: Vec<(usize, u32)> = Vec::new();
                    for i in 0..accumulator.len() - 1 {
                        let b1 = accumulator[i];
                        let b2 = accumulator[i + 1];
                        if b1 == 0xFF && b2 >= 0xF8 && b2 <= 0xFE {
                            if validate_frame_header_crc(&accumulator, i) {
                                if let Some(samples) = parse_flac_block_size(&accumulator, i) {
                                    sync_data.push((i, samples));
                                }
                            }
                        }
                    }

                    if sync_data.len() < 2 { break; }

                    let first_start = sync_data[0].0;
                    let first_samples = sync_data[0].1;
                    let second_start = sync_data[1].0;

                    if first_start != 0 {
                        accumulator.drain(0..first_start);
                        continue;
                    }

                    let frame: Vec<u8> = accumulator.drain(0..second_start).collect();
                    encoded_samples = encoded_samples.saturating_add(first_samples as u64);
                    ogg.add_samples(first_samples as u64);

                    trace!(
                        "OggEncoder: frame {} B, {:.3}s total",
                        frame.len(),
                        encoded_samples as f64 / DIRECT_OGG_FLAC_SAMPLE_RATE as f64,
                    );

                    let ogg_page = ogg.create_page(&frame, false, false, false);
                    if ogg_tx.send(Bytes::from(ogg_page)).await.is_err() {
                        warn!("DirectOggFlacSink: ogg_tx closed after {:.3}s",
                            encoded_samples as f64 / DIRECT_OGG_FLAC_SAMPLE_RATE as f64);
                        return Ok(());
                    }
                }
            }
            Err(e) => {
                return Err(AudioError::ProcessingError(format!("FLAC read: {}", e)));
            }
        }
    }

    flac_stream.wait().await
        .map_err(|e| AudioError::ProcessingError(format!("FLAC encoder wait: {}", e)))?;

    Ok(())
}

// ─── OGG helpers ─────────────────────────────────────────────────────────────

struct OggPageWriter {
    stream_serial: u32,
    page_sequence: u32,
    granule_position: u64,
}

impl OggPageWriter {
    fn new(stream_serial: u32) -> Self {
        Self { stream_serial, page_sequence: 0, granule_position: 0 }
    }

    fn add_samples(&mut self, samples: u64) {
        self.granule_position += samples;
    }

    fn create_page(&mut self, packet_data: &[u8], is_bos: bool, is_eos: bool, is_continuation: bool) -> Vec<u8> {
        use std::io::Write;

        let mut segments = Vec::new();
        let mut remaining = packet_data.len();
        while remaining > 0 {
            let seg = remaining.min(255);
            segments.push(seg as u8);
            remaining -= seg;
        }
        if !packet_data.is_empty() && packet_data.len() % 255 == 0 && !is_continuation {
            segments.push(0);
        }

        let segment_count = segments.len();
        let total_size = 27 + segment_count + packet_data.len();
        let mut page = Vec::with_capacity(total_size);

        page.write_all(b"OggS").unwrap();
        page.write_all(&[0]).unwrap();

        let mut header_type = 0u8;
        if is_continuation { header_type |= 0x01; }
        if is_bos         { header_type |= 0x02; }
        if is_eos         { header_type |= 0x04; }
        page.write_all(&[header_type]).unwrap();

        page.write_all(&self.granule_position.to_le_bytes()).unwrap();
        page.write_all(&self.stream_serial.to_le_bytes()).unwrap();
        page.write_all(&self.page_sequence.to_le_bytes()).unwrap();
        self.page_sequence += 1;

        let crc_offset = page.len();
        page.write_all(&[0, 0, 0, 0]).unwrap();
        page.write_all(&[segment_count as u8]).unwrap();
        page.write_all(&segments).unwrap();
        page.write_all(packet_data).unwrap();

        let crc = calculate_ogg_crc(&page);
        page[crc_offset..crc_offset + 4].copy_from_slice(&crc.to_le_bytes());

        page
    }
}

fn calculate_ogg_crc(data: &[u8]) -> u32 {
    const CRC_TABLE: [u32; 256] = generate_crc_table();
    let mut crc: u32 = 0;
    for &byte in data {
        crc = (crc << 8) ^ CRC_TABLE[((crc >> 24) ^ (byte as u32)) as usize];
    }
    crc
}

const fn generate_crc_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    let mut i = 0usize;
    while i < 256 {
        let mut r = (i as u32) << 24;
        let mut j = 0;
        while j < 8 {
            if (r & 0x80000000) != 0 { r = (r << 1) ^ 0x04c11db7; } else { r <<= 1; }
            j += 1;
        }
        table[i] = r;
        i += 1;
    }
    table
}

fn create_ogg_flac_identification(flac_header: &[u8]) -> Result<Vec<u8>, AudioError> {
    if flac_header.len() < 8 || &flac_header[0..4] != b"fLaC" {
        return Err(AudioError::ProcessingError("Invalid FLAC header".into()));
    }
    let first_block_type = flac_header[4] & 0x7F;
    if first_block_type != 0 {
        return Err(AudioError::ProcessingError("First FLAC block is not STREAMINFO".into()));
    }
    let block_length = u32::from_be_bytes([0, flac_header[5], flac_header[6], flac_header[7]]) as usize;
    let streaminfo_size = 4 + block_length;
    if flac_header.len() < 4 + streaminfo_size {
        return Err(AudioError::ProcessingError("FLAC header truncated".into()));
    }
    let streaminfo = &flac_header[4..4 + streaminfo_size];

    let mut packet = Vec::new();
    packet.push(0x7F);
    packet.extend_from_slice(b"FLAC");
    packet.push(0x01);
    packet.push(0x00);
    packet.extend_from_slice(&1u16.to_be_bytes());
    packet.extend_from_slice(b"fLaC");
    packet.extend_from_slice(streaminfo);
    Ok(packet)
}

fn create_empty_vorbis_comment() -> Vec<u8> {
    let vendor = "pmoaudio DirectOggFlacSink";
    let vendor_bytes = vendor.as_bytes();
    let mut vorbis_data = Vec::new();
    vorbis_data.extend_from_slice(&(vendor_bytes.len() as u32).to_le_bytes());
    vorbis_data.extend_from_slice(vendor_bytes);
    vorbis_data.extend_from_slice(&0u32.to_le_bytes());

    let mut block = Vec::new();
    block.push(0x84); // last-block + VORBIS_COMMENT type
    let length = vorbis_data.len() as u32;
    block.push((length >> 16) as u8);
    block.push((length >> 8) as u8);
    block.push(length as u8);
    block.extend_from_slice(&vorbis_data);
    block
}

// ─── Nœud public ─────────────────────────────────────────────────────────────

pub struct DirectOggFlacSink {
    inner: Node<DirectOggFlacSinkLogic>,
}

impl DirectOggFlacSink {
    pub fn new(encoder_options: EncoderOptions) -> (Self, DirectOggFlacHandle) {
        let pcm_tx: SharedPcmTx = Arc::new(Mutex::new(None));
        let encoder_task: SharedEncoderTask = Arc::new(Mutex::new(None));
        let current_timestamp = Arc::new(tokio::sync::RwLock::new(0.0f64));
        let client_notify = Arc::new(tokio::sync::Notify::new());

        let (ogg_tx, ogg_rx) = mpsc::channel::<Bytes>(OGG_CHANNEL_CAPACITY);
        let ogg_rx = Arc::new(Mutex::new(ogg_rx));

        let logic = DirectOggFlacSinkLogic {
            pcm_tx: pcm_tx.clone(),
            encoder_task: encoder_task.clone(),
            encoder_options: encoder_options.clone(),
            current_timestamp: current_timestamp.clone(),
            ogg_tx: ogg_tx.clone(),
            client_notify: client_notify.clone(),
            has_encoded_frames: false,
        };

        let sink = Self {
            inner: Node::new_with_input(logic, 16),
        };

        let handle = DirectOggFlacHandle {
            pcm_tx,
            encoder_task,
            encoder_options,
            current_timestamp,
            ogg_tx,
            ogg_rx,
            client_notify,
        };

        (sink, handle)
    }
}

#[async_trait]
impl AudioPipelineNode for DirectOggFlacSink {
    fn get_tx(&self) -> Option<mpsc::Sender<Arc<AudioSegment>>> {
        self.inner.get_tx()
    }

    fn register(&mut self, _child: Box<dyn AudioPipelineNode>) {
        panic!("DirectOggFlacSink is a terminal sink and cannot have children");
    }

    async fn run(self: Box<Self>, stop_token: CancellationToken) -> Result<(), AudioError> {
        Box::new(self.inner).run(stop_token).await
    }

    fn start(self: Box<Self>) -> PipelineHandle {
        Box::new(self.inner).start()
    }
}

impl TypedAudioNode for DirectOggFlacSink {
    fn input_type(&self) -> Option<TypeRequirement> {
        Some(TypeRequirement::any_integer())
    }

    fn output_type(&self) -> Option<TypeRequirement> {
        None
    }
}
