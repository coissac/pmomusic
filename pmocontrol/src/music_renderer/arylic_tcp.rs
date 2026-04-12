use std::sync::{atomic::AtomicBool, Arc, Mutex};
use std::time::Duration;

use serde::Deserialize;
use tracing::debug;

use crate::arylic_client::{
    send_command_no_response, send_command_optional, send_command_required, ARYLIC_TCP_PORT,
    DEFAULT_TIMEOUT_SECS,
};
use crate::errors::ControlPointError;
use crate::linkplay_client::extract_linkplay_host;
use crate::model::{PlaybackState, RendererInfo};
use crate::music_renderer::capabilities::{
    HasContinuousStream, PlaybackPosition, PlaybackPositionInfo, PlaybackStatus,
    QueueTransportControl, TransportControl, VolumeControl,
};
use crate::music_renderer::musicrenderer::MusicRendererBackend;
use crate::music_renderer::time_utils::{format_hhmmss, ms_to_seconds, parse_hhmmss_strict};
use crate::music_renderer::HasQueue;
use crate::music_renderer::RendererFromMediaRendererInfo;
use crate::queue::{EnqueueMode, MusicQueue, PlaybackItem, QueueBackend, QueueSnapshot};
use crate::DeviceIdentity;

/// Raw response from Arylic MCU+PINFGET command
#[derive(Debug, Deserialize)]
struct ArylicPlaybackInfoRaw {
    status: String,
    curpos: String,
    totlen: String,
    #[serde(default)]
    vol: Option<String>,
    #[serde(default)]
    mute: Option<String>,
    #[serde(default)]
    plicount: Option<String>,
    #[serde(default)]
    plicurr: Option<String>,
}

/// Backend speaking the Arylic TCP control protocol (port 8899).
#[derive(Clone, Debug)]
pub struct ArylicTcpRenderer {
    host: String,
    port: u16,
    timeout: Duration,
    queue: Arc<Mutex<MusicQueue>>,
    /// Flag indicating if currently playing a continuous stream (radio without duration)
    continuous_stream: Arc<Mutex<bool>>,
}

impl ArylicTcpRenderer {
    fn send_required(&self, cmd: &str, expected: &[&str]) -> Result<String, ControlPointError> {
        send_command_required(&self.host, self.port, self.timeout, cmd, expected)
    }

    fn send_optional(
        &self,
        cmd: &str,
        expected: &[&str],
    ) -> Result<Option<String>, ControlPointError> {
        send_command_optional(&self.host, self.port, self.timeout, cmd, expected)
    }

    fn send_no_response(&self, cmd: &str) -> Result<(), ControlPointError> {
        send_command_no_response(&self.host, self.port, self.timeout, cmd)
    }

    fn fetch_playback_info(&self) -> Result<ArylicPlaybackInfo, ControlPointError> {
        let payload = self.send_required("MCU+PINFGET", &["AXX+PLY+INF"])?;
        match parse_playback_info(&payload) {
            Ok(info) => Ok(info),
            Err(err) => {
                debug!(
                    "Failed to parse Arylic playback info for {}: {}",
                    self.host, err
                );
                Err(err)
            }
        }
    }

    fn format_volume_command(value: u16) -> String {
        format!("MCU+VOL+{:03}", value.min(100))
    }

    fn parse_volume_payload(payload: &str) -> Result<u16, ControlPointError> {
        let data = payload.strip_prefix("AXX+VOL+").ok_or_else(|| {
            ControlPointError::ArilycTcpError(format!("Unexpected volume response: {}", payload))
        })?;
        let value: u16 = data.trim().parse().map_err(|_| {
            ControlPointError::ArilycTcpError(format!("Invalid volume value: {}", data))
        })?;
        Ok(value.min(100))
    }

    fn parse_mute_payload(payload: &str) -> Result<bool, ControlPointError> {
        let data = payload.strip_prefix("AXX+MUT+").ok_or_else(|| {
            ControlPointError::ArilycTcpError(format!("Unexpected mute response: {}", payload))
        })?;
        match data.trim() {
            "000" | "0" => Ok(false),
            "001" | "1" => Ok(true),
            other => Err(ControlPointError::ArilycTcpError(format!(
                "Invalid mute value: {}",
                other
            ))),
        }
    }
}

impl RendererFromMediaRendererInfo for ArylicTcpRenderer {
    fn from_renderer_info(info: &RendererInfo) -> Result<Self, ControlPointError> {
        let host = extract_linkplay_host(info.location()).ok_or_else(|| {
            ControlPointError::ArilycTcpError(format!(
                "Renderer {} has no valid LOCATION host",
                info.udn()
            ))
        })?;

        let queue = Arc::new(Mutex::new(MusicQueue::from_renderer_info(info)?));

        Ok(Self {
            host,
            port: ARYLIC_TCP_PORT,
            timeout: Duration::from_secs(DEFAULT_TIMEOUT_SECS),
            queue,
            continuous_stream: Arc::new(Mutex::new(false)),
        })
    }

    fn to_backend(self) -> MusicRendererBackend {
        MusicRendererBackend::ArylicTcp(self)
    }
}

impl ArylicTcpRenderer {
    /// Returns true if currently playing a continuous stream (radio without duration)
    pub fn is_continuous_stream(&self) -> bool {
        *self.continuous_stream.lock().expect("continuous_stream mutex poisoned")
    }

    /// Create an ArylicTcpRenderer with a shared queue (for HybridUpnpArylic)
    pub fn with_shared_queue(
        info: &RendererInfo,
        shared_queue: Arc<Mutex<MusicQueue>>,
    ) -> Result<Self, ControlPointError> {
        let host = extract_linkplay_host(info.location()).ok_or_else(|| {
            ControlPointError::ArilycTcpError(format!(
                "Renderer {} has no valid LOCATION host",
                info.udn()
            ))
        })?;

        Ok(Self {
            host,
            port: ARYLIC_TCP_PORT,
            timeout: Duration::from_secs(DEFAULT_TIMEOUT_SECS),
            queue: shared_queue,
            continuous_stream: Arc::new(Mutex::new(false)),
        })
    }
}

impl TransportControl for ArylicTcpRenderer {
    fn play_uri(&self, _uri: &str, _meta: &str) -> Result<(), ControlPointError> {
        Err(ControlPointError::upnp_operation_not_supported(
            "Arylic TCP backend does not support direct URL loading.",
            "ArylicTcpRenderer",
        ))
    }

    fn play(&self) -> Result<(), ControlPointError> {
        self.send_no_response("MCU+PLY-PLA")
    }

    fn pause(&self) -> Result<(), ControlPointError> {
        let _ = self.send_optional("MCU+PLY-PUS", &["AXX+PLY+"])?;
        Ok(())
    }

    fn stop(&self) -> Result<(), ControlPointError> {
        self.send_no_response("MCU+PLY-STP")
    }

    fn seek_rel_time(&self, hhmmss: &str) -> Result<(), ControlPointError> {
        let _ = parse_hhmmss_strict(hhmmss)?;
        Err(ControlPointError::ArilycTcpError(
            "Arylic TCP seek_rel_time is not implemented yet for this device.".to_string(),
        ))
    }
}

impl VolumeControl for ArylicTcpRenderer {
    fn volume(&self) -> Result<u16, ControlPointError> {
        if let Ok(info) = self.fetch_playback_info() {
            if let Some(vol) = info.volume {
                return Ok(vol);
            }
            debug!(
                "Arylic playback info for {} missing volume, falling back to VOL GET",
                self.host
            );
        }
        let payload = self.send_required("MCU+VOL+GET", &["AXX+VOL+"])?;
        Self::parse_volume_payload(&payload)
    }

    fn set_volume(&self, v: u16) -> Result<(), ControlPointError> {
        let command = Self::format_volume_command(v);
        let _ = self.send_optional(&command, &["AXX+VOL+"])?;
        Ok(())
    }

    fn mute(&self) -> Result<bool, ControlPointError> {
        if let Ok(info) = self.fetch_playback_info() {
            if let Some(mute) = info.mute {
                return Ok(mute);
            }
            debug!(
                "Arylic playback info for {} missing mute, falling back to MUT GET",
                self.host
            );
        }
        let payload = self.send_required("MCU+MUT+GET", &["AXX+MUT+"])?;
        Self::parse_mute_payload(&payload)
    }

    fn set_mute(&self, m: bool) -> Result<(), ControlPointError> {
        let command = if m { "MCU+MUT+001" } else { "MCU+MUT+000" };
        let payload = self.send_required(command, &["AXX+MUT+"])?;
        let _ = Self::parse_mute_payload(&payload)?;
        Ok(())
    }
}

impl PlaybackStatus for ArylicTcpRenderer {
    fn playback_state(&self) -> Result<PlaybackState, ControlPointError> {
        let info = self.fetch_playback_info()?;
        Ok(info.playback_state())
    }
}

impl PlaybackPosition for ArylicTcpRenderer {
    fn playback_position(&self) -> Result<PlaybackPositionInfo, ControlPointError> {
        let info = match self.fetch_playback_info() {
            Ok(info) => {
                tracing::debug!("ArylicTcp fetch_playback_info returned: {:?}", info);
                info
            }
            Err(e) => {
                tracing::warn!("ArylicTcp fetch_playback_info failed: {}", e);
                return Err(e);
            }
        };

        let mut position_info = info.position_info();
        tracing::debug!(
            "ArylicTcp position_info: track_duration={:?}, rel_time={:?}, track_metadata={:?}, track_uri={:?}",
            position_info.track_duration,
            position_info.rel_time,
            position_info
                .track_metadata
                .as_ref()
                .map(|s| &s[..s.len().min(100)]),
            position_info.track_uri
        );

        // Récupérer les métadonnées depuis la queue (avec protection contre diminution de durée)
        // Normalement current_index est toujours Some() si la queue n'est pas vide (règle métier)
        let mut queue_guard = self.queue.lock().expect("queue mutex poisoned");
        let queue_item = queue_guard.peek_current().ok().flatten();

        if let Some((current_item, _)) = queue_item {
            // Build DIDL metadata XML from cached/protected TrackMetadata
            if let Some(ref metadata) = current_item.metadata {
                tracing::debug!(
                    "ArylicTcp playback_position: using queue metadata - title={:?}, artist={:?}, duration={:?}, is_stream={}",
                    metadata.title,
                    metadata.artist,
                    metadata.duration,
                    metadata.is_continuous_stream
                );
                position_info.track_metadata = Some(
                    crate::music_renderer::musicrenderer::build_didl_lite_metadata(
                        metadata,
                        &current_item.uri,
                        &current_item.protocol_info,
                    ),
                );
            } else {
                tracing::warn!("ArylicTcp playback_position: queue item has no metadata");
            }
            position_info.track_uri = Some(current_item.uri.clone());
        } else {
            tracing::warn!("ArylicTcp playback_position: no current queue item");
        }
        drop(queue_guard);

        Ok(position_info)
    }
}


impl QueueTransportControl for ArylicTcpRenderer {
    fn play_item(&self, item: &PlaybackItem) -> Result<(), ControlPointError> {
        self.play_uri(&item.uri, "")
    }

}

impl HasQueue for ArylicTcpRenderer {
    fn queue(&self) -> &Arc<Mutex<MusicQueue>> {
        &self.queue
    }
}

impl HasContinuousStream for ArylicTcpRenderer {
    fn continuous_stream(&self) -> &Arc<Mutex<bool>> {
        &self.continuous_stream
    }
}

#[derive(Debug)]
struct ArylicPlaybackInfo {
    status_raw: String,
    curpos_ms: u64,
    totlen_ms: u64,
    volume: Option<u16>,
    mute: Option<bool>,
    playlist_size: Option<u32>,
    track_index: Option<u32>,
}

impl ArylicPlaybackInfo {
    fn playback_state(&self) -> PlaybackState {
        match self.status_raw.as_str() {
            "play" => PlaybackState::Playing,
            "pause" => PlaybackState::Paused,
            "stop" => PlaybackState::Stopped,
            other => PlaybackState::Unknown(other.to_string()),
        }
    }

    fn position_info(&self) -> PlaybackPositionInfo {
        let track = match (self.track_index, self.playlist_size) {
            (Some(idx), Some(count)) if count > 0 => Some(idx.min(count)),
            (Some(idx), _) => Some(idx),
            _ => None,
        };

        PlaybackPositionInfo {
            track,
            rel_time: Some(format_hhmmss(ms_to_seconds(self.curpos_ms))),
            abs_time: None,
            track_duration: if self.totlen_ms > 0 {
                Some(format_hhmmss(ms_to_seconds(self.totlen_ms)))
            } else {
                None
            },
            track_metadata: None,
            track_uri: None,
        }
    }
}

fn parse_playback_info(payload: &str) -> Result<ArylicPlaybackInfo, ControlPointError> {
    let json_blob = payload.strip_prefix("AXX+PLY+INF").ok_or_else(|| {
        ControlPointError::ArilycTcpError(format!("Unexpected playback info prefix: {}", payload))
    })?;

    // Extract JSON object between { and }, ignoring trailing & and other garbage
    // Arylic devices send: AXX+PLY+INF{...json...}&
    let json_start = json_blob.find('{').ok_or_else(|| {
        ControlPointError::ArilycTcpError(format!(
            "No JSON object found in playback info: {}",
            payload
        ))
    })?;

    let json_end = json_blob.rfind('}').ok_or_else(|| {
        ControlPointError::ArilycTcpError(format!(
            "No JSON object end found in playback info: {}",
            payload
        ))
    })?;

    let json_blob = &json_blob[json_start..=json_end];

    let raw: ArylicPlaybackInfoRaw = serde_json::from_str(json_blob).map_err(|e| {
        ControlPointError::ArilycTcpError(format!(
            "Failed to parse Arylic playback info JSON: {}",
            e
        ))
    })?;

    let curpos_ms = raw.curpos.parse::<u64>().map_err(|_| {
        ControlPointError::ArilycTcpError(format!("Invalid curpos value: {}", raw.curpos))
    })?;

    let totlen_ms = raw.totlen.parse::<u64>().map_err(|_| {
        ControlPointError::ArilycTcpError(format!("Invalid totlen value: {}", raw.totlen))
    })?;

    let volume = raw.vol.and_then(|raw_vol| match raw_vol.parse::<u16>() {
        Ok(value) => Some(value.min(100)),
        Err(err) => {
            debug!("Invalid Arylic `vol` value {}: {}", raw_vol, err);
            None
        }
    });

    let mute = raw.mute.and_then(|raw_mute| match raw_mute.as_str() {
        "1" => Some(true),
        "0" => Some(false),
        other => {
            debug!("Invalid Arylic `mute` value {}", other);
            None
        }
    });

    let playlist_size = raw
        .plicount
        .and_then(|raw_count| match raw_count.parse::<u32>() {
            Ok(count) if count > 0 => Some(count),
            Ok(_) => None,
            Err(err) => {
                debug!("Invalid Arylic `plicount` value {}: {}", raw_count, err);
                None
            }
        });

    let track_index = raw
        .plicurr
        .and_then(|raw_idx| match raw_idx.parse::<u32>() {
            Ok(idx) if idx > 0 => Some(idx),
            Ok(_) => None,
            Err(err) => {
                debug!("Invalid Arylic `plicurr` value {}: {}", raw_idx, err);
                None
            }
        });

    Ok(ArylicPlaybackInfo {
        status_raw: raw.status,
        curpos_ms,
        totlen_ms,
        volume,
        mute,
        playlist_size,
        track_index,
    })
}
