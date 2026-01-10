use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Deserialize;
use tracing::debug;

use crate::DeviceIdentity;
use crate::arylic_client::{
    ARYLIC_TCP_PORT, DEFAULT_TIMEOUT_SECS, send_command_no_response, send_command_optional,
    send_command_required,
};
use crate::errors::ControlPointError;
use crate::linkplay_client::extract_linkplay_host;
use crate::model::{PlaybackState, RendererInfo};
use crate::music_renderer::RendererFromMediaRendererInfo;
use crate::music_renderer::capabilities::{
    PlaybackPosition, PlaybackPositionInfo, PlaybackStatus, QueueTransportControl, RendererBackend,
    TransportControl, VolumeControl,
};
use crate::music_renderer::musicrenderer::MusicRendererBackend;
use crate::music_renderer::time_utils::{format_hhmmss, ms_to_seconds, parse_hhmmss_strict};
use crate::queue::MusicQueue;
use crate::queue::{EnqueueMode, PlaybackItem, QueueBackend, QueueSnapshot};

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
        let info = self.fetch_playback_info()?;
        Ok(info.position_info())
    }
}

impl RendererBackend for ArylicTcpRenderer {
    fn queue(&self) -> &Arc<Mutex<MusicQueue>> {
        &self.queue
    }
}

impl QueueTransportControl for ArylicTcpRenderer {
    fn play_from_queue(&self) -> Result<(), ControlPointError> {
        let mut queue = self.queue.lock().unwrap();

        let current_index = match queue.current_index()? {
            Some(idx) => idx,
            None => {
                if queue.len()? > 0 {
                    queue.set_index(Some(0))?;
                    0
                } else {
                    return Err(ControlPointError::QueueError("Queue is empty".into()));
                }
            }
        };

        let item = queue
            .get_item(current_index)?
            .ok_or_else(|| ControlPointError::QueueError("Current item not found".into()))?;

        let uri = item.uri.clone();
        drop(queue);

        self.play_uri(&uri, "")
    }

    fn play_next(&self) -> Result<(), ControlPointError> {
        {
            let mut queue = self.queue.lock().unwrap();
            if !queue.advance()? {
                return Err(ControlPointError::QueueError("No next track".into()));
            }
        }

        self.play_from_queue()
    }

    fn play_previous(&self) -> Result<(), ControlPointError> {
        {
            let mut queue = self.queue.lock().unwrap();
            if !queue.rewind()? {
                return Err(ControlPointError::QueueError("No previous track".into()));
            }
        }

        self.play_from_queue()
    }

    fn play_from_index(&self, index: usize) -> Result<(), ControlPointError> {
        {
            let mut queue = self.queue.lock().unwrap();
            queue.set_index(Some(index))?;
        }

        self.play_from_queue()
    }
}

impl QueueBackend for ArylicTcpRenderer {
    fn len(&self) -> Result<usize, ControlPointError> {
        self.queue.lock().unwrap().len()
    }

    fn track_ids(&self) -> Result<Vec<u32>, ControlPointError> {
        self.queue.lock().unwrap().track_ids()
    }

    fn id_to_position(&self, id: u32) -> Result<usize, ControlPointError> {
        self.queue.lock().unwrap().id_to_position(id)
    }

    fn position_to_id(&self, id: usize) -> Result<u32, ControlPointError> {
        self.queue.lock().unwrap().position_to_id(id)
    }

    fn current_track(&self) -> Result<Option<u32>, ControlPointError> {
        self.queue.lock().unwrap().current_track()
    }

    fn current_index(&self) -> Result<Option<usize>, ControlPointError> {
        self.queue.lock().unwrap().current_index()
    }

    fn queue_snapshot(&self) -> Result<QueueSnapshot, ControlPointError> {
        self.queue.lock().unwrap().queue_snapshot()
    }

    fn set_index(&mut self, index: Option<usize>) -> Result<(), ControlPointError> {
        self.queue.lock().unwrap().set_index(index)
    }

    fn replace_queue(
        &mut self,
        items: Vec<PlaybackItem>,
        current_index: Option<usize>,
    ) -> Result<(), ControlPointError> {
        self.queue
            .lock()
            .unwrap()
            .replace_queue(items, current_index)
    }

    fn sync_queue(&mut self, items: Vec<PlaybackItem>) -> Result<(), ControlPointError> {
        self.queue.lock().unwrap().sync_queue(items)
    }

    fn get_item(&self, index: usize) -> Result<Option<PlaybackItem>, ControlPointError> {
        self.queue.lock().unwrap().get_item(index)
    }

    fn replace_item(&mut self, index: usize, item: PlaybackItem) -> Result<(), ControlPointError> {
        self.queue.lock().unwrap().replace_item(index, item)
    }

    fn enqueue_items(
        &mut self,
        items: Vec<PlaybackItem>,
        mode: EnqueueMode,
    ) -> Result<(), ControlPointError> {
        self.queue.lock().unwrap().enqueue_items(items, mode)
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
