//! Backend-agnostic music renderer façade for PMOMusic.
//!
//! `MusicRenderer` wraps every supported backend (UPnP AV/DLNA, OpenHome,
//! LinkPlay HTTP, Arylic TCP, Chromecast, and the hybrid UPnP + Arylic pairing) behind a
//! single control surface. Higher layers in PMOMusic must only interact with
//! renderers through this type so that transport, volume, and state queries
//! stay backend-neutral.

use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use crate::errors::ControlPointError;
use crate::model::{PlaybackSource, PlaybackState, RendererInfo, RendererProtocol, TrackMetadata};
use crate::music_renderer::RendererFromMediaRendererInfo;
use crate::music_renderer::arylic_tcp::ArylicTcpRenderer;
use crate::music_renderer::capabilities::{
    PlaybackPosition, PlaybackPositionInfo, PlaybackStatus, TransportControl, VolumeControl,
};
use crate::music_renderer::chromecast_renderer::ChromecastRenderer;
use crate::music_renderer::linkplay_renderer::LinkPlayRenderer;
use crate::music_renderer::openhome_renderer::OpenHomeRenderer;
use crate::music_renderer::upnp_renderer::UpnpRenderer;
use crate::online::DeviceConnectionState;
use crate::queue::{
    EnqueueMode, MusicQueue, PlaybackItem, QueueBackend, QueueFromRendererInfo, QueueSnapshot,
};
use crate::{DeviceId, DeviceIdentity, DeviceOnline};

use tracing::warn;

/// Describes a renderer's attachment to a media server playlist container.
///
/// When a renderer is attached to a playlist, the ControlPoint monitors the container
/// for updates and automatically refreshes the queue when changes are detected.
#[derive(Clone, Debug)]
pub struct PlaylistBinding {
    /// MediaServer that owns the playlist container.
    pub server_id: DeviceId,
    /// DIDL-Lite object id of the playlist container.
    pub container_id: String,
    /// True once at least one ContainerUpdateIDs notification has been seen.
    pub(crate) has_seen_update: bool,
    /// Flag used internally to signal that the queue should be refreshed
    /// from the server container.
    pub(crate) pending_refresh: bool,
    /// Whether the next refresh should auto-start playback if the renderer is idle.
    pub(crate) auto_play_on_refresh: bool,
}

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

/// Internal state for tracking playback and control flow.
#[derive(Debug, Clone, Default)]
struct MusicRendererState {
    /// Last known track metadata (cached to avoid repeated queries).
    last_metadata: Option<TrackMetadata>,
    /// Source of the current playback (queue vs external).
    playback_source: PlaybackSource,
    /// Flag to distinguish user-requested stop from automatic events.
    user_stop_requested: bool,
}

#[derive(Debug, Clone)]
pub struct MusicRenderer {
    info: RendererInfo,
    connection: Arc<Mutex<DeviceConnectionState>>,
    backend: Arc<Mutex<MusicRendererBackend>>,
    queue: Arc<Mutex<MusicQueue>>,
    playlist_binding: Arc<Mutex<Option<PlaylistBinding>>>,
    state: Arc<Mutex<MusicRendererState>>,
}

impl MusicRenderer {
    pub fn new(
        info: RendererInfo,
        backend: Arc<Mutex<MusicRendererBackend>>,
        queue: Arc<Mutex<MusicQueue>>,
    ) -> Arc<MusicRenderer> {
        let connection = DeviceConnectionState::new();

        let renderer = MusicRenderer {
            info,
            connection: Arc::new(Mutex::new(connection)),
            backend,
            queue,
            playlist_binding: Arc::new(Mutex::new(None)),
            state: Arc::new(Mutex::new(MusicRendererState::default())),
        };

        Arc::new(renderer)
    }

    pub fn from_renderer_info(info: &RendererInfo) -> Result<MusicRenderer, ControlPointError> {
        let connection = Arc::new(Mutex::new(DeviceConnectionState::new()));
        let backend = MusicRendererBackend::make_from_renderer_info(info)?;
        let queue = MusicQueue::make_from_renderer_info(info)?;

        let renderer = MusicRenderer {
            info: info.clone(),
            connection,
            backend,
            queue,
            playlist_binding: Arc::new(Mutex::new(None)),
            state: Arc::new(Mutex::new(MusicRendererState::default())),
        };
        Ok(renderer)
    }

    pub fn info(&self) -> &RendererInfo {
        &self.info
    }

    /// Returns the protocol.
    pub fn protocol(&self) -> RendererProtocol {
        self.info.protocol()
    }

    pub fn is_upnp(&self) -> bool {
        match &*self.backend.lock().expect("Backend mutex poisoned") {
            MusicRendererBackend::Upnp(_) => true,
            _ => false,
        }
    }

    pub fn is_openhome(&self) -> bool {
        match &*self.backend.lock().expect("Backend mutex poisoned") {
            MusicRendererBackend::OpenHome(_) => true,
            _ => false,
        }
    }

    pub fn is_linkplay(&self) -> bool {
        match &*self.backend.lock().expect("Backend mutex poisoned") {
            MusicRendererBackend::LinkPlay(_) => true,
            _ => false,
        }
    }

    pub fn is_arylictcp(&self) -> bool {
        match &*self.backend.lock().expect("Backend mutex poisoned") {
            MusicRendererBackend::ArylicTcp(_) => true,
            _ => false,
        }
    }

    pub fn is_chromecast(&self) -> bool {
        match &*self.backend.lock().expect("Backend mutex poisoned") {
            MusicRendererBackend::Chromecast(_) => true,
            _ => false,
        }
    }

    pub fn is_hybridupnparylic(&self) -> bool {
        match &*self.backend.lock().expect("Backend mutex poisoned") {
            MusicRendererBackend::HybridUpnpArylic { .. } => true,
            _ => false,
        }
    }

    /// Returns true if this renderer is known to support SetNextAVTransportURI.
    pub fn supports_set_next(&self) -> bool {
        self.info.capabilities().supports_set_next()
    }

    /// Prepare the renderer for attaching a new playlist by clearing the queue and stopping playback.
    pub fn clear_for_playlist_attach(&self) -> Result<(), ControlPointError> {
        // Clear the queue first
        self.queue
            .lock()
            .expect("Queue mutex poisoned")
            .clear_queue()?;

        // Then stop playback (ignore errors if already stopped)
        self.backend
            .lock()
            .expect("Backend mutex poisoned")
            .stop()
            .or_else(|err| {
                warn!(
                    renderer = self.id().0.as_str(),
                    error = %err,
                    "Stop failed when preparing for playlist attach (continuing anyway)"
                );
                Ok(())
            })
    }

    /// Get the current queue snapshot.
    pub fn queue_snapshot(&self) -> Result<QueueSnapshot, ControlPointError> {
        self.queue
            .lock()
            .expect("Queue mutex poisoned")
            .queue_snapshot()
    }

    /// Get the current queue item without advancing.
    /// Returns the item and count of remaining items after current.
    pub fn peek_current(&self) -> Result<Option<(PlaybackItem, usize)>, ControlPointError> {
        self.queue
            .lock()
            .expect("Queue mutex poisoned")
            .peek_current()
    }

    /// Get the count of items remaining after the current index.
    pub fn upcoming_len(&self) -> Result<usize, ControlPointError> {
        self.queue
            .lock()
            .expect("Queue mutex poisoned")
            .upcoming_len()
    }

    /// Play the current item from the queue.
    pub fn play_current_from_queue(&self) -> Result<(), ControlPointError> {
        let queue = self.queue.lock().expect("Queue mutex poisoned");
        let backend = self.backend.lock().expect("Backend mutex poisoned");

        // Get the current item from the queue
        let (item, _remaining) = queue.peek_current()?.ok_or_else(|| {
            ControlPointError::QueueError("Queue is empty, cannot play current".to_string())
        })?;

        // Build DIDL metadata if available
        let metadata_xml = item
            .metadata
            .as_ref()
            .map(|m| build_didl_lite_metadata(m, &item.uri, &item.protocol_info))
            .unwrap_or_default();

        // Play the URI using the backend
        backend.play_uri(&item.uri, &metadata_xml)?;

        Ok(())
    }

    /// Advance to and play the next item from the queue.
    pub fn play_next_from_queue(&self) -> Result<(), ControlPointError> {
        let mut queue = self.queue.lock().expect("Queue mutex poisoned");
        let backend = self.backend.lock().expect("Backend mutex poisoned");

        // Dequeue the next item
        let (item, _remaining) = queue
            .dequeue_next()?
            .ok_or_else(|| ControlPointError::QueueError("No next item in queue".to_string()))?;

        // Build DIDL metadata if available
        let metadata_xml = item
            .metadata
            .as_ref()
            .map(|m| build_didl_lite_metadata(m, &item.uri, &item.protocol_info))
            .unwrap_or_default();

        // Play the URI using the backend
        backend.play_uri(&item.uri, &metadata_xml)?;

        Ok(())
    }

    /// Transport control: play URI with metadata
    pub fn play_uri(&self, uri: &str, metadata: &str) -> Result<(), ControlPointError> {
        self.backend
            .lock()
            .expect("Backend mutex poisoned")
            .play_uri(uri, metadata)
    }

    /// Transport control: play
    pub fn play(&self) -> Result<(), ControlPointError> {
        self.backend.lock().expect("Backend mutex poisoned").play()
    }

    /// Transport control: pause
    pub fn pause(&self) -> Result<(), ControlPointError> {
        self.backend.lock().expect("Backend mutex poisoned").pause()
    }

    /// Transport control: stop
    pub fn stop(&self) -> Result<(), ControlPointError> {
        self.backend.lock().expect("Backend mutex poisoned").stop()
    }

    /// Set the next URI for gapless playback (UPnP AVTransport only).
    ///
    /// Returns Ok if the backend supports this feature and it succeeded.
    /// Returns Err if not supported or if it failed.
    pub fn set_next_uri(&self, uri: &str, metadata: &str) -> Result<(), ControlPointError> {
        let backend = self.backend.lock().expect("Backend mutex poisoned");

        match &*backend {
            MusicRendererBackend::Upnp(upnp) => upnp.set_next_uri(uri, metadata),
            MusicRendererBackend::HybridUpnpArylic { upnp, .. } => upnp.set_next_uri(uri, metadata),
            _ => Err(ControlPointError::ControlPoint(
                "SetNextURI not supported by this backend".to_string(),
            )),
        }
    }

    /// Transport control: seek to relative time
    pub fn seek_rel_time(&self, hhmmss: &str) -> Result<(), ControlPointError> {
        self.backend
            .lock()
            .expect("Backend mutex poisoned")
            .seek_rel_time(hhmmss)
    }

    /// Volume control: get current volume
    pub fn volume(&self) -> Result<u16, ControlPointError> {
        self.backend
            .lock()
            .expect("Backend mutex poisoned")
            .volume()
    }

    /// Volume control: set volume
    pub fn set_volume(&self, vol: u16) -> Result<(), ControlPointError> {
        self.backend
            .lock()
            .expect("Backend mutex poisoned")
            .set_volume(vol)
    }

    /// Volume control: get mute state
    pub fn mute(&self) -> Result<bool, ControlPointError> {
        self.backend.lock().expect("Backend mutex poisoned").mute()
    }

    /// Volume control: set mute state
    pub fn set_mute(&self, m: bool) -> Result<(), ControlPointError> {
        self.backend
            .lock()
            .expect("Backend mutex poisoned")
            .set_mute(m)
    }

    /// Get playback state
    pub fn playback_state(&self) -> Result<PlaybackState, ControlPointError> {
        self.backend
            .lock()
            .expect("Backend mutex poisoned")
            .playback_state()
    }

    /// Get playback position
    pub fn playback_position(&self) -> Result<PlaybackPositionInfo, ControlPointError> {
        self.backend
            .lock()
            .expect("Backend mutex poisoned")
            .playback_position()
    }

    /// Sets the playlist binding for this renderer.
    pub fn set_playlist_binding(&self, binding: Option<PlaylistBinding>) {
        *self
            .playlist_binding
            .lock()
            .expect("Playlist binding mutex poisoned") = binding;
    }

    /// Gets the current playlist binding, if any.
    pub fn get_playlist_binding(&self) -> Option<PlaylistBinding> {
        self.playlist_binding
            .lock()
            .expect("Playlist binding mutex poisoned")
            .clone()
    }

    /// Clears the playlist binding.
    pub fn clear_playlist_binding(&self) {
        *self
            .playlist_binding
            .lock()
            .expect("Playlist binding mutex poisoned") = None;
    }

    /// Marks the current playlist binding for refresh if it matches the given server and container.
    /// Returns true if a matching binding was found and marked.
    pub fn mark_binding_for_refresh(&self, server_id: &DeviceId, container_ids: &[String]) -> bool {
        let mut binding_guard = self
            .playlist_binding
            .lock()
            .expect("Playlist binding mutex poisoned");

        if let Some(binding) = binding_guard.as_mut() {
            if &binding.server_id == server_id && container_ids.contains(&binding.container_id) {
                binding.pending_refresh = true;
                binding.has_seen_update = true;
                return true;
            }
        }

        false
    }

    /// Checks if the binding has pending_refresh flag set.
    /// Returns false if no binding exists.
    pub fn has_pending_refresh(&self) -> bool {
        self.playlist_binding
            .lock()
            .expect("Playlist binding mutex poisoned")
            .as_ref()
            .map(|b| b.pending_refresh)
            .unwrap_or(false)
    }

    /// Resets the pending_refresh flag to false.
    /// Does nothing if no binding exists.
    pub fn reset_pending_refresh(&self) {
        if let Some(binding) = self
            .playlist_binding
            .lock()
            .expect("Playlist binding mutex poisoned")
            .as_mut()
        {
            binding.pending_refresh = false;
        }
    }

    /// Marks the binding as pending refresh.
    /// Returns true if a binding exists and was marked, false otherwise.
    pub fn mark_pending_refresh(&self) -> bool {
        self.playlist_binding
            .lock()
            .expect("Playlist binding mutex poisoned")
            .as_mut()
            .map(|binding| {
                binding.pending_refresh = true;
                true
            })
            .unwrap_or(false)
    }

    /// Consumes and returns the auto_play_on_refresh flag, resetting it to false.
    /// Returns false if no binding exists.
    pub fn consume_auto_play(&self) -> bool {
        self.playlist_binding
            .lock()
            .expect("Playlist binding mutex poisoned")
            .as_mut()
            .map(|binding| {
                let auto_play = binding.auto_play_on_refresh;
                binding.auto_play_on_refresh = false;
                auto_play
            })
            .unwrap_or(false)
    }

    /// Add items to the queue using the specified enqueue mode.
    pub fn enqueue_items(
        &self,
        items: Vec<PlaybackItem>,
        mode: EnqueueMode,
    ) -> Result<(), ControlPointError> {
        let mut queue = self.queue.lock().expect("Queue mutex poisoned");
        queue.enqueue_items(items, mode)
    }

    /// Synchronize the queue with new items while preserving the current track.
    ///
    /// This method intelligently updates the queue:
    /// - If the current track is in the new items, it keeps playing at the new position
    /// - If the current track is NOT in the new items, it's preserved as the first item
    /// - If there's no current track, the queue is simply replaced
    pub fn sync_queue(&self, items: Vec<PlaybackItem>) -> Result<(), ControlPointError> {
        let mut queue = self.queue.lock().expect("Queue mutex poisoned");
        queue.sync_queue(items)
    }

    /// Set the current queue index (for advanced use).
    /// Note: This does NOT start playback. Use select_queue_track() to play.
    pub fn set_queue_index(&self, index: Option<usize>) -> Result<(), ControlPointError> {
        let mut queue = self.queue.lock().expect("Queue mutex poisoned");
        queue.set_index(index)
    }

    /// Clears the renderer's queue using the generic QueueBackend trait.
    pub fn clear_queue(&self) -> Result<(), ControlPointError> {
        let mut queue = self.queue.lock().expect("Queue mutex poisoned");
        queue.clear_queue()
    }

    /// Adds a track to the queue.
    ///
    /// This is primarily for backends with persistent queues (OpenHome).
    /// For backends without this capability, it returns an error.
    ///
    /// Returns the backend-specific track ID if applicable.
    pub fn add_track_to_queue(
        &self,
        _uri: &str,
        _metadata: &str,
        _after_id: Option<u32>,
        _play: bool,
    ) -> Result<Option<u32>, ControlPointError> {
        // This operation requires backend-specific APIs (especially for OpenHome)
        // that aren't exposed through the generic QueueBackend trait.
        // The generic implementation cannot support this without more context
        // (media_server_id, didl_id, protocol_info, etc.).
        Err(ControlPointError::QueueError(
            "add_track_to_queue requires backend-specific implementation".to_string(),
        ))
    }

    /// Selects and plays a specific track from the queue by ID.
    ///
    /// Converts the track ID to a position using the generic QueueBackend trait,
    /// then plays the track.
    pub fn select_queue_track(&self, track_id: u32) -> Result<(), ControlPointError> {
        let queue = self.queue.lock().expect("Queue mutex poisoned");

        // Convert track ID to position using generic QueueBackend trait
        let position = queue.id_to_position(track_id)?;
        drop(queue);

        // Set the index
        let mut queue = self.queue.lock().expect("Queue mutex poisoned");
        queue.set_index(Some(position))?;

        // Get the item to play
        let item = queue
            .get_item(position)?
            .ok_or_else(|| ControlPointError::QueueError("Track not found".to_string()))?;
        drop(queue);

        // Build metadata XML
        let metadata_xml = item
            .metadata
            .as_ref()
            .map(|m| build_didl_lite_metadata(m, &item.uri, &item.protocol_info))
            .unwrap_or_default();

        // Play the item using TransportControl
        let backend = self.backend.lock().expect("Backend mutex poisoned");
        backend.play_uri(&item.uri, &metadata_xml)?;

        Ok(())
    }

    /// Synchronizes the queue state with the backend.
    ///
    /// For backends with persistent queues (OpenHome), this refreshes the local view.
    /// For others, this is essentially a no-op (just reads the current state).
    pub fn sync_queue_state(&self) -> Result<(), ControlPointError> {
        let queue = self.queue.lock().expect("Queue mutex poisoned");
        // Calling queue_snapshot() triggers a refresh for backends that need it
        let _ = queue.queue_snapshot()?;
        Ok(())
    }

    /// Plays the current item from the backend queue.
    ///
    /// This is primarily for backends with persistent queues (OpenHome).
    pub fn play_current_from_backend_queue(&self) -> Result<(), ControlPointError> {
        let queue = self.queue.lock().expect("Queue mutex poisoned");

        // Get current track ID using generic QueueBackend trait
        let track_id = queue
            .current_track()?
            .ok_or_else(|| ControlPointError::QueueError("No current track".to_string()))?;

        drop(queue);

        // Play it using select_queue_track
        self.select_queue_track(track_id)
    }

    /// Returns a reference to the queue (read-only access via lock).
    /// This is a convenience method to avoid repetitive `queue.lock().unwrap()` patterns.
    pub fn get_queue(&self) -> std::sync::MutexGuard<'_, MusicQueue> {
        self.queue.lock().unwrap()
    }

    /// Returns a mutable reference to the queue (write access via lock).
    /// This is a convenience method to avoid repetitive `queue.lock().unwrap()` patterns.
    pub fn get_queue_mut(&self) -> std::sync::MutexGuard<'_, MusicQueue> {
        self.queue.lock().unwrap()
    }

    // --- Playback State Management ---

    /// Gets the last known track metadata.
    pub fn last_metadata(&self) -> Option<TrackMetadata> {
        self.state.lock().unwrap().last_metadata.clone()
    }

    /// Sets the last known track metadata.
    pub fn set_last_metadata(&self, metadata: Option<TrackMetadata>) {
        self.state.lock().unwrap().last_metadata = metadata;
    }

    /// Gets the current playback source.
    pub fn playback_source(&self) -> PlaybackSource {
        self.state.lock().unwrap().playback_source
    }

    /// Sets the playback source.
    pub fn set_playback_source(&self, source: PlaybackSource) {
        self.state.lock().unwrap().playback_source = source;
    }

    /// Checks if currently playing from queue.
    pub fn is_playing_from_queue(&self) -> bool {
        matches!(
            self.state.lock().unwrap().playback_source,
            PlaybackSource::FromQueue
        )
    }

    /// Marks playback as external if currently idle (source is None).
    pub fn mark_external_if_idle(&self) {
        let mut state = self.state.lock().unwrap();
        if matches!(state.playback_source, PlaybackSource::None) {
            state.playback_source = PlaybackSource::External;
        }
    }

    /// Marks that the user requested a stop (to prevent auto-advance).
    pub fn mark_user_stop_requested(&self) {
        self.state.lock().unwrap().user_stop_requested = true;
    }

    /// Checks and clears the user stop requested flag.
    pub fn check_and_clear_user_stop_requested(&self) -> bool {
        let mut state = self.state.lock().unwrap();
        let was_requested = state.user_stop_requested;
        state.user_stop_requested = false;
        was_requested
    }
}

/// Helper function to build DIDL-Lite metadata XML from TrackMetadata
fn build_didl_lite_metadata(metadata: &TrackMetadata, uri: &str, protocol_info: &str) -> String {
    format!(
        r#"<DIDL-Lite xmlns="urn:schemas-upnp-org:metadata-1-0/DIDL-Lite/" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:upnp="urn:schemas-upnp-org:metadata-1-0/upnp/">
<item id="0" parentID="-1" restricted="1">
<dc:title>{}</dc:title>
<dc:creator>{}</dc:creator>
<upnp:artist>{}</upnp:artist>
<upnp:album>{}</upnp:album>
{}
<res protocolInfo="{}">{}</res>
</item>
</DIDL-Lite>"#,
        metadata.title.as_deref().unwrap_or("Unknown Title"),
        metadata
            .creator
            .as_deref()
            .or(metadata.artist.as_deref())
            .unwrap_or("Unknown Artist"),
        metadata.artist.as_deref().unwrap_or("Unknown Artist"),
        metadata.album.as_deref().unwrap_or("Unknown Album"),
        metadata
            .album_art_uri
            .as_ref()
            .map(|art_uri| format!("<upnp:albumArtURI>{}</upnp:albumArtURI>", art_uri))
            .unwrap_or_default(),
        protocol_info,
        uri
    )
}

impl DeviceIdentity for MusicRenderer {
    fn id(&self) -> DeviceId {
        self.info.id()
    }

    fn udn(&self) -> &str {
        &*self.info.udn()
    }

    fn friendly_name(&self) -> &str {
        &self.info.friendly_name()
    }

    fn model_name(&self) -> &str {
        &self.info.model_name()
    }
    fn manufacturer(&self) -> &str {
        &self.info.manufacturer()
    }
    fn location(&self) -> &str {
        &self.info.location()
    }
    fn server_header(&self) -> &str {
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
    }
}

impl RendererFromMediaRendererInfo for MusicRendererBackend {
    fn from_renderer_info(info: &RendererInfo) -> Result<Self, ControlPointError> {
        // Try OpenHome first for OpenHome-capable renderers
        match info.protocol() {
            RendererProtocol::OpenHomeOnly | RendererProtocol::OpenHomeHybrid => {
                if let Ok(backend) = OpenHomeRenderer::build_from_renderer_info(info) {
                    return Ok(backend);
                }
                // If OpenHomeOnly failed, it's an error
                if matches!(info.protocol(), RendererProtocol::OpenHomeOnly) {
                    return Err(ControlPointError::MusicRendererBackendBuild(format!(
                        "OpenHomeOnly renderer {} has no usable OpenHome services",
                        info.friendly_name()
                    )));
                }
                // OpenHomeHybrid: fall through to try UPnP-based backends
            }
            RendererProtocol::ChromecastOnly => {
                return ChromecastRenderer::build_from_renderer_info(info);
            }
            RendererProtocol::UpnpAvOnly => {
                // Will be handled below
            }
        }

        // UPnP-based renderers (UpnpAvOnly or OpenHomeHybrid fallback)
        let has_arylic = info.capabilities().has_arylic_tcp;
        let has_avtransport = info.capabilities().has_avtransport();

        // Try Hybrid UPnP + Arylic (special case: combines two backends)
        if has_arylic && has_avtransport {
            let upnp_backend = UpnpRenderer::build_from_renderer_info(info)?;
            if let MusicRendererBackend::Upnp(upnp) = upnp_backend {
                match ArylicTcpRenderer::build_from_renderer_info(info) {
                    Ok(MusicRendererBackend::ArylicTcp(arylic)) => {
                        return Ok(MusicRendererBackend::HybridUpnpArylic { upnp, arylic });
                    }
                    Err(err) => {
                        warn!(
                            renderer = info.friendly_name(),
                            error = %err,
                            "Failed to build Arylic TCP backend, falling back to UPnP only"
                        );
                        return Ok(MusicRendererBackend::Upnp(upnp));
                    }
                    _ => unreachable!(
                        "ArylicTcpRenderer::build_from_renderer_info should return ArylicTcp variant"
                    ),
                }
            } else {
                unreachable!("UpnpRenderer::build_from_renderer_info should return Upnp variant");
            }
        }

        // Try LinkPlay
        if info.capabilities().has_linkplay_http() {
            if let Ok(backend) = LinkPlayRenderer::build_from_renderer_info(info) {
                return Ok(backend);
            }
        }

        // Fallback to UPnP
        if has_avtransport {
            return UpnpRenderer::build_from_renderer_info(info);
        }

        // No suitable backend found
        Err(ControlPointError::MusicRendererBackendBuild(format!(
            "No suitable backend for renderer {}",
            info.friendly_name()
        )))
    }

    fn to_backend(self) -> MusicRendererBackend {
        self
    }
}

/// Transport control façade that dispatches to whichever backend can fulfill
/// the request, returning a standardized error if the backend lacks support.
impl TransportControl for MusicRendererBackend {
    fn play_uri(&self, uri: &str, meta: &str) -> Result<(), ControlPointError> {
        match self {
            MusicRendererBackend::Upnp(upnp) => upnp.play_uri(uri, meta),
            MusicRendererBackend::OpenHome(oh) => oh.play_uri(uri, meta),
            MusicRendererBackend::LinkPlay(lp) => lp.play_uri(uri, meta),
            MusicRendererBackend::ArylicTcp(_) => Err(
                ControlPointError::upnp_operation_not_supported("play_uri", "ArylicTcp"),
            ),
            MusicRendererBackend::Chromecast(cc) => cc.play_uri(uri, meta),
            MusicRendererBackend::HybridUpnpArylic { upnp, .. } => upnp.play_uri(uri, meta),
        }
    }

    fn play(&self) -> Result<(), ControlPointError> {
        match self {
            MusicRendererBackend::Upnp(upnp) => upnp.play(),
            MusicRendererBackend::OpenHome(oh) => oh.play(),
            MusicRendererBackend::LinkPlay(lp) => lp.play(),
            MusicRendererBackend::ArylicTcp(ary) => ary.play(),
            MusicRendererBackend::Chromecast(cc) => cc.play(),
            MusicRendererBackend::HybridUpnpArylic { arylic, .. } => arylic.play(),
        }
    }

    fn pause(&self) -> Result<(), ControlPointError> {
        match self {
            MusicRendererBackend::Upnp(upnp) => upnp.pause(),
            MusicRendererBackend::OpenHome(oh) => oh.pause(),
            MusicRendererBackend::LinkPlay(lp) => lp.pause(),
            MusicRendererBackend::ArylicTcp(ary) => ary.pause(),
            MusicRendererBackend::Chromecast(cc) => cc.pause(),
            MusicRendererBackend::HybridUpnpArylic { arylic, .. } => arylic.pause(),
        }
    }

    fn stop(&self) -> Result<(), ControlPointError> {
        match self {
            MusicRendererBackend::Upnp(upnp) => upnp.stop(),
            MusicRendererBackend::OpenHome(oh) => oh.stop(),
            MusicRendererBackend::LinkPlay(lp) => lp.stop(),
            MusicRendererBackend::ArylicTcp(ary) => ary.stop(),
            MusicRendererBackend::Chromecast(cc) => cc.stop(),
            MusicRendererBackend::HybridUpnpArylic { arylic, .. } => arylic.stop(),
        }
    }

    fn seek_rel_time(&self, hhmmss: &str) -> Result<(), ControlPointError> {
        match self {
            MusicRendererBackend::Upnp(upnp) => upnp.seek_rel_time(hhmmss),
            MusicRendererBackend::OpenHome(oh) => oh.seek_rel_time(hhmmss),
            MusicRendererBackend::LinkPlay(lp) => lp.seek_rel_time(hhmmss),
            MusicRendererBackend::ArylicTcp(_) => Err(
                ControlPointError::upnp_operation_not_supported("seek_rel_time", "ArylicTcp"),
            ),
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
    fn volume(&self) -> Result<u16, ControlPointError> {
        match self {
            MusicRendererBackend::HybridUpnpArylic { arylic, .. } => arylic.volume(),
            MusicRendererBackend::ArylicTcp(ary) => ary.volume(),
            MusicRendererBackend::OpenHome(oh) => oh.volume(),
            MusicRendererBackend::Upnp(upnp) => upnp.volume(),
            MusicRendererBackend::LinkPlay(lp) => lp.volume(),
            MusicRendererBackend::Chromecast(cc) => cc.volume(),
        }
    }

    fn set_volume(&self, vol: u16) -> Result<(), ControlPointError> {
        match self {
            MusicRendererBackend::HybridUpnpArylic { upnp, .. } => upnp.set_volume(vol),
            MusicRendererBackend::ArylicTcp(ary) => ary.set_volume(vol),
            MusicRendererBackend::OpenHome(oh) => oh.set_volume(vol),
            MusicRendererBackend::Upnp(upnp) => upnp.set_volume(vol),
            MusicRendererBackend::LinkPlay(lp) => lp.set_volume(vol),
            MusicRendererBackend::Chromecast(cc) => cc.set_volume(vol),
        }
    }

    fn mute(&self) -> Result<bool, ControlPointError> {
        match self {
            MusicRendererBackend::HybridUpnpArylic { arylic, .. } => arylic.mute(),
            MusicRendererBackend::OpenHome(r) => r.mute(),
            MusicRendererBackend::Upnp(r) => r.mute(),
            MusicRendererBackend::LinkPlay(r) => r.mute(),
            MusicRendererBackend::ArylicTcp(r) => r.mute(),
            MusicRendererBackend::Chromecast(cc) => cc.mute(),
        }
    }

    fn set_mute(&self, m: bool) -> Result<(), ControlPointError> {
        match self {
            MusicRendererBackend::HybridUpnpArylic { arylic, .. } => arylic.set_mute(m),
            MusicRendererBackend::OpenHome(r) => r.set_mute(m),
            MusicRendererBackend::Upnp(r) => r.set_mute(m),
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
    fn playback_state(&self) -> Result<PlaybackState, ControlPointError> {
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
    fn playback_position(&self) -> Result<PlaybackPositionInfo, ControlPointError> {
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
