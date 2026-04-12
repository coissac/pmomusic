use std::fmt;
use std::sync::{atomic::AtomicBool, Arc, Mutex};
use std::time::Duration;

use ureq::Agent;

use crate::errors::ControlPointError;
use crate::linkplay_client::{
    build_agent, extract_linkplay_host, fetch_status_for_host, percent_encode, LinkPlayStatus,
};
use crate::model::{PlaybackState, RendererInfo};
use crate::music_renderer::capabilities::{
    HasContinuousStream, PlaybackPosition, PlaybackPositionInfo, PlaybackStatus,
    QueueTransportControl, TransportControl, VolumeControl,
};
use crate::music_renderer::musicrenderer::MusicRendererBackend;
use crate::music_renderer::time_utils::parse_hhmmss_strict;
use crate::music_renderer::HasQueue;
use crate::music_renderer::RendererFromMediaRendererInfo;
use crate::queue::{EnqueueMode, MusicQueue, PlaybackItem, QueueBackend};
use crate::DeviceIdentity;

const DEFAULT_HTTP_TIMEOUT_SECS: u64 = 3;

/// Renderer backend for devices exposing the LinkPlay HTTP API.
#[derive(Clone)]
pub struct LinkPlayRenderer {
    host: String,
    timeout: Duration,
    queue: Arc<Mutex<MusicQueue>>,
    /// Flag indicating if currently playing a continuous stream (radio without duration)
    continuous_stream: Arc<Mutex<bool>>,
}

impl fmt::Debug for LinkPlayRenderer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LinkPlayRenderer")
            .field("host", &self.host)
            .finish()
    }
}

impl LinkPlayRenderer {
    fn agent(&self) -> Agent {
        build_agent(self.timeout)
    }

    fn send_player_command(&self, command: &str) -> Result<(), ControlPointError> {
        let url = format!(
            "http://{}/httpapi.asp?command=setPlayerCmd:{}",
            self.host, command
        );
        self.agent().get(&url).call().map_err(|_| {
            ControlPointError::ArilycTcpError(format!(
                "LinkPlay command {} failed for {}",
                command, self.host
            ))
        })?;
        Ok(())
    }

    fn fetch_status(&self) -> Result<LinkPlayStatus, ControlPointError> {
        fetch_status_for_host(&self.host, self.timeout)
    }
}

impl RendererFromMediaRendererInfo for LinkPlayRenderer {
    fn from_renderer_info(info: &RendererInfo) -> Result<Self, ControlPointError> {
        let host = extract_linkplay_host(&info.location()).ok_or_else(|| {
            ControlPointError::LinkPlayError(format!(
                "Renderer {} has no valid LOCATION host",
                info.udn()
            ))
        })?;

        let queue = Arc::new(Mutex::new(MusicQueue::from_renderer_info(info)?));

        Ok(Self {
            host,
            timeout: Duration::from_secs(DEFAULT_HTTP_TIMEOUT_SECS),
            queue,
            continuous_stream: Arc::new(Mutex::new(false)),
        })
    }

    fn to_backend(self) -> MusicRendererBackend {
        MusicRendererBackend::LinkPlay(self)
    }
}

impl LinkPlayRenderer {
    /// Returns true if currently playing a continuous stream (radio without duration)
    pub fn is_continuous_stream(&self) -> bool {
        *self.continuous_stream.lock().unwrap()
    }
}

impl TransportControl for LinkPlayRenderer {
    fn play_uri(&self, uri: &str, _meta: &str) -> Result<(), ControlPointError> {
        // Détecte si l'URL est un flux continu
        let is_stream = crate::music_renderer::is_continuous_stream_url(uri);
        *self.continuous_stream.lock().unwrap() = is_stream;
        tracing::debug!(
            "LinkPlayRenderer play_uri: URI={}, continuous_stream={}",
            uri,
            is_stream
        );

        let encoded = percent_encode(uri);
        self.send_player_command(&format!("play:{}", encoded))
    }

    fn play(&self) -> Result<(), ControlPointError> {
        self.send_player_command("resume")
    }

    fn pause(&self) -> Result<(), ControlPointError> {
        self.send_player_command("pause")
    }

    fn stop(&self) -> Result<(), ControlPointError> {
        self.send_player_command("stop")
    }

    fn seek_rel_time(&self, hhmmss: &str) -> Result<(), ControlPointError> {
        let secs = parse_hhmmss_strict(hhmmss)?;
        self.send_player_command(&format!("seek:{}", secs))
    }
}

impl VolumeControl for LinkPlayRenderer {
    fn volume(&self) -> Result<u16, ControlPointError> {
        Ok(self.fetch_status()?.volume)
    }

    fn set_volume(&self, v: u16) -> Result<(), ControlPointError> {
        let value = v.min(100);
        self.send_player_command(&format!("vol:{}", value))
    }

    fn mute(&self) -> Result<bool, ControlPointError> {
        Ok(self.fetch_status()?.mute)
    }

    fn set_mute(&self, m: bool) -> Result<(), ControlPointError> {
        self.send_player_command(if m { "mute:1" } else { "mute:0" })
    }
}

impl PlaybackStatus for LinkPlayRenderer {
    fn playback_state(&self) -> Result<PlaybackState, ControlPointError> {
        Ok(self.fetch_status()?.playback_state())
    }
}

impl PlaybackPosition for LinkPlayRenderer {
    fn playback_position(&self) -> Result<PlaybackPositionInfo, ControlPointError> {
        let mut position_info = self.fetch_status()?.position_info();

        // Use queue metadata instead of direct status metadata to benefit from duration protection
        let mut queue_guard = self.queue.lock().unwrap();
        let queue_item = queue_guard.peek_current().ok().flatten();

        if let Some((current_item, _)) = queue_item {
            if let Some(ref metadata) = current_item.metadata {
                position_info.track_metadata = Some(
                    crate::music_renderer::musicrenderer::build_didl_lite_metadata(
                        metadata,
                        &current_item.uri,
                        &current_item.protocol_info,
                    ),
                );
            }
            position_info.track_uri = Some(current_item.uri.clone());
        }
        drop(queue_guard);

        Ok(position_info)
    }
}


impl QueueTransportControl for LinkPlayRenderer {
    fn play_item(&self, item: &PlaybackItem) -> Result<(), ControlPointError> {
        self.play_uri(&item.uri, "")
    }
}


impl HasQueue for LinkPlayRenderer {
    fn queue(&self) -> &Arc<Mutex<MusicQueue>> {
        &self.queue
    }
}

impl HasContinuousStream for LinkPlayRenderer {
    fn continuous_stream(&self) -> &Arc<Mutex<bool>> {
        &self.continuous_stream
    }
}
