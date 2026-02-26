//! DirectOggFlacSink — nœud puits OGG-FLAC pour un seul client HTTP.
//!
//! Combine la logique de backpressure/reconnexion de `DirectFlacSink`
//! avec l'encodage OGG-FLAC de `StreamingOggFlacSink`.
//!
//! # Cycle de vie
//!
//! - **Play** : le navigateur appelle `GET /stream`. `connect()` crée un nouveau
//!   canal PCM + pipe duplex + encodeur FLAC + wrapper OGG, installe le sender
//!   dans le sink, et notifie le sink via `client_notify`. Le flux reste ouvert :
//!   les morceaux s'enchaînent en gapless.
//! - **Stop** : le navigateur ferme la connexion. Le pipe se rompt, l'encodeur
//!   s'arrête. Le sink voit `pcm_tx.send()` échouer, passe le sender à `None`,
//!   et **bloque** sur `client_notify` jusqu'au prochain Play.
//! - **Play suivant** : `connect()` → nouveau pipe → `client_notify.notify_one()`
//!   → le sink se débloque et reprend la consommation des segments.
//!
//! # Architecture
//!
//! ```text
//! AudioSegment I24 @ 96 kHz
//!     ↓ NodeLogic::process()  [bloque si pas de client]
//! chunk_to_pcm_bytes() → PCM 24-bit LE
//!     ↓ Arc<Mutex<Option<mpsc::Sender<PcmChunk>>>>
//! ByteStreamReader (AsyncRead)
//!     ↓ encode_flac_stream()
//!     ↓ broadcast_ogg_flac_stream() → wrapping OGG pages
//!     ↓ tokio::io::duplex pipe (256 KB)
//!     ↓ DirectOggFlacStream (AsyncRead) → Body HTTP
//! ```

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
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};

use crate::sinks::byte_stream_reader::{ByteStreamReader, PcmChunk};
use crate::sinks::chunk_to_pcm::chunk_to_pcm_bytes;
use crate::sinks::flac_frame_utils::{extract_sample_rate_from_streaminfo, read_flac_header};

/// Format de sortie fixe du sink.
pub const DIRECT_OGG_FLAC_SAMPLE_RATE: u32 = 96_000;
pub const DIRECT_OGG_FLAC_CHANNELS: u8 = 2;
pub const DIRECT_OGG_FLAC_BITS_PER_SAMPLE: u8 = 24;

/// Capacité du pipe duplex (~256 KB).
const PIPE_CAPACITY: usize = 256 * 1024;

// ─── Shared state ─────────────────────────────────────────────────────────────

type SharedPcmTx = Arc<Mutex<Option<mpsc::Sender<PcmChunk>>>>;

// ─── Handle public ────────────────────────────────────────────────────────────

/// Handle vers le sink, cloneable, reconnectable à chaque Play.
#[derive(Clone)]
pub struct DirectOggFlacHandle {
    pcm_tx: SharedPcmTx,
    client_connect_tx: Arc<watch::Sender<u64>>,
    client_notify_internal: Arc<tokio::sync::Notify>,
    first_byte_tx: Arc<watch::Sender<bool>>,
    encoder_options: EncoderOptions,
    /// Position de lecture courante (mise à jour par ByteStreamReader).
    current_timestamp: Arc<tokio::sync::RwLock<f64>>,
}

impl DirectOggFlacHandle {
    /// Crée un nouveau pipe OGG-FLAC et retourne le flux côté lecture.
    /// Débloque le sink s'il attendait un client.
    pub async fn connect(&self) -> DirectOggFlacStream {
        let connect_count_before = *self.client_connect_tx.borrow();
        debug!("DirectOggFlacHandle::connect() called, connect_count={}", connect_count_before);

        let (pcm_tx, pcm_rx) = mpsc::channel::<PcmChunk>(8);
        // Réinitialiser le timestamp à 0 pour la nouvelle connexion
        *self.current_timestamp.write().await = 0.0;
        let current_dur = Arc::new(tokio::sync::RwLock::new(0.0f64));
        // Partager current_timestamp avec ByteStreamReader : il sera mis à jour
        // avec le timestamp absolu du segment audio (position dans le fichier source).
        let pcm_reader = ByteStreamReader::new(pcm_rx, self.current_timestamp.clone(), current_dur);

        let (pipe_writer, pipe_reader) = tokio::io::duplex(PIPE_CAPACITY);

        let _ = self.first_byte_tx.send(false);
        debug!("DirectOggFlacHandle::connect() first_byte reset to false");

        *self.pcm_tx.lock().await = Some(pcm_tx);
        debug!("DirectOggFlacHandle::connect() pcm_tx installed");

        let new_count = connect_count_before.wrapping_add(1);
        let _ = self.client_connect_tx.send(new_count);
        debug!("DirectOggFlacHandle::connect() client_connect_count -> {}", new_count);
        self.client_notify_internal.notify_one();

        let options = self.encoder_options.clone();
        let current_timestamp = self.current_timestamp.clone();
        tokio::spawn(async move {
            debug!("DirectOggFlacHandle: encoder+ogg task started");
            if let Err(e) = run_ogg_encoder(pcm_reader, pipe_writer, options, current_timestamp).await {
                debug!("DirectOggFlacStream encoder stopped: {}", e);
            }
            debug!("DirectOggFlacHandle: encoder+ogg task ended");
        });

        debug!("DirectOggFlacHandle::connect() returning DirectOggFlacStream");
        DirectOggFlacStream {
            inner: pipe_reader,
            first_byte_tx: Some(self.first_byte_tx.clone()),
        }
    }

    pub fn first_byte_ready(&self) -> watch::Receiver<bool> {
        self.first_byte_tx.subscribe()
    }

    /// Retourne la position de lecture courante en secondes.
    pub async fn current_position_sec(&self) -> f64 {
        *self.current_timestamp.read().await
    }

    pub async fn wait_for_client(&self) {
        let seen = *self.client_connect_tx.borrow();
        debug!("DirectOggFlacHandle::wait_for_client() called, seen connect_count={}", seen);
        let mut rx = self.client_connect_tx.subscribe();
        let _ = rx.wait_for(|v| *v > seen).await;
        debug!("DirectOggFlacHandle::wait_for_client() unblocked");
    }
}

// ─── Stream public ────────────────────────────────────────────────────────────

pub struct DirectOggFlacStream {
    inner: tokio::io::DuplexStream,
    first_byte_tx: Option<Arc<watch::Sender<bool>>>,
}

impl AsyncRead for DirectOggFlacStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let filled_before = buf.filled().len();
        let result = Pin::new(&mut self.inner).poll_read(cx, buf);
        if let Poll::Ready(Ok(())) = &result {
            let filled_after = buf.filled().len();
            if filled_after > filled_before {
                if let Some(tx) = self.first_byte_tx.take() {
                    debug!("DirectOggFlacStream: first {} bytes sent to HTTP client", filled_after - filled_before);
                    let _ = tx.send(true);
                }
            }
        }
        result
    }
}

// ─── Logique du nœud ─────────────────────────────────────────────────────────

struct DirectOggFlacSinkLogic {
    pcm_tx: SharedPcmTx,
    client_notify: Arc<tokio::sync::Notify>,
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
                                // Attendre un client si nécessaire (backpressure quand pas de Play)
                                loop {
                                    let tx_opt = self.pcm_tx.lock().await.clone();
                                    if tx_opt.is_some() {
                                        break;
                                    }
                                    debug!("DirectOggFlacSink: no pcm_tx, waiting for client_notify...");
                                    tokio::select! {
                                        _ = stop_token.cancelled() => {
                                            debug!("DirectOggFlacSink: cancelled while waiting for client");
                                            return Ok(());
                                        }
                                        _ = self.client_notify.notified() => {
                                            debug!("DirectOggFlacSink: client_notify received, rechecking pcm_tx");
                                        }
                                    }
                                }

                                let tx = self.pcm_tx.lock().await.clone().unwrap();
                                let pcm_bytes = chunk_to_pcm_bytes(chunk, DIRECT_OGG_FLAC_BITS_PER_SAMPLE)?;
                                let duration_sec = chunk.len() as f64 / DIRECT_OGG_FLAC_SAMPLE_RATE as f64;
                                let pcm_chunk = PcmChunk {
                                    bytes: pcm_bytes,
                                    timestamp_sec: seg.timestamp_sec,
                                    duration_sec,
                                };
                                if tx.send(pcm_chunk).await.is_err() {
                                    warn!(
                                        ts = seg.timestamp_sec,
                                        "DirectOggFlacSink: chunk dropped (client disconnected at {:.3}s), waiting for reconnect",
                                        seg.timestamp_sec,
                                    );
                                    *self.pcm_tx.lock().await = None;
                                }
                            }
                            _AudioSegment::Sync(marker) => match marker.as_ref() {
                                SyncMarker::EndOfStream => {
                                    debug!("DirectOggFlacSink: EndOfStream");
                                }
                                _ => {}
                            },
                        },
                    }
                }
            }
        }

        Ok(())
    }

    async fn cleanup(&mut self, _reason: StopReason) -> Result<(), AudioError> {
        Ok(())
    }
}

// ─── Encodeur FLAC + wrapper OGG ─────────────────────────────────────────────

async fn run_ogg_encoder(
    pcm_reader: ByteStreamReader,
    mut pipe_writer: tokio::io::DuplexStream,
    options: EncoderOptions,
    _current_timestamp: Arc<tokio::sync::RwLock<f64>>,
) -> Result<(), AudioError> {
    let format = PcmFormat {
        sample_rate: DIRECT_OGG_FLAC_SAMPLE_RATE,
        channels: DIRECT_OGG_FLAC_CHANNELS,
        bits_per_sample: DIRECT_OGG_FLAC_BITS_PER_SAMPLE,
    };

    let mut flac_stream = pmoflac::encode_flac_stream(pcm_reader, format, options)
        .await
        .map_err(|e| AudioError::ProcessingError(format!("FLAC encoder init: {}", e)))?;

    // Lire le header FLAC et construire les pages OGG d'en-tête
    let flac_header = read_flac_header(&mut flac_stream).await?;
    let sample_rate = extract_sample_rate_from_streaminfo(&flac_header)?;

    let stream_serial: u32 = rand::random();
    let mut ogg = OggPageWriter::new(stream_serial);

    // Page BOS (identification OGG-FLAC)
    let ogg_flac_id = create_ogg_flac_identification(&flac_header)?;
    let bos_page = Bytes::from(ogg.create_page(&ogg_flac_id, true, false, false));

    // Page Vorbis Comment
    let vorbis_comment = create_empty_vorbis_comment();
    let comment_page = Bytes::from(ogg.create_page(&vorbis_comment, false, false, false));

    pipe_writer.write_all(&bos_page).await
        .map_err(|e| AudioError::IoError(format!("OGG BOS write: {}", e)))?;
    pipe_writer.write_all(&comment_page).await
        .map_err(|e| AudioError::IoError(format!("OGG comment write: {}", e)))?;

    // Lire les frames FLAC et les encapsuler dans des pages OGG
    let sample_rate_f64 = sample_rate as f64;
    let mut encoded_samples = 0u64;
    let mut read_buffer = vec![0u8; 16384];
    let mut accumulator: Vec<u8> = Vec::with_capacity(32768);

    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    loop {
        match flac_stream.read(&mut read_buffer).await {
            Ok(0) => {
                // EOF : page EOS finale
                let eos_page = Bytes::from(ogg.create_page(&accumulator, false, true, false));
                let _ = pipe_writer.write_all(&eos_page).await;
                break;
            }
            Ok(n) => {
                accumulator.extend_from_slice(&read_buffer[..n]);

                loop {
                    if accumulator.len() < 4 {
                        break;
                    }

                    // Trouver les positions de sync FLAC
                    let mut sync_data: Vec<(usize, u32)> = Vec::new();
                    for i in 0..accumulator.len() - 1 {
                        let b1 = accumulator[i];
                        let b2 = accumulator[i + 1];
                        if b1 == 0xFF && b2 >= 0xF8 && b2 <= 0xFE {
                            use crate::sinks::flac_frame_utils::{validate_frame_header_crc, parse_flac_block_size};
                            if validate_frame_header_crc(&accumulator, i) {
                                if let Some(samples) = parse_flac_block_size(&accumulator, i) {
                                    sync_data.push((i, samples));
                                }
                            }
                        }
                    }

                    if sync_data.len() < 2 {
                        break;
                    }

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

                    let ogg_page = Bytes::from(ogg.create_page(&frame, false, false, false));
                    if pipe_writer.write_all(&ogg_page).await.is_err() {
                        // Client déconnecté — le pipe HTTP s'est rompu
                        warn!(
                            samples = encoded_samples,
                            "DirectOggFlacSink: OGG pipe broken after {} samples ({:.3}s), client disconnected",
                            encoded_samples,
                            encoded_samples as f64 / DIRECT_OGG_FLAC_SAMPLE_RATE as f64,
                        );
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

// ─── OGG helpers (copiés de streaming_ogg_flac_sink) ─────────────────────────

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
        let client_notify_internal = Arc::new(tokio::sync::Notify::new());
        let (client_connect_tx, _) = watch::channel(0u64);
        let client_connect_tx = Arc::new(client_connect_tx);
        let (first_byte_tx, _) = watch::channel(false);
        let first_byte_tx = Arc::new(first_byte_tx);
        let current_timestamp = Arc::new(tokio::sync::RwLock::new(0.0f64));

        let logic = DirectOggFlacSinkLogic {
            pcm_tx: pcm_tx.clone(),
            client_notify: client_notify_internal.clone(),
        };

        let sink = Self {
            inner: Node::new_with_input(logic, 16),
        };

        let handle = DirectOggFlacHandle {
            pcm_tx,
            client_connect_tx,
            client_notify_internal,
            first_byte_tx,
            encoder_options,
            current_timestamp,
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
