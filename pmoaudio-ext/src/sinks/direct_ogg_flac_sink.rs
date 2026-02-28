//! DirectOggFlacSink — nœud puits OGG-FLAC pour un seul client HTTP.
//!
//! Combine la logique de backpressure/reconnexion de `DirectFlacSink`
//! avec l'encodage OGG-FLAC de `StreamingOggFlacSink`.
//!
//! # Cycle de vie
//!
//! - **Play** : le navigateur appelle `GET /stream`. `connect()` crée un channel
//!   Bytes (bytes_tx/rx), une task de forwarding qui copie les Bytes dans un
//!   DuplexStream, et lance le premier encodeur OGG-FLAC. Le `bytes_tx` est
//!   stocké dans le sink pour le chaining TrackBoundary.
//! - **Stop** : le navigateur ferme la connexion. Le DuplexStream se rompt,
//!   la task de forwarding se termine, le bytes_tx devient invalide. Le sink
//!   voit `pcm_tx.send()` échouer et bloque sur `client_notify`.
//! - **TrackBoundary** : le sink ferme le `pcm_tx` courant (EOF → encodeur écrit
//!   EOS OGG), attend la fin de l'encodeur, puis relance un nouvel encodeur
//!   dans le même `bytes_tx` (OGG chaining : nouvelle BOS OGG dans le même flux HTTP).
//! - **Play suivant** : `connect()` → nouveau DuplexStream + channel → nouveau pipe.
//!
//! # Architecture
//!
//! ```text
//! AudioSegment I24 @ 96 kHz
//!     ↓ NodeLogic::process()  [bloque si pas de client]
//! chunk_to_pcm_bytes() → PCM 24-bit LE
//!     ↓ SharedPcmTx
//! ByteStreamReader → encode_flac_stream() → OGG pages
//!     ↓ SharedBytesTx (mpsc::Sender<Bytes>)  ← persistant entre encodeurs
//! [forwarding task] → tokio::io::DuplexStream
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
use tokio::task::JoinHandle;
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
/// Capacité du channel Bytes intermédiaire.
const BYTES_CHANNEL_CAPACITY: usize = 64;

// ─── Shared state ─────────────────────────────────────────────────────────────

type SharedPcmTx = Arc<Mutex<Option<mpsc::Sender<PcmChunk>>>>;
/// Canal Bytes persistant entre les encodeurs successifs (OGG chaining).
/// Le sink y envoie les pages OGG ; une task de forwarding les copie dans le DuplexStream.
type SharedBytesTx = Arc<Mutex<Option<mpsc::Sender<Bytes>>>>;
/// Handle de la task encodeur courante.
type SharedEncoderTask = Arc<Mutex<Option<JoinHandle<()>>>>;

// ─── Handle public ────────────────────────────────────────────────────────────

/// Handle vers le sink, cloneable, reconnectable à chaque Play.
#[derive(Clone)]
pub struct DirectOggFlacHandle {
    pcm_tx: SharedPcmTx,
    client_connect_tx: Arc<watch::Sender<u64>>,
    client_notify_internal: Arc<tokio::sync::Notify>,
    first_byte_tx: Arc<watch::Sender<bool>>,
    encoder_options: EncoderOptions,
    current_timestamp: Arc<tokio::sync::RwLock<f64>>,
    /// Canal Bytes persistant partagé avec la logic du sink pour le OGG chaining.
    bytes_tx: SharedBytesTx,
    /// Task encodeur courante partagée avec la logic du sink.
    encoder_task: SharedEncoderTask,
}

impl DirectOggFlacHandle {
    /// Crée un nouveau flux OGG-FLAC pour le client HTTP.
    /// Remplace toute connexion précédente.
    pub async fn connect(&self) -> DirectOggFlacStream {
        let connect_count_before = *self.client_connect_tx.borrow();
        debug!("DirectOggFlacHandle::connect() called, connect_count={}", connect_count_before);

        // Annuler l'encodeur précédent s'il tourne encore
        if let Some(old_task) = self.encoder_task.lock().await.take() {
            old_task.abort();
        }

        // Créer le channel Bytes persistant (OGG chaining)
        let (bytes_tx, bytes_rx) = mpsc::channel::<Bytes>(BYTES_CHANNEL_CAPACITY);
        *self.bytes_tx.lock().await = Some(bytes_tx.clone());

        // Créer le DuplexStream vers le client HTTP
        let (mut pipe_writer, pipe_reader) = tokio::io::duplex(PIPE_CAPACITY);

        // Task de forwarding : Bytes → DuplexStream
        // Se termine quand bytes_rx est fermé (bytes_tx droppé) ou pipe cassé
        tokio::spawn(async move {
            use tokio::io::AsyncWriteExt;
            let mut rx = bytes_rx;
            while let Some(bytes) = rx.recv().await {
                if pipe_writer.write_all(&bytes).await.is_err() {
                    debug!("DirectOggFlacStream forwarder: pipe broken, client disconnected");
                    break;
                }
            }
            debug!("DirectOggFlacStream forwarder: done");
        });

        // Réinitialiser les signaux
        let _ = self.first_byte_tx.send(false);
        *self.current_timestamp.write().await = 0.0;

        // Créer le premier pcm_tx + encodeur
        let (pcm_tx, pcm_rx) = mpsc::channel::<PcmChunk>(8);
        let current_dur = Arc::new(tokio::sync::RwLock::new(0.0f64));
        let pcm_reader = ByteStreamReader::new(pcm_rx, self.current_timestamp.clone(), current_dur);

        *self.pcm_tx.lock().await = Some(pcm_tx);

        let new_count = connect_count_before.wrapping_add(1);
        let _ = self.client_connect_tx.send(new_count);
        self.client_notify_internal.notify_one();
        debug!("DirectOggFlacHandle::connect() client_connect_count -> {}", new_count);

        let options = self.encoder_options.clone();
        let current_timestamp = self.current_timestamp.clone();
        let shared_bytes_tx = self.bytes_tx.clone();

        let handle = tokio::spawn(async move {
            debug!("DirectOggFlacHandle: initial encoder task started");
            if let Err(e) = run_ogg_encoder(pcm_reader, bytes_tx, shared_bytes_tx, options, current_timestamp).await {
                debug!("DirectOggFlacHandle: initial encoder stopped: {}", e);
            }
            debug!("DirectOggFlacHandle: initial encoder task ended");
        });

        *self.encoder_task.lock().await = Some(handle);

        debug!("DirectOggFlacHandle::connect() returning DirectOggFlacStream");
        DirectOggFlacStream {
            inner: pipe_reader,
            first_byte_tx: Some(self.first_byte_tx.clone()),
        }
    }

    pub fn first_byte_ready(&self) -> watch::Receiver<bool> {
        self.first_byte_tx.subscribe()
    }

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
    encoder_options: EncoderOptions,
    encoder_task: SharedEncoderTask,
    bytes_tx: SharedBytesTx,
    current_timestamp: Arc<tokio::sync::RwLock<f64>>,
    /// Vrai dès qu'au moins un chunk audio a été encodé dans le stream courant.
    /// Empêche le OGG chaining sur le TrackBoundary initial (avant tout audio).
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
                                        debug!("DirectOggFlacSink: TrackBoundary ignored (no frames encoded yet)");
                                    }
                                }
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

impl DirectOggFlacSinkLogic {
    /// OGG chaining : ferme l'encodeur courant (EOF → EOS OGG), attend sa fin,
    /// puis relance un nouvel encodeur dans le même channel Bytes (nouvelle BOS OGG).
    async fn do_track_boundary(&mut self) {
        // 1. Fermer le pcm_tx courant → EOF dans ByteStreamReader → encodeur écrit EOS OGG
        {
            let mut guard = self.pcm_tx.lock().await;
            *guard = None;
        }

        // 2. Attendre la fin de la task encodeur courante
        let old_task = self.encoder_task.lock().await.take();
        if let Some(handle) = old_task {
            let _ = handle.await;
            debug!("DirectOggFlacSink: previous encoder task joined");
        }

        // 3. Vérifier que le bytes_tx est encore valide (client pas déconnecté)
        let bytes_tx = {
            let guard = self.bytes_tx.lock().await;
            guard.clone()
        };
        let Some(bytes_tx) = bytes_tx else {
            debug!("DirectOggFlacSink: bytes_tx gone (client disconnected), skip OGG chaining");
            return;
        };

        // 4. Nouveau pcm_tx + ByteStreamReader
        let (pcm_tx, pcm_rx) = mpsc::channel::<PcmChunk>(8);
        let current_dur = Arc::new(tokio::sync::RwLock::new(0.0f64));
        let pcm_reader = ByteStreamReader::new(
            pcm_rx,
            self.current_timestamp.clone(),
            current_dur,
        );
        *self.pcm_tx.lock().await = Some(pcm_tx);

        // 5. Relancer l'encodeur dans le même bytes_tx (OGG chaining : nouvelle BOS OGG)
        let options = self.encoder_options.clone();
        let current_timestamp = self.current_timestamp.clone();
        let shared_bytes_tx = self.bytes_tx.clone();

        let handle = tokio::spawn(async move {
            debug!("DirectOggFlacSink: chained encoder task started");
            if let Err(e) = run_ogg_encoder(pcm_reader, bytes_tx, shared_bytes_tx, options, current_timestamp).await {
                debug!("DirectOggFlacSink: chained encoder stopped: {}", e);
            }
            debug!("DirectOggFlacSink: chained encoder task ended");
        });

        *self.encoder_task.lock().await = Some(handle);
        debug!("DirectOggFlacSink: OGG chaining complete, new encoder started");
    }
}

// ─── Encodeur FLAC + wrapper OGG ─────────────────────────────────────────────

/// Encode PCM → FLAC → OGG et envoie les pages OGG dans `bytes_tx`.
/// Quand le channel devient invalide (client déconnecté), nettoie `shared_bytes_tx`.
async fn run_ogg_encoder(
    pcm_reader: ByteStreamReader,
    bytes_tx: mpsc::Sender<Bytes>,
    shared_bytes_tx: SharedBytesTx,
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

    let flac_header = read_flac_header(&mut flac_stream).await?;
    let _sample_rate = extract_sample_rate_from_streaminfo(&flac_header)?;

    let stream_serial: u32 = rand::random();
    let mut ogg = OggPageWriter::new(stream_serial);

    let ogg_flac_id = create_ogg_flac_identification(&flac_header)?;
    let bos_page = Bytes::from(ogg.create_page(&ogg_flac_id, true, false, false));
    let vorbis_comment = create_empty_vorbis_comment();
    let comment_page = Bytes::from(ogg.create_page(&vorbis_comment, false, false, false));

    macro_rules! send_or_cleanup {
        ($page:expr) => {
            if bytes_tx.send($page).await.is_err() {
                debug!("DirectOggFlacSink: bytes_tx broken, client disconnected");
                *shared_bytes_tx.lock().await = None;
                return Ok(());
            }
        };
    }

    send_or_cleanup!(bos_page);
    send_or_cleanup!(comment_page);

    use tokio::io::AsyncReadExt;
    use crate::sinks::flac_frame_utils::{validate_frame_header_crc, parse_flac_block_size};

    let mut encoded_samples = 0u64;
    let mut read_buffer = vec![0u8; 16384];
    let mut accumulator: Vec<u8> = Vec::with_capacity(32768);

    loop {
        match flac_stream.read(&mut read_buffer).await {
            Ok(0) => {
                // EOF : page EOS finale
                let eos_page = Bytes::from(ogg.create_page(&accumulator, false, true, false));
                let _ = bytes_tx.send(eos_page).await;
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

                    let ogg_page = Bytes::from(ogg.create_page(&frame, false, false, false));
                    if bytes_tx.send(ogg_page).await.is_err() {
                        warn!(
                            "DirectOggFlacSink: bytes_tx broken after {} samples ({:.3}s), client disconnected",
                            encoded_samples,
                            encoded_samples as f64 / DIRECT_OGG_FLAC_SAMPLE_RATE as f64,
                        );
                        *shared_bytes_tx.lock().await = None;
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
        let client_notify_internal = Arc::new(tokio::sync::Notify::new());
        let (client_connect_tx, _) = watch::channel(0u64);
        let client_connect_tx = Arc::new(client_connect_tx);
        let (first_byte_tx, _) = watch::channel(false);
        let first_byte_tx = Arc::new(first_byte_tx);
        let current_timestamp = Arc::new(tokio::sync::RwLock::new(0.0f64));
        let bytes_tx: SharedBytesTx = Arc::new(Mutex::new(None));
        let encoder_task: SharedEncoderTask = Arc::new(Mutex::new(None));

        let logic = DirectOggFlacSinkLogic {
            pcm_tx: pcm_tx.clone(),
            client_notify: client_notify_internal.clone(),
            encoder_options: encoder_options.clone(),
            encoder_task: encoder_task.clone(),
            bytes_tx: bytes_tx.clone(),
            current_timestamp: current_timestamp.clone(),
            has_encoded_frames: false,
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
            bytes_tx,
            encoder_task,
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
