use std::sync::{Arc, Mutex};

use crate::DeviceIdentity;
use crate::music_renderer::capabilities::{
    PlaybackPosition, PlaybackPositionInfo, PlaybackStatus, QueueTransportControl, RendererBackend,
    TransportControl, VolumeControl,
};
use crate::music_renderer::time_utils::{format_hhmmss_u32, parse_time_flexible};

use crate::errors::ControlPointError;
use crate::model::{PlaybackState, RendererInfo};
use crate::music_renderer::RendererFromMediaRendererInfo;
use crate::music_renderer::musicrenderer::MusicRendererBackend;
use crate::music_renderer::openhome::{
    build_info_client, build_playlist_client, build_product_client, build_radio_client,
    build_time_client, build_volume_client,
};
use crate::queue::{EnqueueMode, MusicQueue, PlaybackItem, QueueBackend, QueueSnapshot};
use crate::upnp_clients::{
    OPENHOME_PLAYLIST_HEAD_ID, OhInfoClient, OhPlaylistClient, OhProductClient, OhRadioClient,
    OhTimeClient, OhVolumeClient,
};
use tracing::debug;

#[derive(Clone, Debug)]
pub struct OpenHomeRenderer {
    playlist: Option<OhPlaylistClient>,
    info_client: Option<OhInfoClient>,
    time_client: Option<OhTimeClient>,
    volume_client: Option<OhVolumeClient>,
    product_client: Option<OhProductClient>,
    #[allow(dead_code)]
    radio_client: Option<OhRadioClient>,
    queue: Arc<Mutex<MusicQueue>>,
}

impl OpenHomeRenderer {
    pub fn new(
        playlist: Option<OhPlaylistClient>,
        info_client: Option<OhInfoClient>,
        time_client: Option<OhTimeClient>,
        volume_client: Option<OhVolumeClient>,
        product_client: Option<OhProductClient>,
        radio_client: Option<OhRadioClient>,
        queue: Arc<Mutex<MusicQueue>>,
    ) -> Self {
        Self {
            playlist,
            info_client,
            time_client,
            volume_client,
            product_client,
            radio_client,
            queue,
        }
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

    fn playlist_client_for(&self, op: &str) -> Result<&OhPlaylistClient, ControlPointError> {
        let playlist = self.playlist.as_ref().ok_or_else(|| {
            ControlPointError::upnp_operation_not_supported(op, "OpenHome Playlist")
        })?;
        self.ensure_playlist_source_selected()?;
        Ok(playlist)
    }

    fn info_client_for(&self, op: &str) -> Result<&OhInfoClient, ControlPointError> {
        self.info_client
            .as_ref()
            .ok_or_else(|| ControlPointError::upnp_operation_not_supported(op, "OpenHome Info"))
    }

    fn time_client_for(&self, op: &str) -> Result<&OhTimeClient, ControlPointError> {
        self.time_client
            .as_ref()
            .ok_or_else(|| ControlPointError::upnp_operation_not_supported(op, "OpenHome Time"))
    }

    fn volume_client_for(&self, op: &str) -> Result<&OhVolumeClient, ControlPointError> {
        self.volume_client
            .as_ref()
            .ok_or_else(|| ControlPointError::upnp_operation_not_supported(op, "OpenHome Volume"))
    }

    fn ensure_playlist_source_selected(&self) -> Result<(), ControlPointError> {
        if let Some(product) = &self.product_client {
            product.ensure_playlist_source_selected()
        } else {
            Ok(())
        }
    }

    // pub(crate) fn snapshot_openhome_playlist(&self) -> Result<OpenHomePlaylistSnapshot> {
    //     let playlist = self.playlist_client_for("snapshot_openhome_playlist")?;
    //     let entries = playlist.read_all_tracks()?;

    //     // Get current track ID from the playlist service
    //     let current_id = playlist.id().ok();

    //     let current_index =
    //         current_id.and_then(|id| entries.iter().position(|entry| entry.id == id));

    //     debug!(
    //         current_id = ?current_id,
    //         current_index = ?current_index,
    //         track_count = entries.len(),
    //         "snapshot_openhome_playlist completed"
    //     );

    //     let tracks = entries.iter().map(convert_oh_track_entry).collect();

    //     Ok(OpenHomePlaylistSnapshot {
    //         current_id,
    //         current_index,
    //         tracks,
    //     })
    // }

    /// Retourne la longueur de la playlist OpenHome sans récupérer toutes les métadonnées.
    /// Plus rapide que snapshot_openhome_playlist() pour juste connaître le nombre de pistes.
    pub(crate) fn openhome_playlist_len(&self) -> Result<usize, ControlPointError> {
        let playlist = self.playlist_client_for("openhome_playlist_len")?;
        let ids = playlist.id_array()?;
        Ok(ids.len())
    }

    /// Retourne les IDs des pistes de la playlist OpenHome.
    /// Plus rapide que snapshot_openhome_playlist() car ne récupère pas les métadonnées.
    pub(crate) fn openhome_playlist_ids(&self) -> Result<Vec<u32>, ControlPointError> {
        let playlist = self.playlist_client_for("openhome_playlist_ids")?;
        playlist.id_array()
    }

    pub(crate) fn clear_openhome_playlist(&self) -> Result<(), ControlPointError> {
        let playlist = self.playlist_client_for("clear_openhome_playlist")?;
        playlist.delete_all()
    }

    pub(crate) fn add_track_openhome(
        &self,
        uri: &str,
        metadata: &str,
        after_id: Option<u32>,
        play: bool,
    ) -> Result<u32, ControlPointError> {
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
            playlist.seek_id(new_id)?;
        }
        Ok(new_id)
    }

    pub(crate) fn play_openhome_track_id(&self, id: u32) -> Result<(), ControlPointError> {
        let playlist = self.playlist_client_for("play_openhome_track_id")?;
        playlist.seek_id(id)
    }
}

impl RendererFromMediaRendererInfo for OpenHomeRenderer {
    fn from_renderer_info(info: &RendererInfo) -> Result<Self, ControlPointError> {
        // Create the OpenHome queue
        let queue = Arc::new(Mutex::new(MusicQueue::from_renderer_info(info)?));

        let renderer = OpenHomeRenderer::new(
            build_playlist_client(&info),
            build_info_client(&info),
            build_time_client(&info),
            build_volume_client(&info),
            build_product_client(&info),
            build_radio_client(&info),
            queue,
        );

        if renderer.has_any_openhome_service() {
            Ok(renderer)
        } else {
            Err(ControlPointError::OpenHomeNotAValidDevice(format!(
                "{:?}",
                info.id()
            )))
        }
    }

    fn to_backend(self) -> MusicRendererBackend {
        MusicRendererBackend::OpenHome(self)
    }
}

impl RendererBackend for OpenHomeRenderer {
    fn queue(&self) -> &Arc<Mutex<MusicQueue>> {
        &self.queue
    }
}

impl TransportControl for OpenHomeRenderer {
    fn play_uri(&self, uri: &str, meta: &str) -> Result<(), ControlPointError> {
        let playlist = self.playlist_client_for("play_uri")?;

        if let Err(err) = playlist.delete_all() {
            debug!(
               // renderer = self.info.id.0.as_str(),
                error = %err,
                "Failed to clear OpenHome playlist before insert"
            );
        }

        // Reuse the same insertion logic as the queue path so that we honor
        // renderer expectations (IdArray sequencing, etc.).
        self.add_track_openhome(uri, meta, None, true)?;

        // Start playback (like UPnP renderer does with avt.play())
        playlist.play()?;
        Ok(())
    }

    fn play(&self) -> Result<(), ControlPointError> {
        let playlist = self.playlist_client_for("play")?;
        playlist.play()
    }

    fn pause(&self) -> Result<(), ControlPointError> {
        let playlist = self.playlist_client_for("pause")?;
        playlist.pause()
    }

    fn stop(&self) -> Result<(), ControlPointError> {
        let playlist = self.playlist_client_for("stop")?;
        playlist.stop()
    }

    fn seek_rel_time(&self, hhmmss: &str) -> Result<(), ControlPointError> {
        let seconds = parse_time_flexible(hhmmss)?;
        let playlist = self.playlist_client_for("seek_rel_time")?;
        playlist.seek_second_absolute(seconds)
    }
}

impl VolumeControl for OpenHomeRenderer {
    fn volume(&self) -> Result<u16, ControlPointError> {
        let client = self.volume_client_for("volume")?;
        client.volume()
    }

    fn set_volume(&self, v: u16) -> Result<(), ControlPointError> {
        let client = self.volume_client_for("set_volume")?;
        client.set_volume(v)
    }

    fn mute(&self) -> Result<bool, ControlPointError> {
        let client = self.volume_client_for("mute")?;
        client.mute()
    }

    fn set_mute(&self, m: bool) -> Result<(), ControlPointError> {
        let client = self.volume_client_for("set_mute")?;
        client.set_mute(m)
    }
}

impl PlaybackStatus for OpenHomeRenderer {
    fn playback_state(&self) -> Result<PlaybackState, ControlPointError> {
        let client = self.playlist_client_for("playback_state")?;
        let state = client.transport_state()?;
        Ok(map_openhome_state(&state))
    }
}

impl PlaybackPosition for OpenHomeRenderer {
    fn playback_position(&self) -> Result<PlaybackPositionInfo, ControlPointError> {
        let time_info = self.time_client_for("playback_position")?.position()?;

        let mut track_id = None;
        let mut track_uri = None;
        let mut track_metadata_xml = None;

        if let Some(playlist_client) = &self.playlist {
            match playlist_client.id() {
                Ok(id) => track_id = Some(id),
                Err(err) => debug!(
                //   renderer = self.info.id.0.as_str(),
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
                //    renderer = self.info.id.0.as_str(),
                    error = %err,
                    "Failed to read OpenHome track metadata"
                ),
            }
        }

        Ok(PlaybackPositionInfo {
            track: track_id,
            rel_time: Some(format_hhmmss_u32(time_info.elapsed_secs)),
            abs_time: None,
            track_duration: Some(format_hhmmss_u32(time_info.duration_secs)),
            track_metadata: track_metadata_xml,
            track_uri,
        })
    }
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

impl QueueTransportControl for OpenHomeRenderer {
    fn play_from_queue(&self) -> Result<(), ControlPointError> {
        {
            let queue = self.queue.lock().unwrap();

            if queue.current_index()?.is_none() {
                if queue.len()? > 0 {
                    drop(queue);
                    let mut queue = self.queue.lock().unwrap();
                    queue.set_index(Some(0))?;
                } else {
                    return Err(ControlPointError::QueueError("Queue is empty".into()));
                }
            }
        }

        let playlist = self.playlist_client_for("play_from_queue")?;
        playlist.play()
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
        // For OpenHome, we need to convert index to track_id
        let track_id = {
            let queue = self.queue.lock().unwrap();
            queue.position_to_id(index)?
        };

        // Seek to the track by ID
        let playlist = self.playlist_client_for("play_from_index")?;
        playlist.seek_id(track_id)?;

        // Update local queue index
        {
            let mut queue = self.queue.lock().unwrap();
            queue.set_index(Some(index))?;
        }

        // Start playback
        playlist.play()?;
        Ok(())
    }
}

impl QueueBackend for OpenHomeRenderer {
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
