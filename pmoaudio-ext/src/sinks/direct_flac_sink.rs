//! DirectFlacSink — nœud puits FLAC pour un seul client HTTP.
//!
//! Encode l'audio en FLAC (format fixe : 96 kHz / stéréo / 24 bits).
//!
//! # Cycle de vie
//!
//! - **Play** : le navigateur appelle `GET /stream`. `connect()` crée un nouveau
//!   canal PCM + pipe duplex + encodeur FLAC, installe le sender dans le sink,
//!   et notifie le sink via `client_notify`. Le flux reste ouvert : les morceaux
//!   s'enchaînent en gapless.
//! - **Stop** : le navigateur ferme la connexion. Le pipe se rompt, l'encodeur
//!   s'arrête. Le sink voit `pcm_tx.send()` échouer, passe le sender à `None`,
//!   et **bloque** sur `client_notify` jusqu'au prochain Play.
//!   Cela bloque la source et préserve la backpressure.
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
//!     ↓ tokio::io::copy()
//!     ↓ tokio::io::duplex pipe (256 KB)
//!     ↓ DirectFlacStream (AsyncRead) → Body HTTP
//! ```

use std::io;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use async_trait::async_trait;
use pmoaudio::{
    pipeline::{AudioPipelineNode, Node, NodeLogic, PipelineHandle, StopReason},
    AudioError, AudioSegment, SyncMarker, TypeRequirement, TypedAudioNode, _AudioSegment,
};
use pmoflac::{EncoderOptions, PcmFormat};
use tokio::io::{AsyncRead, ReadBuf};
use tokio::sync::{mpsc, watch, Mutex};
use tokio_util::sync::CancellationToken;
use tracing::debug;

use crate::sinks::byte_stream_reader::{ByteStreamReader, PcmChunk};
use crate::sinks::chunk_to_pcm::chunk_to_pcm_bytes;

/// Format de sortie fixe du sink.
pub const DIRECT_FLAC_SAMPLE_RATE: u32 = 96_000;
pub const DIRECT_FLAC_CHANNELS: u8 = 2;
pub const DIRECT_FLAC_BITS_PER_SAMPLE: u8 = 24;

/// Capacité du pipe duplex (~256 KB ≈ 0.35s à 96 kHz/24 bits/stéréo).
const PIPE_CAPACITY: usize = 256 * 1024;

// ─── Shared state ─────────────────────────────────────────────────────────────

type SharedPcmTx = Arc<Mutex<Option<mpsc::Sender<PcmChunk>>>>;

// ─── Handle public ────────────────────────────────────────────────────────────

/// Handle vers le sink, cloneable, reconnectable à chaque Play.
#[derive(Clone)]
pub struct DirectFlacHandle {
    pcm_tx: SharedPcmTx,
    /// Compteur de connexions : incrémenté à chaque connect().
    /// Utiliser un watch channel pour éviter les notifications perdues (vs Notify).
    client_connect_tx: Arc<watch::Sender<u64>>,
    /// Notifie le sink interne qu'un client vient de se connecter (edge-triggered,
    /// usage interne uniquement — le sink tourne dans le même contexte que connect()).
    client_notify_internal: Arc<tokio::sync::Notify>,
    /// Signale que le premier byte FLAC a été lu par le client HTTP.
    /// `false` au démarrage / après connect(), `true` dès le premier poll_read non-vide.
    first_byte_tx: Arc<watch::Sender<bool>>,
    encoder_options: EncoderOptions,
}

impl DirectFlacHandle {
    /// Crée un nouveau pipe + encodeur FLAC et retourne le flux côté lecture.
    /// Débloque le sink s'il attendait un client.
    pub async fn connect(&self) -> DirectFlacStream {
        let connect_count_before = *self.client_connect_tx.borrow();
        debug!("DirectFlacHandle::connect() called, connect_count={}", connect_count_before);

        let (pcm_tx, pcm_rx) = mpsc::channel::<PcmChunk>(8);
        let current_ts = Arc::new(tokio::sync::RwLock::new(0.0f64));
        let current_dur = Arc::new(tokio::sync::RwLock::new(0.0f64));
        let pcm_reader = ByteStreamReader::new(pcm_rx, current_ts, current_dur);

        let (pipe_writer, pipe_reader) = tokio::io::duplex(PIPE_CAPACITY);

        // Réinitialiser le signal "premier byte" AVANT de notifier le sink,
        // pour éviter qu'une notification précédente ne se propage.
        let _ = self.first_byte_tx.send(false);
        debug!("DirectFlacHandle::connect() first_byte reset to false");

        // Installer le nouveau sender (remplace l'éventuel ancien)
        *self.pcm_tx.lock().await = Some(pcm_tx);
        debug!("DirectFlacHandle::connect() pcm_tx installed");

        // Incrémenter le compteur de connexions (mémorisé dans watch — pas de perte)
        let new_count = connect_count_before.wrapping_add(1);
        let _ = self.client_connect_tx.send(new_count);
        debug!("DirectFlacHandle::connect() client_connect_count -> {}", new_count);
        // Débloquer le sink interne (même contexte async → pas de race)
        self.client_notify_internal.notify_one();

        // Lancer l'encodeur en background
        let options = self.encoder_options.clone();
        tokio::spawn(async move {
            debug!("DirectFlacHandle: encoder task started");
            if let Err(e) = run_encoder(pcm_reader, pipe_writer, options).await {
                debug!("DirectFlacStream encoder stopped: {}", e);
            }
            debug!("DirectFlacHandle: encoder task ended");
        });

        debug!("DirectFlacHandle::connect() returning DirectFlacStream");
        DirectFlacStream {
            inner: pipe_reader,
            first_byte_tx: Some(self.first_byte_tx.clone()),
        }
    }

    /// Retourne un receiver qui passe à `true` quand le premier byte FLAC
    /// a été effectivement lu par le client HTTP.
    pub fn first_byte_ready(&self) -> watch::Receiver<bool> {
        self.first_byte_tx.subscribe()
    }

    /// Attend qu'un client HTTP se connecte (i.e. que `connect()` soit appelé).
    /// Utilisé par `stream_source` pour retarder l'ouverture de la source
    /// jusqu'à ce que le navigateur soit prêt à recevoir des données.
    ///
    /// Mémorise la valeur du compteur au moment de l'appel et attend qu'elle
    /// augmente — ce qui garantit qu'on attend bien UNE NOUVELLE connexion,
    /// même si `connect()` a déjà été appelé lors d'une lecture précédente.
    pub async fn wait_for_client(&self) {
        let seen = *self.client_connect_tx.borrow();
        debug!("DirectFlacHandle::wait_for_client() called, seen connect_count={}", seen);
        // subscribe() retourne un receiver dont la valeur courante est marquée "changed"
        // donc wait_for() retourne immédiatement si la condition est déjà vraie.
        let mut rx = self.client_connect_tx.subscribe();
        let result = rx.wait_for(|v| {
            debug!("DirectFlacHandle::wait_for_client() checking v={} > seen={}: {}", v, seen, *v > seen);
            *v > seen
        }).await;
        debug!("DirectFlacHandle::wait_for_client() unblocked, result ok={}", result.is_ok());
    }
}

// ─── Stream public ────────────────────────────────────────────────────────────

/// Flux FLAC exposé au handler HTTP.
///
/// Transmet les bytes du pipe directement au client HTTP.
/// Intercepte le premier `poll_read` non-vide pour signaler via `first_byte_tx`
/// que des données FLAC ont effectivement été transmises au client.
pub struct DirectFlacStream {
    inner: tokio::io::DuplexStream,
    /// Présent jusqu'au premier byte reçu, puis consommé (set à None).
    first_byte_tx: Option<Arc<watch::Sender<bool>>>,
}

impl AsyncRead for DirectFlacStream {
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
                    debug!("DirectFlacStream: first {} bytes sent to HTTP client", filled_after - filled_before);
                    let _ = tx.send(true);
                }
            }
        }
        result
    }
}

// ─── Logique du nœud ─────────────────────────────────────────────────────────

struct DirectFlacSinkLogic {
    pcm_tx: SharedPcmTx,
    client_notify: Arc<tokio::sync::Notify>,
}

#[async_trait]
impl NodeLogic for DirectFlacSinkLogic {
    async fn process(
        &mut self,
        input: Option<mpsc::Receiver<Arc<AudioSegment>>>,
        _output: Vec<mpsc::Sender<Arc<AudioSegment>>>,
        stop_token: CancellationToken,
    ) -> Result<(), AudioError> {
        let mut input = input.ok_or_else(|| {
            AudioError::ProcessingError("DirectFlacSink requires an input".into())
        })?;

        loop {
            tokio::select! {
                _ = stop_token.cancelled() => {
                    debug!("DirectFlacSink: cancelled");
                    break;
                }

                segment = input.recv() => {
                    match segment {
                        None => {
                            debug!("DirectFlacSink: input channel closed");
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
                                    // Pas de client : bloquer jusqu'à connect() ou stop
                                    debug!("DirectFlacSink: no pcm_tx, waiting for client_notify...");
                                    tokio::select! {
                                        _ = stop_token.cancelled() => {
                                            debug!("DirectFlacSink: cancelled while waiting for client");
                                            return Ok(());
                                        }
                                        _ = self.client_notify.notified() => {
                                            debug!("DirectFlacSink: client_notify received, rechecking pcm_tx");
                                        }
                                    }
                                }

                                let tx = self.pcm_tx.lock().await.clone().unwrap();
                                let pcm_bytes = chunk_to_pcm_bytes(chunk, DIRECT_FLAC_BITS_PER_SAMPLE)?;
                                let duration_sec = chunk.len() as f64 / DIRECT_FLAC_SAMPLE_RATE as f64;
                                let pcm_chunk = PcmChunk {
                                    bytes: pcm_bytes,
                                    timestamp_sec: seg.timestamp_sec,
                                    duration_sec,
                                };
                                if tx.send(pcm_chunk).await.is_err() {
                                    debug!("DirectFlacSink: pcm_tx send failed (client disconnected), clearing pcm_tx");
                                    *self.pcm_tx.lock().await = None;
                                }
                            }
                            _AudioSegment::Sync(marker) => match marker.as_ref() {
                                SyncMarker::EndOfStream => {
                                    debug!("DirectFlacSink: EndOfStream");
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

// ─── Encodeur FLAC ────────────────────────────────────────────────────────────

async fn run_encoder(
    pcm_reader: ByteStreamReader,
    mut pipe_writer: tokio::io::DuplexStream,
    options: EncoderOptions,
) -> Result<(), AudioError> {
    let format = PcmFormat {
        sample_rate: DIRECT_FLAC_SAMPLE_RATE,
        channels: DIRECT_FLAC_CHANNELS,
        bits_per_sample: DIRECT_FLAC_BITS_PER_SAMPLE,
    };

    let mut flac_stream = pmoflac::encode_flac_stream(pcm_reader, format, options)
        .await
        .map_err(|e| AudioError::ProcessingError(format!("FLAC encoder init: {}", e)))?;

    tokio::io::copy(&mut flac_stream, &mut pipe_writer)
        .await
        .map_err(|e| AudioError::IoError(format!("FLAC pipe copy: {}", e)))?;

    flac_stream
        .wait()
        .await
        .map_err(|e| AudioError::ProcessingError(format!("FLAC encoder wait: {}", e)))?;

    Ok(())
}

// ─── Nœud public ─────────────────────────────────────────────────────────────

pub struct DirectFlacSink {
    inner: Node<DirectFlacSinkLogic>,
}

impl DirectFlacSink {
    pub fn new(encoder_options: EncoderOptions) -> (Self, DirectFlacHandle) {
        let pcm_tx: SharedPcmTx = Arc::new(Mutex::new(None));
        let client_notify_internal = Arc::new(tokio::sync::Notify::new());
        let (client_connect_tx, _) = watch::channel(0u64);
        let client_connect_tx = Arc::new(client_connect_tx);
        let (first_byte_tx, _) = watch::channel(false);
        let first_byte_tx = Arc::new(first_byte_tx);

        let logic = DirectFlacSinkLogic {
            pcm_tx: pcm_tx.clone(),
            client_notify: client_notify_internal.clone(),
        };

        let sink = Self {
            inner: Node::new_with_input(logic, 16),
        };

        let handle = DirectFlacHandle {
            pcm_tx,
            client_connect_tx,
            client_notify_internal,
            first_byte_tx,
            encoder_options,
        };

        (sink, handle)
    }
}

#[async_trait]
impl AudioPipelineNode for DirectFlacSink {
    fn get_tx(&self) -> Option<mpsc::Sender<Arc<AudioSegment>>> {
        self.inner.get_tx()
    }

    fn register(&mut self, _child: Box<dyn AudioPipelineNode>) {
        panic!("DirectFlacSink is a terminal sink and cannot have children");
    }

    async fn run(self: Box<Self>, stop_token: CancellationToken) -> Result<(), AudioError> {
        Box::new(self.inner).run(stop_token).await
    }

    fn start(self: Box<Self>) -> PipelineHandle {
        Box::new(self.inner).start()
    }
}

impl TypedAudioNode for DirectFlacSink {
    fn input_type(&self) -> Option<TypeRequirement> {
        Some(TypeRequirement::any_integer())
    }

    fn output_type(&self) -> Option<TypeRequirement> {
        None
    }
}
