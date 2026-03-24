//! Pipeline audio serveur par instance WebRenderer
//!
//! Chaque instance WebRenderer possède un pipeline indépendant :
//! - Une `PlayerSource` qui gère le cycle de vie AVTransport (Play/Pause/Stop/Seek/LoadUri)
//! - Un `StreamingOggFlacSink` qui encode et diffuse le flux OGG-FLAC aux clients HTTP
//! - Des nœuds de normalisation (resampling → 96 kHz, conversion → I24)

use std::sync::Arc;

use pmoaudio::{ResamplingNode, ToI24Node};
use pmoaudio_ext::{PlayerCommand, PlayerHandle, PlayerSource};
use pmoaudio_ext::sinks::{OggFlacStreamHandle, StreamingOggFlacSink};
use pmoflac::EncoderOptions;
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};

use crate::state::SharedState;

// ─── Ré-export des commandes pour les handlers ────────────────────────────────

/// Commandes de transport — alias vers PlayerCommand pour compatibilité handlers
pub use pmoaudio_ext::PlayerCommand as PipelineControl;

// ─── Handle vers le pipeline ─────────────────────────────────────────────────

/// Handle partageable vers le pipeline audio d'une instance.
///
/// Expose le `PlayerHandle` pour les commandes AVTransport et le `CancellationToken`
/// pour l'arrêt complet du pipeline.
#[derive(Clone)]
pub struct PipelineHandle {
    pub player: PlayerHandle,
    pub stop_token: CancellationToken,
    /// Volume courant (géré localement, pas dans PlayerSource)
    state: SharedState,
}

impl PipelineHandle {
    /// Envoie une commande de transport. Gère SetVolume/SetMute localement.
    pub async fn send(&self, cmd: PipelineControl) {
        match cmd {
            PlayerCommand::LoadUri(uri) => self.player.load_uri(uri).await,
            PlayerCommand::LoadNextUri(uri) => self.player.load_next_uri(uri).await,
            PlayerCommand::Play => self.player.play().await,
            PlayerCommand::Pause => self.player.pause().await,
            PlayerCommand::Stop => self.player.stop().await,
            PlayerCommand::Seek(pos) => self.player.seek(pos).await,
        }
    }
}

// ─── Pipeline instancié ──────────────────────────────────────────────────────

/// Pipeline audio complet pour une instance WebRenderer.
///
/// Créé au `POST /register`. Le flux OGG-FLAC est accessible via `flac_handle`
/// (multi-client broadcast, chaque `subscribe()` crée un flux indépendant).
pub struct InstancePipeline {
    /// Handle vers le sink OGG-FLAC — clonable, subscribe() crée un flux indépendant par client.
    pub flac_handle: OggFlacStreamHandle,
    pub pipeline_handle: PipelineHandle,
}

impl InstancePipeline {
    /// Crée et démarre le pipeline en background.
    /// Retourne immédiatement avec les handles nécessaires.
    pub fn start(
        state: SharedState,
        #[cfg(feature = "pmoserver")]
        control_point: Arc<pmocontrol::ControlPoint>,
        udn: String,
    ) -> Self {
        let stop_token = CancellationToken::new();

        use pmoaudio::pipeline::AudioPipelineNode;

        // Sink broadcast OGG-FLAC (multi-client, pacé à 0.5s max d'avance)
        let (sink, flac_handle) = StreamingOggFlacSink::new(EncoderOptions::default(), 24);

        // Nœud de conversion de profondeur : tout type entier → I24
        let mut to_i24 = ToI24Node::new();
        to_i24.register(sink.boxed());

        // Nœud de rééchantillonnage : n'importe quel sample rate → 96 kHz
        let mut resampler = ResamplingNode::new(96_000);
        resampler.register(to_i24.boxed());

        // Source avec contrôle AVTransport complet
        let (mut player_source, player_handle) = PlayerSource::new();
        player_source.register(resampler.boxed());

        // Lancer le pipeline en background
        let sink_stop = stop_token.clone();
        tokio::spawn(async move {
            if let Err(e) = player_source.boxed().run(sink_stop).await {
                warn!("Audio pipeline error: {:?}", e);
            }
            debug!("Pipeline task terminated");
        });

        // Écouter les événements PlayerSource pour mettre à jour le state UPnP
        let event_rx = player_handle.subscribe_events();
        let state_clone = state.clone();
        let udn_clone = udn.clone();
        #[cfg(feature = "pmoserver")]
        let cp_clone = control_point.clone();
        tokio::spawn(async move {
            run_event_listener(
                event_rx,
                state_clone,
                udn_clone,
                #[cfg(feature = "pmoserver")]
                cp_clone,
            ).await;
        });

        let pipeline_handle = PipelineHandle {
            player: player_handle,
            stop_token: stop_token.clone(),
            state,
        };

        Self {
            flac_handle,
            pipeline_handle,
        }
    }
}

// ─── Listener d'événements ────────────────────────────────────────────────────

async fn run_event_listener(
    mut event_rx: tokio::sync::broadcast::Receiver<pmoaudio_ext::PlayerEvent>,
    state: SharedState,
    udn: String,
    #[cfg(feature = "pmoserver")]
    control_point: Arc<pmocontrol::ControlPoint>,
) {
    use pmoaudio_ext::PlayerEvent;
    use crate::messages::PlaybackState;

    loop {
        match event_rx.recv().await {
            Ok(event) => match event {
                PlayerEvent::Playing { uri, duration_sec } => {
                    let mut s = state.write();
                    s.playback_state = PlaybackState::Playing;
                    s.current_uri = Some(uri);
                    s.duration = duration_sec.map(seconds_to_upnp_time);
                    s.position = None;
                    // Effacer next_uri/next_metadata : la nouvelle piste est maintenant courante
                    s.next_uri = None;
                    s.next_metadata = None;
                }
                PlayerEvent::Paused { position_sec } => {
                    let mut s = state.write();
                    s.playback_state = PlaybackState::Paused;
                    s.position = Some(seconds_to_upnp_time(position_sec));
                }
                PlayerEvent::Stopped => {
                    let mut s = state.write();
                    s.playback_state = PlaybackState::Stopped;
                    s.position = None;
                }
                PlayerEvent::Position { position_sec } => {
                    state.write().position = Some(seconds_to_upnp_time(position_sec));
                }
                PlayerEvent::TrackEnded => {
                    #[cfg(feature = "pmoserver")]
                    {
                        let cp = control_point.clone();
                        let udn_c = udn.clone();
                        tokio::spawn(async move {
                            cp.advance_queue_and_prefetch(&pmocontrol::DeviceId(udn_c));
                        });
                    }
                }
                PlayerEvent::Error(e) => {
                    tracing::warn!(udn = %udn, "PlayerSource error: {}", e);
                    state.write().playback_state = PlaybackState::Stopped;
                }
            },
            Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                tracing::warn!(udn = %udn, "Event listener lagged {} events", n);
            }
            Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                break;
            }
        }
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

pub fn seconds_to_upnp_time(s: f64) -> String {
    let s = s as u64;
    let h = s / 3600;
    let m = (s % 3600) / 60;
    let sec = s % 60;
    format!("{}:{:02}:{:02}", h, m, sec)
}

pub fn upnp_time_to_seconds(t: &str) -> f64 {
    let parts: Vec<f64> = t.split(':').filter_map(|p| p.parse().ok()).collect();
    match parts.as_slice() {
        [h, m, s] => h * 3600.0 + m * 60.0 + s,
        [m, s] => m * 60.0 + s,
        [s] => *s,
        _ => 0.0,
    }
}
