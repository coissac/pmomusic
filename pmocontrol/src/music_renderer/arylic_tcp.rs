use std::time::Duration;

use serde::Deserialize;
use tracing::debug;

use crate::DeviceIdentity;
use crate::arylic_client::{ARYLIC_TCP_PORT, DEFAULT_TIMEOUT_SECS, send_command_no_response, send_command_optional, send_command_required};
use crate::errors::ControlPointError;
use crate::linkplay_client::extract_linkplay_host;
use crate::model::{PlaybackState, RendererInfo};
use crate::music_renderer::RendererFromMediaRendererInfo;
use crate::music_renderer::capabilities::{
    PlaybackPosition, PlaybackPositionInfo, PlaybackStatus, TransportControl,
    VolumeControl,
};
use crate::music_renderer::time_utils::{ms_to_seconds, format_hhmmss, parse_hhmmss_strict};
use crate::music_renderer::musicrenderer::MusicRendererBackend;

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

        Ok(Self {
            host,
            port: ARYLIC_TCP_PORT,
            timeout: Duration::from_secs(DEFAULT_TIMEOUT_SECS),
        })
    }

    fn to_backend(self) -> MusicRendererBackend {
        MusicRendererBackend::ArylicTcp(self)
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
            "Arylic TCP seek_rel_time is not implemented yet for this device.".to_string()
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
        let info = self.fetch_playback_info()?;
        Ok(info.position_info())
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

    let json_blob = json_blob.trim_end_matches('&').trim();

    let raw: ArylicPlaybackInfoRaw = serde_json::from_str(json_blob)
        .map_err(|e| ControlPointError::ArilycTcpError(format!("Failed to parse Arylic playback info JSON: {}", e)))?;

    let curpos_ms = raw.curpos.parse::<u64>()
        .map_err(|_| ControlPointError::ArilycTcpError(format!("Invalid curpos value: {}", raw.curpos)))?;

    let totlen_ms = raw.totlen.parse::<u64>()
        .map_err(|_| ControlPointError::ArilycTcpError(format!("Invalid totlen value: {}", raw.totlen)))?;

    let volume = raw.vol.and_then(|raw_vol| {
        match raw_vol.parse::<u16>() {
            Ok(value) => Some(value.min(100)),
            Err(err) => {
                debug!("Invalid Arylic `vol` value {}: {}", raw_vol, err);
                None
            }
        }
    });

    let mute = raw.mute.and_then(|raw_mute| {
        match raw_mute.as_str() {
            "1" => Some(true),
            "0" => Some(false),
            other => {
                debug!("Invalid Arylic `mute` value {}", other);
                None
            }
        }
    });

    let playlist_size = raw.plicount.and_then(|raw_count| {
        match raw_count.parse::<u32>() {
            Ok(count) if count > 0 => Some(count),
            Ok(_) => None,
            Err(err) => {
                debug!("Invalid Arylic `plicount` value {}: {}", raw_count, err);
                None
            }
        }
    });

    let track_index = raw.plicurr.and_then(|raw_idx| {
        match raw_idx.parse::<u32>() {
            Ok(idx) if idx > 0 => Some(idx),
            Ok(_) => None,
            Err(err) => {
                debug!("Invalid Arylic `plicurr` value {}: {}", raw_idx, err);
                None
            }
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
