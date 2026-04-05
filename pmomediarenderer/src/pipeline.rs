//! Pipeline audio serveur par instance MediaRenderer
//!
//! Chaque instance MediaRenderer possède un pipeline独立的音频处理：
//! - 一个 `PlayerSource` 管理 AVTransport 生命周期（Play/Pause/Stop/Seek/LoadUri）
//! - 一个 `StreamingOggFlacSink` 编码并向 HTTP 客户端传输 OGG-FLAC 流
//! - 规范化节点（重采样 → 96 kHz，转换 → I24）

use std::sync::Arc;
use pmoaudio::{ResamplingNode, ToI24Node};
use pmoaudio_ext::{PlayerCommand, PlayerHandle, PlayerSource};
use pmoaudio_ext::sinks::{OggFlacStreamHandle, StreamingOggFlacSink};
use pmoflac::EncoderOptions;
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};

use crate::state::SharedState;

// ─── Ré-export des commandes pour les handlers ────────────────────────────────

pub use pmoaudio_ext::PlayerCommand as PipelineControl;

// ─── Handle vers le pipeline ─────────────────────────────────────────────────

#[derive(Clone)]
pub struct PipelineHandle {
    pub player: PlayerHandle,
    pub stop_token: CancellationToken,
    pub flac_handle: pmoaudio_ext::sinks::OggFlacStreamHandle,
    pub adapter: Arc<dyn crate::adapter::DeviceAdapter>,
    #[allow(dead_code)]
    state: SharedState,
}

impl PipelineHandle {
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

pub struct InstancePipeline {
    pub flac_handle: OggFlacStreamHandle,
    pub pipeline_handle: PipelineHandle,
}

impl InstancePipeline {
    pub fn start(
        state: SharedState,
        #[cfg(feature = "pmoserver")]
        control_point: Arc<pmocontrol::ControlPoint>,
        udn: String,
        adapter: Arc<dyn crate::adapter::DeviceAdapter>,
    ) -> Self {
        let stop_token = CancellationToken::new();

        use pmoaudio::pipeline::AudioPipelineNode;

        let (sink, flac_handle) = StreamingOggFlacSink::new(EncoderOptions::default(), 24);

        let mut to_i24 = ToI24Node::new();
        to_i24.register(sink.boxed());

        let mut resampler = ResamplingNode::new(96_000);
        resampler.register(to_i24.boxed());

        let (mut player_source, player_handle) = PlayerSource::new();
        player_source.register(resampler.boxed());

        let sink_stop = stop_token.clone();
        tokio::spawn(async move {
            if let Err(e) = player_source.boxed().run(sink_stop).await {
                warn!("Audio pipeline error: {:?}", e);
            }
            debug!("Pipeline task terminated");
        });

        let event_rx = player_handle.subscribe_events();
        let state_clone = state.clone();
        let udn_clone = udn.clone();
        let adapter_clone = Arc::downgrade(&adapter);
        #[cfg(feature = "pmoserver")]
        let cp_clone = control_point.clone();
        tokio::spawn(async move {
            run_event_listener(
                event_rx,
                state_clone,
                adapter_clone,
                udn_clone,
                #[cfg(feature = "pmoserver")]
                cp_clone,
            ).await;
        });

        let pipeline_handle = PipelineHandle {
            player: player_handle,
            stop_token: stop_token.clone(),
            flac_handle: flac_handle.clone(),
            adapter,
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
    adapter: std::sync::Weak<dyn crate::adapter::DeviceAdapter>,
    udn: String,
    #[cfg(feature = "pmoserver")]
    control_point: Arc<pmocontrol::ControlPoint>,
) {
    use pmoaudio_ext::PlayerEvent;
    use crate::messages::PlaybackState;
    use crate::adapter::DeviceCommand;

    loop {
        match event_rx.recv().await {
            Ok(event) => match event {
                PlayerEvent::Playing { uri, duration_sec } => {
                    let mut s = state.write();
                    s.playback_state = PlaybackState::Playing;
                    s.current_uri = Some(uri);
                    s.duration = duration_sec.map(seconds_to_upnp_time);
                    s.position = None;
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
                    state.write().playback_state = PlaybackState::Transitioning;
                    if let Some(adapter) = adapter.upgrade() {
                        adapter.deliver(DeviceCommand::Flush);
                    }
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