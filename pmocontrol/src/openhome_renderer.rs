use crate::capabilities::{
    PlaybackPosition, PlaybackPositionInfo, PlaybackState, PlaybackStatus, TransportControl,
    VolumeControl,
};
use crate::model::{RendererId, RendererInfo, RendererProtocol};
use crate::music_renderer::op_not_supported;
use crate::openhome::{
    build_info_client, build_playlist_client, build_product_client, build_radio_client,
    build_time_client, build_volume_client,
};
use crate::openhome_client::{
    parse_track_metadata_from_didl, OhInfoClient, OhPlaylistClient, OhProductClient, OhRadioClient,
    OhTimeClient, OhTrackEntry, OhVolumeClient, OPENHOME_PLAYLIST_HEAD_ID,
};
use crate::openhome_playlist::{OpenHomePlaylistSnapshot, OpenHomePlaylistTrack};
use anyhow::{anyhow, Result};
use tracing::debug;

#[derive(Clone, Debug)]
pub struct OpenHomeRenderer {
    pub info: RendererInfo,
    playlist: Option<OhPlaylistClient>,
    info_client: Option<OhInfoClient>,
    time_client: Option<OhTimeClient>,
    volume_client: Option<OhVolumeClient>,
    product_client: Option<OhProductClient>,
    #[allow(dead_code)]
    radio_client: Option<OhRadioClient>,
}

impl OpenHomeRenderer {
    pub fn new(info: RendererInfo) -> Self {
        Self {
            playlist: build_playlist_client(&info),
            info_client: build_info_client(&info),
            time_client: build_time_client(&info),
            volume_client: build_volume_client(&info),
            product_client: build_product_client(&info),
            radio_client: build_radio_client(&info),
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
        let playlist = self
            .playlist
            .as_ref()
            .ok_or_else(|| op_not_supported(op, "OpenHome Playlist"))?;
        self.ensure_playlist_source_selected()?;
        Ok(playlist)
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

    fn ensure_playlist_source_selected(&self) -> Result<()> {
        if let Some(product) = &self.product_client {
            product.ensure_playlist_source_selected().map_err(|err| {
                anyhow!(
                    "Failed to select OpenHome Playlist source for {}: {}",
                    self.info.id.0,
                    err
                )
            })
        } else {
            Ok(())
        }
    }

    pub(crate) fn snapshot_openhome_playlist(&self) -> Result<OpenHomePlaylistSnapshot> {
        let playlist = self.playlist_client_for("snapshot_openhome_playlist")?;
        let entries = playlist.read_all_tracks()?;

        // Get current track ID from the playlist service
        let current_id = playlist.id().ok();

        let current_index =
            current_id.and_then(|id| entries.iter().position(|entry| entry.id == id));

        debug!(
            renderer = self.info.id.0.as_str(),
            current_id = ?current_id,
            current_index = ?current_index,
            track_count = entries.len(),
            "snapshot_openhome_playlist completed"
        );

        let tracks = entries.iter().map(convert_oh_track_entry).collect();

        Ok(OpenHomePlaylistSnapshot {
            renderer_id: self.info.id.0.clone(),
            current_id,
            current_index,
            tracks,
        })
    }

    /// Retourne la longueur de la playlist OpenHome sans récupérer toutes les métadonnées.
    /// Plus rapide que snapshot_openhome_playlist() pour juste connaître le nombre de pistes.
    pub(crate) fn openhome_playlist_len(&self) -> Result<usize> {
        let playlist = self.playlist_client_for("openhome_playlist_len")?;
        let ids = playlist.id_array()?;
        Ok(ids.len())
    }

    /// Retourne les IDs des pistes de la playlist OpenHome.
    /// Plus rapide que snapshot_openhome_playlist() car ne récupère pas les métadonnées.
    pub(crate) fn openhome_playlist_ids(&self) -> Result<Vec<u32>> {
        let playlist = self.playlist_client_for("openhome_playlist_ids")?;
        playlist.id_array()
    }

    pub(crate) fn clear_openhome_playlist(&self) -> Result<()> {
        let playlist = self.playlist_client_for("clear_openhome_playlist")?;
        playlist.delete_all()
    }

    pub(crate) fn add_track_openhome(
        &self,
        uri: &str,
        metadata: &str,
        after_id: Option<u32>,
        play: bool,
    ) -> Result<u32> {
        let playlist = self.playlist_client_for("add_track_openhome")?;
        let insert_after = match after_id {
            Some(id) => id,
            None => playlist
                .id_array()?
                .last()
                .copied()
                .unwrap_or(OPENHOME_PLAYLIST_HEAD_ID),
        };

        let new_id = playlist.insert(insert_after, uri, metadata)?;
        if play {
            playlist.play_id(new_id)?;
        }
        Ok(new_id)
    }

    pub(crate) fn play_openhome_track_id(&self, id: u32) -> Result<()> {
        let playlist = self.playlist_client_for("play_openhome_track_id")?;
        playlist.play_id(id)
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

        // Reuse the same insertion logic as the queue path so that we honor
        // renderer expectations (IdArray sequencing, etc.).
        self.add_track_openhome(uri, meta, None, true)?;
        Ok(())
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
        let client = self.playlist_client_for("playback_state")?;
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

        if let Some(playlist_client) = &self.playlist {
            match playlist_client.id() {
                Ok(id) => track_id = Some(id),
                Err(err) => debug!(
                    renderer = self.info.id.0.as_str(),
                    error = %err,
                    "Failed to read OpenHome track id"
                ),
            }
        }

        if let Some(info_client) = &self.info_client {
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

fn convert_oh_track_entry(entry: &OhTrackEntry) -> OpenHomePlaylistTrack {
    let metadata = parse_track_metadata_from_didl(&entry.metadata_xml);
    OpenHomePlaylistTrack {
        id: entry.id,
        uri: entry.uri.clone(),
        title: metadata.as_ref().and_then(|m| m.title.clone()),
        artist: metadata.as_ref().and_then(|m| m.artist.clone()),
        album: metadata.as_ref().and_then(|m| m.album.clone()),
        album_art_uri: metadata.and_then(|m| m.album_art_uri),
    }
}
