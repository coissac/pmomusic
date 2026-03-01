//! PlayerSource — Source audio avec contrôle de transport AVTransport UPnP.
//!
//! Implémente un nœud `AudioPipelineNode` qui encapsule le cycle de vie complet
//! d'un renderer UPnP : LoadUri, LoadNextUri, Play, Pause, Stop, Seek.
//!
//! # Architecture
//!
//! ```text
//! PlayerHandle (clonable, envoyé aux handlers UPnP)
//!     │ PlayerCommand (mpsc)
//!     ▼
//! PlayerSource (AudioPipelineNode — nœud source)
//!     │ AudioSegment (mpsc)
//!     ▼
//! ResamplingNode → ToI24Node → StreamingOggFlacSink → HTTP clients
//! ```
//!
//! # Gestion de la Pause
//!
//! La Pause ne coupe pas la source brutalement. Elle bloque l'émission de chunks.
//! À la reprise, un `TrackBoundary` est injecté avant les données — cela déclenche
//! EOS + nouveau BOS OGG dans `StreamingOggFlacSink`, garantissant un bitstream
//! propre aligné sur un frame boundary.
//!
//! # Transitions gapless
//!
//! Si `LoadNextUri` a été appelé avant la fin de la piste courante, la transition
//! se fait via un `TrackBoundary` sans interruption du flux OGG.

use std::sync::Arc;

use async_trait::async_trait;
use pmometadata::{MemoryTrackMetadata, TrackMetadata};
use pmoaudio::{
    AudioSegment, SyncMarker,
    nodes::AudioError,
    pipeline::{AudioPipelineNode, Node, NodeLogic, send_to_children},
};
use tokio::sync::{broadcast, mpsc};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

use super::uri_source::UriSource;

// ─── Commandes de transport ───────────────────────────────────────────────────

/// Commandes de transport AVTransport UPnP
#[derive(Debug)]
pub enum PlayerCommand {
    /// SetAVTransportURI — charge une URI (démarre pas encore)
    LoadUri(String),
    /// SetNextAVTransportURI — pré-charge pour transition gapless
    LoadNextUri(String),
    /// Play — démarre ou reprend la lecture
    Play,
    /// Pause — suspend la lecture, position préservée
    Pause,
    /// Stop — arrête la lecture, position remise à 0
    Stop,
    /// Seek — reprend depuis la position donnée (en secondes)
    Seek(f64),
}

// ─── Événements remontés ──────────────────────────────────────────────────────

/// Événements remontés par la PlayerSource vers les handlers UPnP
#[derive(Debug, Clone)]
pub enum PlayerEvent {
    /// Lecture démarrée ou reprise
    Playing {
        uri: String,
        duration_sec: Option<f64>,
    },
    /// Lecture suspendue
    Paused {
        position_sec: f64,
    },
    /// Lecture arrêtée
    Stopped,
    /// Fin de piste (pour que le ControlPoint avance la queue)
    TrackEnded,
    /// Position courante (émise ~1/s pendant la lecture)
    Position { position_sec: f64 },
    /// Erreur lors de l'ouverture ou de la lecture
    Error(String),
}

// ─── Handle de contrôle ───────────────────────────────────────────────────────

/// Handle de contrôle clonable, envoyé aux handlers UPnP
#[derive(Clone)]
pub struct PlayerHandle {
    command_tx: mpsc::Sender<PlayerCommand>,
    event_tx: broadcast::Sender<PlayerEvent>,
}

impl PlayerHandle {
    /// Charge une URI (sans démarrer la lecture)
    pub async fn load_uri(&self, uri: impl Into<String>) {
        let _ = self.command_tx.send(PlayerCommand::LoadUri(uri.into())).await;
    }

    /// Pré-charge l'URI suivante pour transition gapless
    pub async fn load_next_uri(&self, uri: impl Into<String>) {
        let _ = self.command_tx.send(PlayerCommand::LoadNextUri(uri.into())).await;
    }

    /// Démarre ou reprend la lecture
    pub async fn play(&self) {
        let _ = self.command_tx.send(PlayerCommand::Play).await;
    }

    /// Suspend la lecture (position préservée)
    pub async fn pause(&self) {
        let _ = self.command_tx.send(PlayerCommand::Pause).await;
    }

    /// Arrête la lecture (position remise à 0)
    pub async fn stop(&self) {
        let _ = self.command_tx.send(PlayerCommand::Stop).await;
    }

    /// Reprend depuis la position donnée (en secondes)
    pub async fn seek(&self, pos_sec: f64) {
        let _ = self.command_tx.send(PlayerCommand::Seek(pos_sec)).await;
    }

    /// Souscrit aux événements de transport
    pub fn subscribe_events(&self) -> broadcast::Receiver<PlayerEvent> {
        self.event_tx.subscribe()
    }
}

// ─── État de transport ────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
enum TransportState {
    /// Aucune URI chargée
    Idle,
    /// URI chargée, en attente de Play
    Loaded,
    /// Lecture en cours
    Playing,
    /// Lecture suspendue
    Paused,
}

// ─── Logique interne ──────────────────────────────────────────────────────────

struct PlayerSourceLogic {
    command_rx: mpsc::Receiver<PlayerCommand>,
    event_tx: broadcast::Sender<PlayerEvent>,
}

#[async_trait]
impl NodeLogic for PlayerSourceLogic {
    async fn process(
        &mut self,
        _input: Option<mpsc::Receiver<Arc<AudioSegment>>>,
        output: Vec<mpsc::Sender<Arc<AudioSegment>>>,
        stop_token: CancellationToken,
    ) -> Result<(), AudioError> {
        let mut state = TransportState::Idle;
        let mut current_uri: Option<String> = None;
        let mut next_uri: Option<String> = None;
        let mut paused_at_sec: f64 = 0.0;

        info!("PlayerSource: started");

        loop {
            match state {
                TransportState::Idle | TransportState::Loaded | TransportState::Paused => {
                    // Attendre une commande
                    tokio::select! {
                        _ = stop_token.cancelled() => {
                            debug!("PlayerSource: stop requested");
                            break;
                        }
                        cmd = self.command_rx.recv() => {
                            match cmd {
                                None => break,
                                Some(cmd) => {
                                    self.handle_command(
                                        cmd, &mut state, &mut current_uri,
                                        &mut next_uri, &mut paused_at_sec,
                                        &output, &stop_token,
                                    ).await?;
                                }
                            }
                        }
                    }
                }

                TransportState::Playing => {
                    let uri = match current_uri.clone() {
                        Some(u) => u,
                        None => {
                            state = TransportState::Idle;
                            continue;
                        }
                    };

                    // Ouvrir la source depuis la position courante
                    let source = match UriSource::open(&uri, paused_at_sec, stop_token.clone()).await {
                        Ok(s) => s,
                        Err(e) => {
                            warn!("PlayerSource: failed to open {:?}: {}", uri, e);
                            let _ = self.event_tx.send(PlayerEvent::Error(e.to_string()));
                            state = TransportState::Idle;
                            continue;
                        }
                    };

                    let duration_sec = source.duration_sec();
                    let _ = self.event_tx.send(PlayerEvent::Playing {
                        uri: uri.clone(),
                        duration_sec,
                    });
                    info!("PlayerSource: playing {:?} from {:.1}s", uri, paused_at_sec);

                    // Pompe audio — s'arrête sur EOF, Pause, Stop, ou commande
                    let result = self.pump(
                        source,
                        &mut state,
                        &mut current_uri,
                        &mut next_uri,
                        &mut paused_at_sec,
                        &output,
                        &stop_token,
                    ).await;

                    match result {
                        Ok(()) => {}
                        Err(AudioError::ChildDied) => {
                            debug!("PlayerSource: child died, stopping");
                            break;
                        }
                        Err(e) => {
                            warn!("PlayerSource: pump error: {}", e);
                            let _ = self.event_tx.send(PlayerEvent::Error(e.to_string()));
                            state = TransportState::Idle;
                        }
                    }
                }
            }
        }

        info!("PlayerSource: stopped");
        Ok(())
    }
}

impl PlayerSourceLogic {
    /// Traite une commande de transport dans les états non-Playing.
    async fn handle_command(
        &mut self,
        cmd: PlayerCommand,
        state: &mut TransportState,
        current_uri: &mut Option<String>,
        next_uri: &mut Option<String>,
        paused_at_sec: &mut f64,
        output: &[mpsc::Sender<Arc<AudioSegment>>],
        stop_token: &CancellationToken,
    ) -> Result<(), AudioError> {
        match cmd {
            PlayerCommand::LoadUri(uri) => {
                info!("PlayerSource: LoadUri {:?}", uri);
                *current_uri = Some(uri);
                *next_uri = None;
                *paused_at_sec = 0.0;
                *state = TransportState::Loaded;
            }

            PlayerCommand::LoadNextUri(uri) => {
                debug!("PlayerSource: LoadNextUri {:?}", uri);
                *next_uri = Some(uri);
            }

            PlayerCommand::Play => {
                match state {
                    TransportState::Paused => {
                        info!("PlayerSource: Play (resume from {:.1}s)", paused_at_sec);
                        // Injecter un TrackBoundary pour EOS + nouveau BOS OGG propre
                        send_track_boundary(current_uri.as_deref(), output, *paused_at_sec, stop_token).await?;
                        *state = TransportState::Playing;
                    }
                    TransportState::Loaded => {
                        info!("PlayerSource: Play (start)");
                        *paused_at_sec = 0.0;
                        // TrackBoundary initial pour le premier BOS OGG
                        send_track_boundary(current_uri.as_deref(), output, 0.0, stop_token).await?;
                        *state = TransportState::Playing;
                    }
                    TransportState::Idle => {
                        debug!("PlayerSource: Play ignored (no URI loaded)");
                    }
                    TransportState::Playing => {
                        debug!("PlayerSource: Play ignored (already playing)");
                    }
                }
            }

            PlayerCommand::Pause => {
                if *state == TransportState::Playing {
                    // Géré dans pump() — ne devrait pas arriver ici
                    debug!("PlayerSource: Pause ignored in non-Playing state");
                } else {
                    debug!("PlayerSource: Pause ignored (not playing)");
                }
            }

            PlayerCommand::Stop => {
                info!("PlayerSource: Stop");
                *paused_at_sec = 0.0;
                *state = TransportState::Idle;
                let _ = self.event_tx.send(PlayerEvent::Stopped);
            }

            PlayerCommand::Seek(pos) => {
                if current_uri.is_some() {
                    info!("PlayerSource: Seek to {:.1}s", pos);
                    *paused_at_sec = pos;
                    send_track_boundary(current_uri.as_deref(), output, pos, stop_token).await?;
                    *state = TransportState::Playing;
                }
            }
        }
        Ok(())
    }

    /// Pompe audio : émet les chunks depuis `source` vers `output`.
    ///
    /// Surveille simultanément les commandes de contrôle.
    /// Se termine quand : EOF, Pause, Stop, cancel, ou erreur.
    async fn pump(
        &mut self,
        source: UriSource,
        state: &mut TransportState,
        current_uri: &mut Option<String>,
        next_uri: &mut Option<String>,
        paused_at_sec: &mut f64,
        output: &[mpsc::Sender<Arc<AudioSegment>>],
        stop_token: &CancellationToken,
    ) -> Result<(), AudioError> {
        // Canal interne pour recevoir les chunks de UriSource
        let (chunk_tx, mut chunk_rx) = mpsc::channel::<Arc<AudioSegment>>(16);
        let source_stop = stop_token.child_token();
        let source_stop_clone = source_stop.clone();

        // Dernière seconde entière pour laquelle on a émis un Position
        let mut last_reported_sec: i64 = -1;

        // Spawner l'émission de la source dans une tâche séparée
        let emit_task = tokio::spawn(async move {
            source.emit_to_channel(&chunk_tx, &source_stop_clone).await
        });

        let mut result = Ok(());

        loop {
            tokio::select! {
                _ = stop_token.cancelled() => {
                    source_stop.cancel();
                    break;
                }

                cmd = self.command_rx.recv() => {
                    match cmd {
                        None => {
                            source_stop.cancel();
                            break;
                        }
                        Some(PlayerCommand::Pause) => {
                            info!("PlayerSource: Pause at {:.1}s", *paused_at_sec);
                            source_stop.cancel();
                            *state = TransportState::Paused;
                            let _ = self.event_tx.send(PlayerEvent::Paused {
                                position_sec: *paused_at_sec,
                            });
                            break;
                        }
                        Some(PlayerCommand::Stop) => {
                            info!("PlayerSource: Stop");
                            source_stop.cancel();
                            *paused_at_sec = 0.0;
                            *state = TransportState::Idle;
                            let _ = self.event_tx.send(PlayerEvent::Stopped);
                            break;
                        }
                        Some(PlayerCommand::LoadUri(uri)) => {
                            info!("PlayerSource: LoadUri (replacing current) {:?}", uri);
                            source_stop.cancel();
                            *current_uri = Some(uri);
                            *next_uri = None;
                            *paused_at_sec = 0.0;
                            // TrackBoundary pour clore le bitstream OGG proprement
                            if let Err(e) = send_track_boundary(current_uri.as_deref(), output, 0.0, stop_token).await {
                                result = Err(e);
                            }
                            *state = TransportState::Playing;
                            break;
                        }
                        Some(PlayerCommand::LoadNextUri(uri)) => {
                            debug!("PlayerSource: LoadNextUri {:?}", uri);
                            *next_uri = Some(uri);
                        }
                        Some(PlayerCommand::Seek(pos)) => {
                            info!("PlayerSource: Seek to {:.1}s", pos);
                            source_stop.cancel();
                            *paused_at_sec = pos;
                            if let Err(e) = send_track_boundary(current_uri.as_deref(), output, pos, stop_token).await {
                                result = Err(e);
                                break;
                            }
                            *state = TransportState::Playing;
                            break;
                        }
                        Some(PlayerCommand::Play) => {
                            debug!("PlayerSource: Play ignored (already playing)");
                        }
                    }
                }

                segment = chunk_rx.recv() => {
                    match segment {
                        None => {
                            // EOF de la source
                            debug!("PlayerSource: EOF");
                            // Signaler la fin de piste dans tous les cas (gapless ou non)
                            // pour que le ControlPoint avance sa queue et mette à jour son état.
                            let _ = self.event_tx.send(PlayerEvent::TrackEnded);
                            if let Some(next) = next_uri.take() {
                                // Transition gapless vers la piste suivante
                                info!("PlayerSource: gapless transition to {:?}", next);
                                *current_uri = Some(next);
                                *paused_at_sec = 0.0;
                                if let Err(e) = send_track_boundary(current_uri.as_deref(), output, 0.0, stop_token).await {
                                    result = Err(e);
                                }
                                *state = TransportState::Playing;
                            } else {
                                *state = TransportState::Loaded;
                                *paused_at_sec = 0.0;
                            }
                            break;
                        }
                        Some(seg) => {
                            // Mettre à jour la position courante
                            if seg.is_audio_chunk() {
                                *paused_at_sec = seg.timestamp_sec;
                                // Émettre Position ~1/s
                                let sec = paused_at_sec.floor() as i64;
                                if sec != last_reported_sec {
                                    last_reported_sec = sec;
                                    let _ = self.event_tx.send(PlayerEvent::Position {
                                        position_sec: *paused_at_sec,
                                    });
                                }
                            }
                            // Envoyer au pipeline en aval
                            if let Err(e) = send_to_children("PlayerSource", output, seg).await {
                                source_stop.cancel();
                                result = Err(e);
                                break;
                            }
                        }
                    }
                }
            }
        }

        // Attendre que la tâche source se termine proprement
        let _ = emit_task.await;
        result
    }
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

/// Envoie un TrackBoundary à tous les enfants.
///
/// Utilisé pour déclencher EOS + nouveau BOS OGG dans StreamingOggFlacSink,
/// garantissant un bitstream propre à chaque démarrage, reprise ou seek.
async fn send_track_boundary(
    uri: Option<&str>,
    output: &[mpsc::Sender<Arc<AudioSegment>>],
    timestamp_sec: f64,
    stop_token: &CancellationToken,
) -> Result<(), AudioError> {
    if output.is_empty() || stop_token.is_cancelled() {
        return Ok(());
    }

    let mut meta = MemoryTrackMetadata::new();
    if let Some(u) = uri {
        let _ = meta.set_title(Some(u.to_string())).await;
    }
    let meta_arc = Arc::new(tokio::sync::RwLock::new(meta));
    let boundary = AudioSegment::new_track_boundary(0, timestamp_sec, meta_arc);

    send_to_children("PlayerSource", output, boundary).await
}

// ─── Nœud public ─────────────────────────────────────────────────────────────

/// Source audio avec contrôle de transport AVTransport UPnP.
///
/// Utilisée comme nœud racine d'un pipeline audio. Le `PlayerHandle` retourné
/// permet d'envoyer des commandes (Play, Pause, Stop, Seek, LoadUri) depuis
/// les handlers UPnP.
pub struct PlayerSource {
    inner: Node<PlayerSourceLogic>,
}

impl PlayerSource {
    /// Crée une nouvelle PlayerSource et son handle de contrôle.
    pub fn new() -> (Self, PlayerHandle) {
        let (command_tx, command_rx) = mpsc::channel::<PlayerCommand>(32);
        let (event_tx, _) = broadcast::channel::<PlayerEvent>(16);

        let logic = PlayerSourceLogic {
            command_rx,
            event_tx: event_tx.clone(),
        };

        let handle = PlayerHandle {
            command_tx,
            event_tx,
        };

        (
            Self {
                inner: Node::new_source(logic),
            },
            handle,
        )
    }
}

#[async_trait]
impl AudioPipelineNode for PlayerSource {
    fn get_tx(&self) -> Option<mpsc::Sender<Arc<AudioSegment>>> {
        self.inner.get_tx()
    }

    fn register(&mut self, child: Box<dyn AudioPipelineNode>) {
        self.inner.register(child);
    }

    async fn run(self: Box<Self>, stop_token: CancellationToken) -> Result<(), AudioError> {
        Box::new(self.inner).run(stop_token).await
    }
}
