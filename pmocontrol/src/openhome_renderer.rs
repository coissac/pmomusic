use anyhow::{Result, anyhow};
use crate::capabilities::{
    PlaybackPosition, PlaybackPositionInfo, PlaybackState, PlaybackStatus, TransportControl,
    VolumeControl,
};
use crate::model::{RendererId, RendererInfo, RendererProtocol};
use crate::music_renderer::op_not_supported;
use crate::openhome_client::{
    OhInfoClient, OhPlaylistClient, OhRadioClient, OhTimeClient, OhVolumeClient,
};
use crate::registry::DeviceRegistry;
use tracing::debug;

#[derive(Clone, Debug)]
pub struct OpenHomeRenderer {
    pub info: RendererInfo,
    playlist: Option<OhPlaylistClient>,
    info_client: Option<OhInfoClient>,
    time_client: Option<OhTimeClient>,
    volume_client: Option<OhVolumeClient>,
    #[allow(dead_code)]
    radio_client: Option<OhRadioClient>,
}

impl OpenHomeRenderer {
    pub fn new(info: RendererInfo, registry: &DeviceRegistry) -> Self {
        let id = info.id.clone();
        Self {
            playlist: registry.oh_playlist_client_for_renderer(&id),
            info_client: registry.oh_info_client_for_renderer(&id),
            time_client: registry.oh_time_client_for_renderer(&id),
            volume_client: registry.oh_volume_client_for_renderer(&id),
            radio_client: registry.oh_radio_client_for_renderer(&id),
            info,
        }
    }

    pub fn id(&self) -> &RendererId {
        &self.info.id
    }

    pub fn friendly_name(&self) -> &str {
        &self.info.friendly_name
    }

    pub fn protocol(&self) -> &RendererProtocol {
        &self.info.protocol
    }

    pub fn has_playlist(&self) -> bool {
        self.playlist.is_some()
    }

    pub fn has_info(&self) -> bool {
        self.info_client.is_some()
    }

    pub fn has_time(&self) -> bool {
        self.time_client.is_some()
    }

    pub fn has_volume(&self) -> bool {
        self.volume_client.is_some()
    }

    pub fn has_any_openhome_service(&self) -> bool {
        self.has_playlist() || self.has_info() || self.has_time() || self.has_volume()
    }

    fn playlist_client_for(&self, op: &str) -> Result<&OhPlaylistClient> {
        self.playlist
            .as_ref()
            .ok_or_else(|| op_not_supported(op, "OpenHome Playlist"))
    }

    fn info_client_for(&self, op: &str) -> Result<&OhInfoClient> {
        self.info_client
            .as_ref()
            .ok_or_else(|| op_not_supported(op, "OpenHome Info"))
    }

    fn time_client_for(&self, op: &str) -> Result<&OhTimeClient> {
        self.time_client
            .as_ref()
            .ok_or_else(|| op_not_supported(op, "OpenHome Time"))
    }

    fn volume_client_for(&self, op: &str) -> Result<&OhVolumeClient> {
        self.volume_client
            .as_ref()
            .ok_or_else(|| op_not_supported(op, "OpenHome Volume"))
    }
}

impl TransportControl for OpenHomeRenderer {
    fn play_uri(&self, uri: &str, meta: &str) -> Result<()> {
        let playlist = self.playlist_client_for("play_uri")?;

        if let Err(err) = playlist.delete_all() {
            debug!(
                renderer = self.info.id.0.as_str(),
                error = %err,
                "Failed to clear OpenHome playlist before insert"
            );
        }

        let new_id = playlist.insert(0, uri, meta)?;
        playlist.play_id(new_id)
    }

    fn play(&self) -> Result<()> {
        let playlist = self.playlist_client_for("play")?;
        playlist.play()
    }

    fn pause(&self) -> Result<()> {
        let playlist = self.playlist_client_for("pause")?;
        playlist.pause()
    }

    fn stop(&self) -> Result<()> {
        let playlist = self.playlist_client_for("stop")?;
        playlist.stop()
    }

    fn seek_rel_time(&self, hhmmss: &str) -> Result<()> {
        let seconds = parse_hms(hhmmss).ok_or_else(|| {
            anyhow!(
                "Invalid HH:MM:SS format for OpenHome SeekSecondAbsolute: {}",
                hhmmss
            )
        })?;
        let playlist = self.playlist_client_for("seek_rel_time")?;
        playlist.seek_second_absolute(seconds)
    }
}

impl VolumeControl for OpenHomeRenderer {
    fn volume(&self) -> Result<u16> {
        let client = self.volume_client_for("volume")?;
        client.volume()
    }

    fn set_volume(&self, v: u16) -> Result<()> {
        let client = self.volume_client_for("set_volume")?;
        client.set_volume(v)
    }

    fn mute(&self) -> Result<bool> {
        let client = self.volume_client_for("mute")?;
        client.mute()
    }

    fn set_mute(&self, m: bool) -> Result<()> {
        let client = self.volume_client_for("set_mute")?;
        client.set_mute(m)
    }
}

impl PlaybackStatus for OpenHomeRenderer {
    fn playback_state(&self) -> Result<PlaybackState> {
        let client = self.info_client_for("playback_state")?;
        let state = client.transport_state()?;
        Ok(map_openhome_state(&state))
    }
}

impl PlaybackPosition for OpenHomeRenderer {
    fn playback_position(&self) -> Result<PlaybackPositionInfo> {
        let time_info = self.time_client_for("playback_position")?.position()?;

        let mut track_id = None;
        let mut track_uri = None;
        let mut track_metadata_xml = None;

        if let Some(info_client) = &self.info_client {
            match info_client.id() {
                Ok(id) => track_id = Some(id),
                Err(err) => debug!(
                    renderer = self.info.id.0.as_str(),
                    error = %err,
                    "Failed to read OpenHome track id"
                ),
            }

            match info_client.track() {
                Ok(track) => {
                    track_uri = Some(track.uri);
                    track_metadata_xml = track.metadata_xml;
                }
                Err(err) => debug!(
                    renderer = self.info.id.0.as_str(),
                    error = %err,
                    "Failed to read OpenHome track metadata"
                ),
            }
        }

        Ok(PlaybackPositionInfo {
            track: track_id,
            rel_time: Some(format_seconds(time_info.elapsed_secs)),
            abs_time: None,
            track_duration: Some(format_seconds(time_info.duration_secs)),
            track_metadata: track_metadata_xml,
            track_uri,
        })
    }
}

fn parse_hms(input: &str) -> Option<u32> {
    let parts: Vec<&str> = input.split(':').collect();
    if parts.is_empty() || parts.len() > 3 {
        return None;
    }

    let mut total = 0u32;
    for part in parts {
        let value = part.parse::<u32>().ok()?;
        total = total * 60 + value;
    }
    Some(total)
}

pub(crate) fn map_openhome_state(raw: &str) -> PlaybackState {
    match raw.trim().to_ascii_uppercase().as_str() {
        "PLAYING" => PlaybackState::Playing,
        "PAUSED" | "PAUSED_PLAYBACK" => PlaybackState::Paused,
        "STOPPED" => PlaybackState::Stopped,
        "BUFFERING" | "TRANSITIONING" => PlaybackState::Transitioning,
        other => PlaybackState::Unknown(other.to_string()),
    }
}

pub(crate) fn format_seconds(seconds: u32) -> String {
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    let secs = seconds % 60;
    format!("{hours:02}:{minutes:02}:{secs:02}")
}
