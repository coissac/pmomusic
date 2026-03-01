//! Pipeline audio serveur par instance WebRenderer
//!
//! Chaque instance WebRenderer possède un pipeline indépendant :
//! - Un `StreamingFlacSink` qui encode et diffuse le flux FLAC aux clients HTTP
//! - Un canal de contrôle `PipelineControl` alimenté par les handlers UPnP
//! - Une task background qui orchestre sources et sink

use std::sync::Arc;

use pmoaudio::{AudioSegment, ResamplingNode, ToI24Node};
use pmometadata::{MemoryTrackMetadata, TrackMetadata};
use pmoaudio_ext::sinks::{
    OggFlacStreamHandle, StreamingOggFlacSink,
};
use pmoaudio_ext::UriSource;
use pmoflac::EncoderOptions;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

use crate::messages::PlaybackState;
use crate::state::SharedState;

// ─── Commandes de contrôle ───────────────────────────────────────────────────

/// Commandes envoyées au pipeline audio de l'instance
#[derive(Debug)]
pub enum PipelineControl {
    LoadUri(String),
    LoadNextUri(String),
    Play,
    Pause,
    Stop,
    Seek(f64),
    SetVolume(u16),
    SetMute(bool),
    /// Notification interne : la source courante s'est terminée (EOF ou erreur)
    SourceEnded,
}

// ─── Handle vers le pipeline ─────────────────────────────────────────────────

/// Handle partageable vers le pipeline audio d'une instance
#[derive(Clone)]
pub struct PipelineHandle {
    pub control_tx: mpsc::Sender<PipelineControl>,
    pub stop_token: CancellationToken,
}

impl PipelineHandle {
    pub async fn send(&self, cmd: PipelineControl) {
        let _ = self.control_tx.send(cmd).await;
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
        let (control_tx, control_rx) = mpsc::channel::<PipelineControl>(32);

        // Chaîne de traitement : ResamplingNode(96kHz) → ToI24Node → StreamingOggFlacSink
        // Le broadcast pacé à 0.5s max d'avance, chaque subscribe() est indépendant.
        use pmoaudio::pipeline::AudioPipelineNode;

        let (sink, flac_handle) = StreamingOggFlacSink::new(EncoderOptions::default(), 24);

        // Nœud de conversion de profondeur : tout type entier → I24
        let mut to_i24 = ToI24Node::new();
        to_i24.register(sink.boxed());

        // Nœud de rééchantillonnage : n'importe quel sample rate → 96 kHz
        let mut resampler = ResamplingNode::new(96_000);
        resampler.register(to_i24.boxed());

        // Le tx d'entrée du resampler est le point d'entrée du pipeline
        let segment_tx = resampler.get_tx().expect("ResamplingNode doit avoir un sender");

        let pipeline_handle = PipelineHandle {
            control_tx,
            stop_token: stop_token.clone(),
        };

        // Lancer la chaîne resampler → to_i24 → sink en background
        let sink_stop = stop_token.clone();
        tokio::spawn(async move {
            if let Err(e) = resampler.boxed().run(sink_stop).await {
                warn!("Audio pipeline error: {:?}", e);
            }
            debug!("Sink task terminated");
        });

        // Task pipeline : reçoit les commandes UPnP et pilote les sources
        let stop_token_clone = stop_token.clone();
        let control_tx_clone = pipeline_handle.control_tx.clone();
        tokio::spawn(async move {
            run_pipeline(
                segment_tx,
                control_rx,
                control_tx_clone,
                stop_token_clone,
                state,
                udn,
                #[cfg(feature = "pmoserver")]
                control_point,
            )
            .await;
            debug!("Pipeline task terminated");
        });

        Self {
            flac_handle,
            pipeline_handle,
        }
    }
}

// ─── Task principale du pipeline ─────────────────────────────────────────────

async fn run_pipeline(
    segment_tx: mpsc::Sender<Arc<AudioSegment>>,
    mut control_rx: mpsc::Receiver<PipelineControl>,
    control_tx: mpsc::Sender<PipelineControl>,
    stop_token: CancellationToken,
    state: SharedState,
    udn: String,
    #[cfg(feature = "pmoserver")]
    control_point: Arc<pmocontrol::ControlPoint>,
) {
    let mut current_source_stop: Option<CancellationToken> = None;
    let mut current_uri: Option<String> = None;

    loop {
        tokio::select! {
            _ = stop_token.cancelled() => {
                info!(udn = %udn, "Pipeline stopping by cancellation");
                if let Some(src_stop) = current_source_stop.take() {
                    src_stop.cancel();
                }
                break;
            }

            cmd = control_rx.recv() => {
                match cmd {
                    None => {
                        info!(udn = %udn, "Pipeline control channel closed");
                        break;
                    }

                    Some(PipelineControl::SourceEnded) => {
                        // La source s'est terminée (EOF ou erreur) : libérer le slot
                        debug!(udn = %udn, "Pipeline: SourceEnded — source slot freed");
                        current_source_stop = None;
                    }

                    Some(PipelineControl::LoadUri(uri)) => {
                        info!(udn = %udn, uri = %uri, "Pipeline: LoadUri");
                        if let Some(src_stop) = current_source_stop.take() {
                            src_stop.cancel();
                        }
                        current_uri = Some(uri.clone());
                        {
                            let mut s = state.write();
                            s.playback_state = PlaybackState::Transitioning;
                            s.current_uri = Some(uri.clone());
                            s.position = None;
                        }
                        let src_stop = stop_token.child_token();
                        current_source_stop = Some(src_stop.clone());
                        let tx = segment_tx.clone();
                        let notify = control_tx.clone();
                        let st = state.clone();
                        let udn_c = udn.clone();
                        #[cfg(feature = "pmoserver")]
                        let cp = control_point.clone();
                        tokio::spawn(async move {
                            stream_source(
                                uri, 0.0, tx, src_stop, st, udn_c,
                                #[cfg(feature = "pmoserver")]
                                cp,
                            ).await;
                            let _ = notify.send(PipelineControl::SourceEnded).await;
                        });
                    }

                    Some(PipelineControl::LoadNextUri(uri)) => {
                        debug!(udn = %udn, uri = %uri, "Pipeline: LoadNextUri");
                        state.write().next_uri = Some(uri);
                    }

                    Some(PipelineControl::Play) => {
                        // Redémarrer la source si elle n'est pas en cours
                        if current_source_stop.is_none() {
                            if let Some(uri) = current_uri.clone() {
                                info!(udn = %udn, uri = %uri, "Pipeline: Play — restarting source");
                                let src_stop = stop_token.child_token();
                                current_source_stop = Some(src_stop.clone());
                                let tx = segment_tx.clone();
                                let notify = control_tx.clone();
                                let st = state.clone();
                                let udn_c = udn.clone();
                                #[cfg(feature = "pmoserver")]
                                let cp = control_point.clone();
                                tokio::spawn(async move {
                                    stream_source(
                                        uri, 0.0, tx, src_stop, st, udn_c,
                                        #[cfg(feature = "pmoserver")]
                                        cp,
                                    ).await;
                                    let _ = notify.send(PipelineControl::SourceEnded).await;
                                });
                            } else {
                                debug!(udn = %udn, "Pipeline: Play — no URI loaded, ignoring");
                            }
                        } else {
                            debug!(udn = %udn, "Pipeline: Play — source already running");
                        }
                    }

                    Some(PipelineControl::Pause) => {
                        debug!(udn = %udn, "Pipeline: Pause (not supported on live stream)");
                    }

                    Some(PipelineControl::Stop) => {
                        info!(udn = %udn, "Pipeline: Stop");
                        if let Some(src_stop) = current_source_stop.take() {
                            src_stop.cancel();
                        }
                        // Conserver current_uri pour permettre un Play ultérieur
                        let mut s = state.write();
                        s.playback_state = PlaybackState::Stopped;
                        s.position = None;
                    }

                    Some(PipelineControl::Seek(pos_sec)) => {
                        info!(udn = %udn, pos = pos_sec, "Pipeline: Seek");
                        if let Some(uri) = current_uri.clone() {
                            if let Some(src_stop) = current_source_stop.take() {
                                src_stop.cancel();
                            }
                            let src_stop = stop_token.child_token();
                            current_source_stop = Some(src_stop.clone());
                            let tx = segment_tx.clone();
                            let notify = control_tx.clone();
                            let st = state.clone();
                            let udn_c = udn.clone();
                            #[cfg(feature = "pmoserver")]
                            let cp = control_point.clone();
                            tokio::spawn(async move {
                                stream_source(
                                    uri, pos_sec, tx, src_stop, st, udn_c,
                                    #[cfg(feature = "pmoserver")]
                                    cp,
                                ).await;
                                let _ = notify.send(PipelineControl::SourceEnded).await;
                            });
                        }
                    }

                    Some(PipelineControl::SetVolume(vol)) => {
                        state.write().volume = vol;
                    }

                    Some(PipelineControl::SetMute(mute)) => {
                        state.write().mute = mute;
                    }
                }
            }
        }
    }
}

// ─── Task source ─────────────────────────────────────────────────────────────

async fn stream_source(
    uri: String,
    seek_sec: f64,
    tx: mpsc::Sender<Arc<AudioSegment>>,
    stop_token: CancellationToken,
    state: SharedState,
    udn: String,
    #[cfg(feature = "pmoserver")]
    control_point: Arc<pmocontrol::ControlPoint>,
) {
    info!(udn = %udn, uri = %uri, seek = seek_sec, "Source task: opening URI");

    match UriSource::open(&uri, seek_sec, stop_token.clone()).await {
        Ok(source) => {
            debug!(udn = %udn, "Source task: URI opened, duration={:?}", source.duration_sec());
            if let Some(dur) = source.duration_sec() {
                state.write().duration = Some(seconds_to_upnp_time(dur));
            }

            // TrackBoundary avec métadonnées minimales
            let boundary = {
                let mut meta = MemoryTrackMetadata::new();
                let _ = meta.set_title(Some(uri.clone())).await;
                let meta_arc = Arc::new(tokio::sync::RwLock::new(meta));
                AudioSegment::new_track_boundary(0, seek_sec, meta_arc)
            };
            let _ = tx.send(boundary).await;

            // L'URI est ouverte, le flux va commencer à couler.
            // Le sink bloquera naturellement si aucun client HTTP n'est connecté.
            {
                let mut s = state.write();
                s.playback_state = PlaybackState::Playing;
                if seek_sec > 0.0 {
                    s.position = Some(seconds_to_upnp_time(seek_sec));
                }
            }

            match source.emit_to_channel(&tx, &stop_token).await {
                Ok(true) => {
                    debug!(udn = %udn, "Source task: EOF → TrackEnded");
                    handle_track_ended(
                        state, udn, tx, stop_token,
                        #[cfg(feature = "pmoserver")]
                        control_point,
                    ).await;
                }
                Ok(false) => {
                    debug!(udn = %udn, "Source task: cancelled");
                }
                Err(e) => {
                    warn!(udn = %udn, error = %e, "Source task error");
                    state.write().playback_state = PlaybackState::Stopped;
                }
            }
        }
        Err(e) => {
            warn!(udn = %udn, error = %e, "Source task: failed to open URI");
            state.write().playback_state = PlaybackState::Stopped;
        }
    }
}

// ─── TrackEnded : avancer current←next ───────────────────────────────────────

async fn handle_track_ended(
    state: SharedState,
    udn: String,
    tx: mpsc::Sender<Arc<AudioSegment>>,
    stop_token: CancellationToken,
    #[cfg(feature = "pmoserver")]
    control_point: Arc<pmocontrol::ControlPoint>,
) {
    let next_uri = {
        let mut s = state.write();
        let uri = s.next_uri.take();
        let meta = s.next_metadata.take();
        s.current_uri = uri.clone();
        s.current_metadata = meta;
        s.next_uri = None;
        s.next_metadata = None;
        s.position = None;
        s.duration = None;
        if uri.is_some() {
            s.playback_state = PlaybackState::Playing;
        } else {
            s.playback_state = PlaybackState::Stopped;
        }
        uri
    };

    #[cfg(feature = "pmoserver")]
    if next_uri.is_some() {
        let cp = control_point.clone();
        let udn_clone = udn.clone();
        tokio::spawn(async move {
            cp.advance_queue_and_prefetch(&pmocontrol::DeviceId(udn_clone));
        });
    }

    if let Some(uri) = next_uri {
        info!(udn = %udn, uri = %uri, "TrackEnded: starting next track");
        Box::pin(stream_source(
            uri, 0.0, tx, stop_token, state, udn,
            #[cfg(feature = "pmoserver")]
            control_point,
        )).await;
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
