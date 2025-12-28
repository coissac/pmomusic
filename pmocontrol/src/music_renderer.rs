//! Backend-agnostic music renderer façade for PMOMusic.
//!
//! `MusicRenderer` wraps every supported backend (UPnP AV/DLNA, OpenHome,
//! LinkPlay HTTP, Arylic TCP, Chromecast, and the hybrid UPnP + Arylic pairing) behind a
//! single control surface. Higher layers in PMOMusic must only interact with
//! renderers through this type so that transport, volume, and state queries
//! stay backend-neutral.

use std::sync::{Arc, Mutex, RwLock};
use std::time::SystemTime;

use crate::capabilities::{PlaybackPositionInfo, PlaybackStatus};
// use crate::control_point::RendererRuntimeStateMut;
use crate::control_point::music_queue::MusicQueue;
use crate::control_point::openhome_queue::didl_id_from_metadata;
use crate::errors::ControlPointError;
use crate::media_server::ServerId;
use crate::model::{
    RendererConnectionState, ServiceId, RendererInfo, RendererProtocol, TrackMetadata,
};
use crate::openhome_client::parse_track_metadata_from_didl;
use crate::openhome_playlist::{OpenHomePlaylistSnapshot, OpenHomePlaylistTrack};
use crate::queue_backend::{PlaybackItem, QueueSnapshot};
use crate::{
    ArylicTcpRenderer, ChromecastRenderer, DeviceIdentity, DeviceOnline, DeviceRegistry, LinkPlayRenderer, OpenHomeRenderer, PlaybackPosition, PlaybackState, TransportControl, UpnpRenderer, VolumeControl
};
use anyhow::{Result, anyhow};
use tracing::{debug, info, warn};

/// Backend-agnostic façade exposing transport, volume, and status contracts.
#[derive(Clone, Debug)]
pub enum MusicRendererBackend {
    /// Classic UPnP AV / DLNA renderer (AVTransport + RenderingControl).
    Upnp(UpnpRenderer),
    /// Renderer powered by OpenHome services.
    OpenHome(OpenHomeRenderer),
    /// Renderer controlled via the LinkPlay HTTP API.
    LinkPlay(LinkPlayRenderer),
    /// Renderer reachable through the Arylic TCP control protocol (port 8899).
    ArylicTcp(ArylicTcpRenderer),
    /// Renderer controlled via the Google Cast protocol (Chromecast).
    Chromecast(ChromecastRenderer),
    /// Combined backend using UPnP for transport + volume writes and Arylic TCP
    /// to read detailed playback information as well as live volume/mute state.
    HybridUpnpArylic {
        upnp: UpnpRenderer,
        arylic: ArylicTcpRenderer,
    },
}

#[derive(Debug, Clone)]
pub struct MusicRenderer {
    info: RendererInfo,
    connection: Arc<Mutex<RendererConnectionState>>,
    backend: Arc<Mutex<MusicRendererBackend>>,
    queue: Arc<Mutex<MusicQueue>>,
}

impl MusicRenderer {
    pub fn new(
        info: RendererInfo,
        backend: Arc<Mutex<MusicRendererBackend>>,
        queue: Arc<Mutex<MusicQueue>>,
    ) -> Arc<MusicRenderer> {
        let connection = RendererConnectionState::new();

        let renderer = MusicRenderer {
            info,
            connection,
            backend,
            queue,
        };

        Arc::new(renderer)
    }

    pub fn from_renderer_info(info: RendererInfo) -> Result<Arc<MusicRenderer>,ControlPointError>  {
        let connection = RendererConnectionState::new();
        let backend = MusicRendererBackend::from_renderer_info(info)?;
        let queue = Mutex::new(MusicQueue::new());

        let renderer = Arc::new(MusicRenderer { info, connection, backend, queue });
        Ok(renderer)
    }

    pub fn info(&self) -> &RendererInfo {
        &self.info
    }

    /// Returns the protocol.
    fn protocol(&self) -> RendererProtocol {
        self.info.protocol()
    }

    fn is_upnp(&self) -> bool {
        match &*self.backend.lock().expect("Backend mutex poisoned") {
            MusicRendererBackend::Upnp(_) => true,
            _ => false,
        }
    }

    fn is_openhome(&self) -> bool {
        match &*self.backend.lock().expect("Backend mutex poisoned") {
            MusicRendererBackend::OpenHome(_) => true,
            _ => false,
        }
    }

    fn is_linkplay(&self) -> bool {
        match &*self.backend.lock().expect("Backend mutex poisoned") {
            MusicRendererBackend::LinkPlay(_) => true,
            _ => false,
        }
    }

    fn is_arylictcp(&self) -> bool {
        match &*self.backend.lock().expect("Backend mutex poisoned") {
            MusicRendererBackend::ArylicTcp(_) => true,
            _ => false,
        }
    }

    fn is_chromecast(&self) -> bool {
        match &*self.backend.lock().expect("Backend mutex poisoned") {
            MusicRendererBackend::Chromecast(_) => true,
            _ => false,
        }
    }

    fn is_hybridupnparylic(&self) -> bool {
        match &*self.backend.lock().expect("Backend mutex poisoned") {
            MusicRendererBackend::HybridUpnpArylic { .. } => true,
            _ => false,
        }
    }

    /// Returns true if this renderer is known to support SetNextAVTransportURI.
    pub fn supports_set_next(&self) -> bool {
        self.info.capabilities().supports_set_next()
    }


}

impl DeviceIdentity for MusicRenderer  {
    fn id(&self) -> ServiceId {
        self.info.id()
    }

    fn udn(&self)   -> &str {
         &*self.info.udn()
      }

    fn friendly_name(&self) -> &str {
        &self.info.friendly_name()
    }

    fn model_name(&self)  -> &str {
         &self.info.model_name()
     }
    fn manufacturer(&self)   -> &str {
         &self.info.manufacturer()
      }
    fn location(&self)   ->  &str {
          &self.info.location()
       }
    fn server_header(&self)   -> &str {
          &self.info.server_header()
       }

}

impl DeviceOnline for MusicRenderer {
     fn is_online(&self) -> bool {
        self.connection
            .lock()
            .expect("Connection mutex poisoned")
            .is_online()
    }

     fn last_seen(&self) -> SystemTime {
        self.connection
            .lock()
            .expect("Connection mutex poisoned")
            .last_seen()
    }

     fn has_been_seen_now(&self, max_age: u32) {
        self.connection
            .lock()
            .expect("Connection mutex poisoned")
            .has_been_seen_now(max_age)
    }

     fn mark_as_offline(&self) {
        self.connection
             .lock()
              .expect("Connection mutex poisoned")
               .mark_as_offline()
    }

     fn max_age(&self) -> u32 {
        self.connection
            .lock()
            .expect("Connection mutex poisoned")
            .max_age()
    }}
// #[derive(Clone, Debug)]
// pub struct RendererRuntimeState {
//     pub queue: MusicQueue,
// }

// pub trait OpenHomeQueueProvider: Send + Sync + 'static {
//     fn renderer_state(&self, renderer_id: &RendererId) -> Result<RendererRuntimeState>;
//     fn renderer_state_mut<'a>(
//         &'a self,
//         renderer_id: &RendererId,
//     ) -> Result<RendererRuntimeStateMut<'a>>;
//     fn invalidate_openhome_cache(&self, renderer_id: &RendererId) -> Result<()>;
// }

// static OPENHOME_QUEUE_PROVIDER: OnceLock<Arc<dyn OpenHomeQueueProvider>> = OnceLock::new();

// pub fn set_openhome_queue_provider(provider: Arc<dyn OpenHomeQueueProvider>) {
//     let _ = OPENHOME_QUEUE_PROVIDER.set(provider);
// }

impl MusicRendererBackend {
    // fn as_backend(&self) -> &dyn MusicRendererBackend {
    //     match self {
    //         MusicRendererBackend::OpenHome(r) => r,
    //         MusicRendererBackend::HybridUpnpArylic { arylic, .. } => arylic,
    //         MusicRendererBackend::Upnp(r) => r,
    //         MusicRendererBackend::LinkPlay(r) => r,
    //         MusicRendererBackend::ArylicTcp(r) => r,
    //         MusicRendererBackend::Chromecast(r) => r,
    //     }
    // }

    /// Construct a music renderer from a [`RendererInfo`] and the registry.
    ///
    /// Returns `None` when no supported backend can be built for this renderer.
    /// UPnP AV / hybrid renderers map either to [`MusicRenderer::LinkPlay`] (when supported)
    /// or [`MusicRenderer::Upnp`].
    pub fn from_renderer_info(
        info: RendererInfo,
    ) -> Result<Arc<Mutex<Self>>, ControlPointError>  {

        match info.protocol() {
            RendererProtocol::OpenHomeOnly | RendererProtocol::OpenHomeHybrid => {
                OpenHomeRenderer::from_renderer_info(info.clone())
            }

            _ =>   Err(ControlPointError::MusicRendererBackendBuild(format!("{:#?}", info.id()))),   
        }    
        

        // Le backend est de type openhome

        if matches!(
            info.protocol(),
            RendererProtocol::OpenHomeOnly | RendererProtocol::OpenHomeHybrid
        ) {
            if let Some(renderer) = {
                let renderer = OpenHomeRenderer::new(info.clone());
                renderer.has_any_openhome_service().then_some(renderer)
            } {
                return Some(MusicRendererBackend::OpenHome(renderer));
            }

            if matches!(info.protocol(), RendererProtocol::OpenHomeOnly) {
                warn!(
                    renderer = info.friendly_name(),
                    "Renderer advertises OpenHome only but exposes no usable services"
                );
                return None;
            }
        }

        if matches!(info.protocol(), RendererProtocol::ChromecastOnly) {
            if let Ok(renderer) = ChromecastRenderer::from_renderer_info(info.clone()) {
                return Some(MusicRendererBackend::Chromecast(renderer));
            }
            warn!(
                renderer = info.friendly_name(),
                "Failed to build Chromecast renderer"
            );
            return None;
        }

        match info.protocol() {
            RendererProtocol::UpnpAvOnly | RendererProtocol::OpenHomeHybrid => {
                let has_arylic = info.capabilities().has_arylic_tcp;
                let has_avtransport = info.capabilities().has_avtransport;

                if has_arylic && has_avtransport {
                    // Construire UpnpRenderer
                    let upnp = UpnpRenderer::from_info(&info);

                    // Construire ArylicTcpRenderer
                    match ArylicTcpRenderer::from_renderer_info(info.clone()) {
                        Ok(arylic) => {
                            return Some(MusicRendererBackend::HybridUpnpArylic { upnp, arylic });
                        }
                        Err(err) => {
                            warn!(
                                "Failed to build Arylic TCP backend for {}: {}. Falling back to UPnP only.",
                                info.friendly_name(),
                                err
                            );
                            return Some(MusicRendererBackend::Upnp(upnp));
                        }
                    }
                }

                // Pas d’Arylic : logique existante
                if info.capabilities().has_linkplay_http() {
                    if let Ok(lp) = LinkPlayRenderer::from_renderer_info(info.clone()) {
                        return Some(MusicRendererBackend::LinkPlay(lp));
                    }
                }

                Some(MusicRendererBackend::Upnp(UpnpRenderer::from_info(
                    &info,
                )))
            }
            RendererProtocol::OpenHomeOnly => None,
            RendererProtocol::ChromecastOnly => None,
        }
    }

    pub fn openhome_playlist_snapshot(&self) -> Result<OpenHomePlaylistSnapshot> {
        match self {
            MusicRendererBackend::OpenHome(renderer) => renderer.snapshot_openhome_playlist(),
            _ => Err(op_not_supported(
                "openhome_playlist_snapshot",
                self.unsupported_backend_name(),
            )),
        }
    }

    pub fn openhome_playlist_len(&self) -> Result<usize> {
        match self {
            MusicRendererBackend::OpenHome(renderer) => renderer.openhome_playlist_len(),
            _ => Err(op_not_supported(
                "openhome_playlist_len",
                self.unsupported_backend_name(),
            )),
        }
    }

    pub fn openhome_playlist_ids(&self) -> Result<Vec<u32>> {
        match self {
            MusicRendererBackend::OpenHome(renderer) => renderer.openhome_playlist_ids(),
            _ => Err(op_not_supported(
                "openhome_playlist_ids",
                self.unsupported_backend_name(),
            )),
        }
    }

    pub fn openhome_playlist_clear(&self) -> Result<()> {
        if let Some(provider) = OPENHOME_QUEUE_PROVIDER.get() {
            let result = {
                let mut state = provider.renderer_state_mut(self.id())?;
                match &mut *state.queue {
                    MusicQueue::OpenHome(queue) => queue.clear(),
                    _ => Err(op_not_supported(
                        "openhome_playlist_clear",
                        self.unsupported_backend_name(),
                    )),
                }
            };
            if result.is_ok() {
                provider.invalidate_openhome_cache(self.id())?;
            }
            result
        } else {
            self.fetch_openhome_playlist_clear()
        }
    }

    pub(crate) fn fetch_openhome_playlist_clear(&self) -> Result<()> {
        match self {
            MusicRendererBackend::OpenHome(renderer) => renderer.clear_openhome_playlist(),
            _ => Err(op_not_supported(
                "openhome_playlist_clear",
                self.unsupported_backend_name(),
            )),
        }
    }

    /// High-level method to prepare the renderer for attaching a new playlist.
    ///
    /// This method handles backend-specific clearing logic:
    /// - For OpenHome: clears the OpenHome playlist
    /// - For AVTransport/Chromecast/etc.: stops the renderer (since they don't have a persistent queue)
    ///
    /// This should be called by ControlPoint when attaching a new playlist, ensuring that:
    /// - Any currently playing content is stopped
    /// - The renderer is in a clean state ready to receive new content
    pub fn clear_for_playlist_attach(&self) -> Result<()> {
        match self {
            MusicRendererBackend::OpenHome(_) => {
                // For OpenHome: clear the playlist (DeleteAll also stops playback automatically)
                // then explicitly stop to ensure clean state
                self.openhome_playlist_clear()?;
                self.stop().or_else(|err| -> Result<()> {
                    // If stop fails (e.g., already stopped), that's fine
                    warn!(
                        renderer = self.id().0.as_str(),
                        error = %err,
                        "Stop failed after clearing OpenHome playlist (continuing anyway)"
                    );
                    Ok(())
                })
            }
            MusicRendererBackend::Upnp(_)
            | MusicRendererBackend::Chromecast(_)
            | MusicRendererBackend::LinkPlay(_)
            | MusicRendererBackend::ArylicTcp(_)
            | MusicRendererBackend::HybridUpnpArylic { .. } => {
                // For AVTransport and other single-track renderers: stop playback
                // This ensures we're not in the middle of playing when we start the new playlist
                self.stop().or_else(|err| {
                    // If stop fails (e.g., already stopped), that's fine - we just want to ensure it's not playing
                    warn!(
                        renderer = self.id().0.as_str(),
                        error = %err,
                        "Stop failed when preparing for playlist attach (continuing anyway)"
                    );
                    Ok(())
                })
            }
        }
    }

    /// Synchronize the local queue state with the backend's actual state.
    ///
    /// - For OpenHome: fetches the playlist from the renderer and updates local cache
    /// - For Internal queue/AVTransport: no-op (queue is already local)
    pub fn sync_queue_state(&self) -> Result<()> {
        match self {
            MusicRendererBackend::OpenHome(_) => {
                if let Some(provider) = OPENHOME_QUEUE_PROVIDER.get() {
                    // Fetch fresh playlist snapshot and update cache
                    provider.invalidate_openhome_cache(self.id())?;
                }
                Ok(())
            }
            MusicRendererBackend::Upnp(_)
            | MusicRendererBackend::Chromecast(_)
            | MusicRendererBackend::LinkPlay(_)
            | MusicRendererBackend::ArylicTcp(_)
            | MusicRendererBackend::HybridUpnpArylic { .. } => {
                // No sync needed - queue is local only
                Ok(())
            }
        }
    }

    /// Clear the queue on both the backend and in local state.
    ///
    /// This ensures the backend renderer and local cache are consistent.
    /// Should be called before queue mutations to ensure clean state.
    pub fn clear_queue(&self) -> Result<()> {
        match self {
            MusicRendererBackend::OpenHome(_) => {
                // For OpenHome: clear the playlist on the renderer itself
                self.openhome_playlist_clear()
            }
            MusicRendererBackend::Upnp(_)
            | MusicRendererBackend::Chromecast(_)
            | MusicRendererBackend::LinkPlay(_)
            | MusicRendererBackend::ArylicTcp(_)
            | MusicRendererBackend::HybridUpnpArylic { .. } => {
                // For other renderers: no persistent queue to clear
                Ok(())
            }
        }
    }

    /// Add a track to the backend's queue, returning backend-specific track ID if applicable.
    ///
    /// - For OpenHome: adds to OpenHome playlist and returns track ID
    /// - For others: returns error (not supported for single-track renderers)
    pub fn add_track_to_queue(
        &self,
        uri: &str,
        metadata: &str,
        after_id: Option<u32>,
        play: bool,
    ) -> Result<Option<u32>> {
        match self {
            MusicRendererBackend::OpenHome(_) => {
                let track_id = self.openhome_playlist_add_track(uri, metadata, after_id, play)?;
                Ok(Some(track_id))
            }
            MusicRendererBackend::Upnp(_)
            | MusicRendererBackend::Chromecast(_)
            | MusicRendererBackend::LinkPlay(_)
            | MusicRendererBackend::ArylicTcp(_)
            | MusicRendererBackend::HybridUpnpArylic { .. } => Err(anyhow!(
                "add_track_to_queue is not supported for {} backend",
                self.unsupported_backend_name()
            )),
        }
    }

    /// Select and play a specific track from the backend's queue.
    ///
    /// - For OpenHome: uses track ID to select from OpenHome playlist
    /// - For others: returns error (use play_uri instead)
    pub fn select_queue_track(&self, track_id: u32) -> Result<()> {
        match self {
            MusicRendererBackend::OpenHome(_) => self.openhome_playlist_play_id(track_id),
            MusicRendererBackend::Upnp(_)
            | MusicRendererBackend::Chromecast(_)
            | MusicRendererBackend::LinkPlay(_)
            | MusicRendererBackend::ArylicTcp(_)
            | MusicRendererBackend::HybridUpnpArylic { .. } => Err(anyhow!(
                "select_queue_track is not supported for {} backend",
                self.unsupported_backend_name()
            )),
        }
    }

    /// Get the current queue state from the backend.
    ///
    /// - For OpenHome: fetches current playlist snapshot
    /// - For others: returns None (no persistent queue on backend)
    pub fn queue_snapshot(&self) -> Result<Option<QueueSnapshot>> {
        match self {
            MusicRendererBackend::OpenHome(_) => {
                let oh_snapshot = self.openhome_playlist_snapshot()?;

                // Convert OpenHome tracks to PlaybackItems
                let items: Vec<PlaybackItem> = oh_snapshot
                    .tracks
                    .iter()
                    .map(|track| Self::playback_item_from_openhome_track(self.id(), track))
                    .collect();

                let snapshot = QueueSnapshot {
                    items,
                    current_index: oh_snapshot.current_index,
                };

                Ok(Some(snapshot))
            }
            MusicRendererBackend::Upnp(_)
            | MusicRendererBackend::Chromecast(_)
            | MusicRendererBackend::LinkPlay(_)
            | MusicRendererBackend::ArylicTcp(_)
            | MusicRendererBackend::HybridUpnpArylic { .. } => {
                // No backend queue for these renderers
                Ok(None)
            }
        }
    }

    /// Play the current item from the backend queue.
    ///
    /// - For OpenHome: Uses the native playlist to play the current track
    /// - For others: Returns error (no backend queue)
    pub fn play_current_from_backend_queue(&self) -> Result<()> {
        match self {
            MusicRendererBackend::OpenHome(_) => {
                // Get the current OpenHome playlist snapshot
                let snapshot = self.openhome_playlist_snapshot()?;

                debug!(
                    renderer = self.id().0.as_str(),
                    tracks_count = snapshot.tracks.len(),
                    current_id = ?snapshot.current_id,
                    current_index = ?snapshot.current_index,
                    "play_current_from_backend_queue: OpenHome snapshot fetched"
                );

                if snapshot.tracks.is_empty() {
                    return Err(anyhow!("OpenHome playlist is empty"));
                }

                // Find the track_id to play (prefer current_id, then current_index, then first)
                let target_track_id = if let Some(current_id) = snapshot.current_id {
                    debug!(
                        renderer = self.id().0.as_str(),
                        track_id = current_id,
                        "Using current_id for playback"
                    );
                    Some(current_id)
                } else if let Some(current_idx) = snapshot.current_index {
                    let track_id = snapshot.tracks.get(current_idx).map(|track| track.id);
                    debug!(
                        renderer = self.id().0.as_str(),
                        current_idx,
                        track_id = ?track_id,
                        "Using current_index for playback"
                    );
                    track_id
                } else {
                    let track_id = snapshot.tracks.first().map(|track| track.id);
                    debug!(
                        renderer = self.id().0.as_str(),
                        track_id = ?track_id,
                        "Using first track for playback"
                    );
                    track_id
                };

                if let Some(track_id) = target_track_id {
                    info!(
                        renderer = self.id().0.as_str(),
                        track_id, "Calling openhome_playlist_play_id to start playback"
                    );
                    self.openhome_playlist_play_id(track_id)?;
                    info!(
                        renderer = self.id().0.as_str(),
                        track_id, "Successfully called openhome_playlist_play_id"
                    );
                    Ok(())
                } else {
                    Err(anyhow!("No track to play in OpenHome playlist"))
                }
            }
            MusicRendererBackend::Upnp(_)
            | MusicRendererBackend::Chromecast(_)
            | MusicRendererBackend::LinkPlay(_)
            | MusicRendererBackend::ArylicTcp(_)
            | MusicRendererBackend::HybridUpnpArylic { .. } => Err(anyhow!(
                "play_current_from_backend_queue is not supported for {} backend (no persistent queue)",
                self.unsupported_backend_name()
            )),
        }
    }

    /// Play the next item from the backend queue.
    ///
    /// - For OpenHome: Advances to the next track in the playlist
    /// - For others: Returns error (no backend queue)
    pub fn play_next_from_backend_queue(&self) -> Result<()> {
        match self {
            MusicRendererBackend::OpenHome(_) => {
                // Get the current OpenHome playlist snapshot
                let snapshot = self.openhome_playlist_snapshot()?;

                if snapshot.tracks.is_empty() {
                    return Err(anyhow!("OpenHome playlist is empty, cannot play next"));
                }

                // Determine the next track_id
                let next_track_id = match snapshot.current_index {
                    Some(idx) => {
                        // Take the next track if it exists, otherwise loop to first
                        snapshot
                            .tracks
                            .get(idx + 1)
                            .map(|track| track.id)
                            .or_else(|| snapshot.tracks.first().map(|track| track.id))
                    }
                    None => snapshot.tracks.first().map(|track| track.id),
                };

                if let Some(track_id) = next_track_id {
                    self.openhome_playlist_play_id(track_id)?;
                    Ok(())
                } else {
                    Err(anyhow!(
                        "No track available to advance to in OpenHome playlist"
                    ))
                }
            }
            MusicRendererBackend::Upnp(_)
            | MusicRendererBackend::Chromecast(_)
            | MusicRendererBackend::LinkPlay(_)
            | MusicRendererBackend::ArylicTcp(_)
            | MusicRendererBackend::HybridUpnpArylic { .. } => Err(anyhow!(
                "play_next_from_backend_queue is not supported for {} backend (no persistent queue)",
                self.unsupported_backend_name()
            )),
        }
    }

    /// Convert an OpenHome playlist track to a PlaybackItem.
    fn playback_item_from_openhome_track(
        renderer_id: &ServiceId,
        track: &OpenHomePlaylistTrack,
    ) -> PlaybackItem {
        let metadata = TrackMetadata {
            title: track.title.clone(),
            artist: track.artist.clone(),
            album: track.album.clone(),
            genre: None,
            album_art_uri: track.album_art_uri.clone(),
            date: None,
            track_number: None,
            creator: None,
        };

        PlaybackItem {
            media_server_id: ServerId(format!("openhome:{}", renderer_id.0)),
            didl_id: format!("openhome:{}", track.id),
            uri: track.uri.clone(),
            // OpenHome tracks don't provide protocolInfo, use generic default
            protocol_info: "http-get:*:audio/*:*".to_string(),
            metadata: Some(metadata),
        }
    }

    pub fn openhome_playlist_add_track(
        &self,
        uri: &str,
        metadata: &str,
        after_id: Option<u32>,
        play: bool,
    ) -> Result<u32> {
        if let Some(provider) = OPENHOME_QUEUE_PROVIDER.get() {
            let result = {
                let mut state = provider.renderer_state_mut(self.id())?;
                match &mut *state.queue {
                    MusicQueue::OpenHome(queue) => {
                        let playback_item =
                            Self::playback_item_from_params(self.id(), uri, metadata)?;
                        queue.add_playback_item(playback_item, after_id, play)
                    }
                    _ => Err(op_not_supported(
                        "openhome_playlist_add_track",
                        self.unsupported_backend_name(),
                    )),
                }
            };
            if result.is_ok() {
                provider.invalidate_openhome_cache(self.id())?;
            }
            result
        } else {
            self.fetch_openhome_playlist_add_track(uri, metadata, after_id, play)
        }
    }

    pub fn openhome_playlist_play_id(&self, id: u32) -> Result<()> {
        if let Some(provider) = OPENHOME_QUEUE_PROVIDER.get() {
            let result = {
                let mut state = provider.renderer_state_mut(self.id())?;
                match &mut *state.queue {
                    MusicQueue::OpenHome(queue) => queue.select_track_id(id),
                    _ => Err(op_not_supported(
                        "openhome_playlist_play_id",
                        self.unsupported_backend_name(),
                    )),
                }
            };
            if result.is_ok() {
                provider.invalidate_openhome_cache(self.id())?;
            }
            result
        } else {
            self.fetch_openhome_playlist_play_id(id)
        }
    }

    fn unsupported_backend_name(&self) -> &'static str {
        match self {
            MusicRendererBackend::Upnp(_) => "UPnP",
            MusicRendererBackend::OpenHome(_) => "OpenHome",
            MusicRendererBackend::LinkPlay(_) => "LinkPlay",
            MusicRendererBackend::ArylicTcp(_) => "ArylicTcp",
            MusicRendererBackend::Chromecast(_) => "Chromecast",
            MusicRendererBackend::HybridUpnpArylic { .. } => "HybridUpnpArylic",
        }
    }

    fn fetch_openhome_playlist_add_track(
        &self,
        uri: &str,
        metadata: &str,
        after_id: Option<u32>,
        play: bool,
    ) -> Result<u32> {
        match self {
            MusicRendererBackend::OpenHome(renderer) => {
                renderer.add_track_openhome(uri, metadata, after_id, play)
            }
            _ => Err(op_not_supported(
                "openhome_playlist_add_track",
                self.unsupported_backend_name(),
            )),
        }
    }

    fn fetch_openhome_playlist_play_id(&self, id: u32) -> Result<()> {
        match self {
            MusicRendererBackend::OpenHome(renderer) => renderer.play_openhome_track_id(id),
            _ => Err(op_not_supported(
                "openhome_playlist_play_id",
                self.unsupported_backend_name(),
            )),
        }
    }

    fn playback_item_from_params(
        renderer_id: &ServiceId,
        uri: &str,
        metadata_xml: &str,
    ) -> Result<PlaybackItem> {
        let metadata = parse_track_metadata_from_didl(metadata_xml);
        let didl_id = didl_id_from_metadata(metadata_xml)
            .unwrap_or_else(|| format!("openhome:{}", renderer_id.0));
        Ok(PlaybackItem {
            media_server_id: ServerId(format!("openhome:{}", renderer_id.0)),
            didl_id,
            uri: uri.to_string(),
            // OpenHome tracks don't provide protocolInfo, use generic default
            protocol_info: "http-get:*:audio/*:*".to_string(),
            metadata,
        })
    }
}

/// Transport control façade that dispatches to whichever backend can fulfill
/// the request, returning a standardized error if the backend lacks support.
impl TransportControl for MusicRendererBackend {
    fn play_uri(&self, uri: &str, meta: &str) -> Result<()> {
        match self {
            MusicRendererBackend::Upnp(upnp) => upnp.play_uri(uri, meta),
            MusicRendererBackend::OpenHome(oh) => oh.play_uri(uri, meta),
            MusicRendererBackend::LinkPlay(lp) => lp.play_uri(uri, meta),
            MusicRendererBackend::ArylicTcp(_) => Err(op_not_supported("play_uri", "ArylicTcp")),
            MusicRendererBackend::Chromecast(cc) => cc.play_uri(uri, meta),
            MusicRendererBackend::HybridUpnpArylic { upnp, .. } => upnp.play_uri(uri, meta),
        }
    }

    fn play(&self) -> Result<()> {
        match self {
            MusicRendererBackend::Upnp(upnp) => upnp.play(),
            MusicRendererBackend::OpenHome(oh) => oh.play(),
            MusicRendererBackend::LinkPlay(lp) => lp.play(),
            MusicRendererBackend::ArylicTcp(ary) => ary.play(),
            MusicRendererBackend::Chromecast(cc) => cc.play(),
            MusicRendererBackend::HybridUpnpArylic { arylic, .. } => arylic.play(),
        }
    }

    fn pause(&self) -> Result<()> {
        match self {
            MusicRendererBackend::Upnp(upnp) => upnp.pause(),
            MusicRendererBackend::OpenHome(oh) => oh.pause(),
            MusicRendererBackend::LinkPlay(lp) => lp.pause(),
            MusicRendererBackend::ArylicTcp(ary) => ary.pause(),
            MusicRendererBackend::Chromecast(cc) => cc.pause(),
            MusicRendererBackend::HybridUpnpArylic { arylic, .. } => arylic.pause(),
        }
    }

    fn stop(&self) -> Result<()> {
        match self {
            MusicRendererBackend::Upnp(upnp) => upnp.stop(),
            MusicRendererBackend::OpenHome(oh) => oh.stop(),
            MusicRendererBackend::LinkPlay(lp) => lp.stop(),
            MusicRendererBackend::ArylicTcp(ary) => ary.stop(),
            MusicRendererBackend::Chromecast(cc) => cc.stop(),
            MusicRendererBackend::HybridUpnpArylic { arylic, .. } => arylic.stop(),
        }
    }

    fn seek_rel_time(&self, hhmmss: &str) -> Result<()> {
        match self {
            MusicRendererBackend::Upnp(upnp) => upnp.seek_rel_time(hhmmss),
            MusicRendererBackend::OpenHome(oh) => oh.seek_rel_time(hhmmss),
            MusicRendererBackend::LinkPlay(lp) => lp.seek_rel_time(hhmmss),
            MusicRendererBackend::ArylicTcp(_) => {
                Err(op_not_supported("seek_rel_time", "ArylicTcp"))
            }
            MusicRendererBackend::Chromecast(cc) => cc.seek_rel_time(hhmmss),
            MusicRendererBackend::HybridUpnpArylic { upnp, .. } => upnp.seek_rel_time(hhmmss),
        }
    }
}

/// Volume and mute controls exposed via the façade.
///
/// Hybrid backends may read via Arylic TCP and write via UPnP, but callers
/// always depend on a single [`VolumeControl`] entry point.
impl VolumeControl for MusicRendererBackend {
    fn volume(&self) -> Result<u16> {
        match self {
            MusicRendererBackend::HybridUpnpArylic { arylic, .. } => arylic.volume(),
            MusicRendererBackend::ArylicTcp(ary) => ary.volume(),
            MusicRendererBackend::OpenHome(oh) => oh.volume(),
            MusicRendererBackend::Upnp(upnp) => upnp.volume(),
            MusicRendererBackend::LinkPlay(lp) => lp.volume(),
            MusicRendererBackend::Chromecast(cc) => cc.volume(),
        }
    }

    fn set_volume(&self, vol: u16) -> Result<()> {
        match self {
            MusicRendererBackend::HybridUpnpArylic { upnp, .. } => upnp.set_volume(vol),
            MusicRendererBackend::ArylicTcp(ary) => ary.set_volume(vol),
            MusicRendererBackend::OpenHome(oh) => oh.set_volume(vol),
            MusicRendererBackend::Upnp(upnp) => upnp.set_volume(vol),
            MusicRendererBackend::LinkPlay(lp) => lp.set_volume(vol),
            MusicRendererBackend::Chromecast(cc) => cc.set_volume(vol),
        }
    }

    fn mute(&self) -> Result<bool> {
        match self {
            MusicRendererBackend::HybridUpnpArylic { arylic, .. } => arylic.mute(),
            MusicRendererBackend::OpenHome(r) => r.mute(),
            MusicRendererBackend::Upnp(r) => r.get_master_mute(),
            MusicRendererBackend::LinkPlay(r) => r.mute(),
            MusicRendererBackend::ArylicTcp(r) => r.mute(),
            MusicRendererBackend::Chromecast(cc) => cc.mute(),
        }
    }

    fn set_mute(&self, m: bool) -> Result<()> {
        match self {
            MusicRendererBackend::HybridUpnpArylic { arylic, .. } => arylic.set_mute(m),
            MusicRendererBackend::OpenHome(r) => r.set_mute(m),
            MusicRendererBackend::Upnp(r) => r.set_master_mute(m),
            MusicRendererBackend::LinkPlay(r) => r.set_mute(m),
            MusicRendererBackend::ArylicTcp(r) => r.set_mute(m),
            MusicRendererBackend::Chromecast(cc) => cc.set_mute(m),
        }
    }
}

/// Playback-state queries sourced from the backend best suited for the job.
///
/// Each backend reports into [`PlaybackState`], ensuring consumers never have
/// to reason about protocol-specific state machines.
impl PlaybackStatus for MusicRendererBackend {
    fn playback_state(&self) -> Result<PlaybackState> {
        match self {
            MusicRendererBackend::Upnp(r) => PlaybackStatus::playback_state(r),
            MusicRendererBackend::OpenHome(r) => PlaybackStatus::playback_state(r),
            MusicRendererBackend::LinkPlay(r) => r.playback_state(),
            MusicRendererBackend::ArylicTcp(r) => r.playback_state(),
            MusicRendererBackend::Chromecast(cc) => cc.playback_state(),
            MusicRendererBackend::HybridUpnpArylic { arylic, .. } => arylic.playback_state(),
        }
    }
}

/// Playback-position queries that always yield a [`PlaybackPositionInfo`]
/// regardless of the backend providing the raw transport data.
impl PlaybackPosition for MusicRendererBackend {
    fn playback_position(&self) -> Result<PlaybackPositionInfo> {
        match self {
            MusicRendererBackend::Upnp(r) => r.playback_position(),
            MusicRendererBackend::OpenHome(r) => r.playback_position(),
            MusicRendererBackend::LinkPlay(r) => r.playback_position(),
            MusicRendererBackend::ArylicTcp(r) => r.playback_position(),
            MusicRendererBackend::Chromecast(cc) => cc.playback_position(),
            MusicRendererBackend::HybridUpnpArylic { arylic, .. } => arylic.playback_position(),
        }
    }
}
