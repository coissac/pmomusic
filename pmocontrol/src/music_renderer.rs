//! Backend-agnostic music renderer façade for PMOMusic.
//!
//! `MusicRenderer` wraps every supported backend (UPnP AV/DLNA, OpenHome,
//! LinkPlay HTTP, Arylic TCP, Chromecast, and the hybrid UPnP + Arylic pairing) behind a
//! single control surface. Higher layers in PMOMusic must only interact with
//! renderers through this type so that transport, volume, and state queries
//! stay backend-neutral.

use std::sync::{Arc, OnceLock, RwLock};

use crate::capabilities::{PlaybackPositionInfo, PlaybackStatus};
use crate::control_point::RendererRuntimeStateMut;
use crate::control_point::music_queue::MusicQueue;
use crate::control_point::openhome_queue::didl_id_from_metadata;
use crate::media_server::ServerId;
use crate::model::{RendererId, RendererInfo, RendererProtocol, TrackMetadata};
use crate::openhome_client::parse_track_metadata_from_didl;
use crate::openhome_playlist::{OpenHomePlaylistSnapshot, OpenHomePlaylistTrack};
use crate::queue_backend::{PlaybackItem, QueueSnapshot};
use crate::{
    ArylicTcpRenderer, ChromecastRenderer, DeviceRegistry, LinkPlayRenderer, OpenHomeRenderer,
    PlaybackPosition, PlaybackState, TransportControl, UpnpRenderer, VolumeControl,
};
use anyhow::{Result, anyhow};
use tracing::{debug, info, warn};

/// Backend-agnostic façade exposing transport, volume, and status contracts.
#[derive(Clone, Debug)]
pub enum MusicRenderer {
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

/// Build a standardized error when an operation is not supported by a backend.
pub(crate) fn op_not_supported(op: &str, backend: &str) -> anyhow::Error {
    anyhow!(
        "MusicRenderer operation '{}' is not supported by backend '{}'",
        op,
        backend
    )
}

#[derive(Clone, Debug)]
pub struct RendererRuntimeState {
    pub queue: MusicQueue,
}

pub trait OpenHomeQueueProvider: Send + Sync + 'static {
    fn renderer_state(&self, renderer_id: &RendererId) -> Result<RendererRuntimeState>;
    fn renderer_state_mut<'a>(
        &'a self,
        renderer_id: &RendererId,
    ) -> Result<RendererRuntimeStateMut<'a>>;
    fn invalidate_openhome_cache(&self, renderer_id: &RendererId) -> Result<()>;
}

static OPENHOME_QUEUE_PROVIDER: OnceLock<Arc<dyn OpenHomeQueueProvider>> = OnceLock::new();

pub fn set_openhome_queue_provider(provider: Arc<dyn OpenHomeQueueProvider>) {
    let _ = OPENHOME_QUEUE_PROVIDER.set(provider);
}

impl MusicRenderer {
    /// Renderer identifier (stable within the registry).
    pub fn id(&self) -> &RendererId {
        match self {
            MusicRenderer::HybridUpnpArylic { arylic, .. } => arylic.id(),
            MusicRenderer::OpenHome(r) => r.id(),
            MusicRenderer::Upnp(r) => r.id(),
            MusicRenderer::LinkPlay(r) => r.id(),
            MusicRenderer::ArylicTcp(r) => r.id(),
            MusicRenderer::Chromecast(r) => r.id(),
        }
    }

    /// Human-friendly name reported by the device.
    pub fn friendly_name(&self) -> &str {
        match self {
            MusicRenderer::HybridUpnpArylic { arylic, .. } => arylic.friendly_name(),
            MusicRenderer::OpenHome(r) => r.friendly_name(),
            MusicRenderer::Upnp(r) => r.friendly_name(),
            MusicRenderer::LinkPlay(r) => r.friendly_name(),
            MusicRenderer::ArylicTcp(r) => r.friendly_name(),
            MusicRenderer::Chromecast(r) => r.friendly_name(),
        }
    }

    /// Protocol classification (UPnP AV only, OpenHome only, hybrid).
    pub fn protocol(&self) -> &RendererProtocol {
        &self.info().protocol
    }

    /// Full static info as stored in the registry.
    pub fn info(&self) -> &RendererInfo {
        match self {
            MusicRenderer::HybridUpnpArylic { arylic, .. } => &arylic.info,
            MusicRenderer::OpenHome(r) => &r.info,
            MusicRenderer::Upnp(r) => &r.info,
            MusicRenderer::LinkPlay(r) => &r.info,
            MusicRenderer::ArylicTcp(r) => &r.info,
            MusicRenderer::Chromecast(r) => &r.info,
        }
    }

    /// Return a reference to the underlying UPnP backend, if any.
    pub fn as_upnp(&self) -> Option<&UpnpRenderer> {
        match self {
            MusicRenderer::Upnp(r) => Some(r),
            _ => None,
        }
    }

    /// Construct a music renderer from a [`RendererInfo`] and the registry.
    ///
    /// Returns `None` when no supported backend can be built for this renderer.
    /// UPnP AV / hybrid renderers map either to [`MusicRenderer::LinkPlay`] (when supported)
    /// or [`MusicRenderer::Upnp`].
    pub fn from_registry_info(
        info: RendererInfo,
        registry: &Arc<RwLock<DeviceRegistry>>,
    ) -> Option<Self> {
        if matches!(
            info.protocol,
            RendererProtocol::OpenHomeOnly | RendererProtocol::Hybrid
        ) {
            if let Some(renderer) = {
                let renderer = OpenHomeRenderer::new(info.clone());
                renderer.has_any_openhome_service().then_some(renderer)
            } {
                return Some(MusicRenderer::OpenHome(renderer));
            }

            if matches!(info.protocol, RendererProtocol::OpenHomeOnly) {
                warn!(
                    renderer = info.friendly_name.as_str(),
                    "Renderer advertises OpenHome only but exposes no usable services"
                );
                return None;
            }
        }

        if matches!(info.protocol, RendererProtocol::ChromecastOnly) {
            if let Ok(renderer) = ChromecastRenderer::from_renderer_info(info.clone()) {
                return Some(MusicRenderer::Chromecast(renderer));
            }
            warn!(
                renderer = info.friendly_name.as_str(),
                "Failed to build Chromecast renderer"
            );
            return None;
        }

        match info.protocol {
            RendererProtocol::UpnpAvOnly | RendererProtocol::Hybrid => {
                let has_arylic = info.capabilities.has_arylic_tcp;
                let has_avtransport = info.capabilities.has_avtransport;

                if has_arylic && has_avtransport {
                    // Construire UpnpRenderer
                    let upnp = UpnpRenderer::from_registry(info.clone(), registry);

                    // Construire ArylicTcpRenderer
                    match ArylicTcpRenderer::from_renderer_info(info.clone()) {
                        Ok(arylic) => {
                            return Some(MusicRenderer::HybridUpnpArylic { upnp, arylic });
                        }
                        Err(err) => {
                            warn!(
                                "Failed to build Arylic TCP backend for {}: {}. Falling back to UPnP only.",
                                info.friendly_name, err
                            );
                            return Some(MusicRenderer::Upnp(upnp));
                        }
                    }
                }

                // Pas d’Arylic : logique existante
                if info.capabilities.has_linkplay_http {
                    if let Ok(lp) = LinkPlayRenderer::from_renderer_info(info.clone()) {
                        return Some(MusicRenderer::LinkPlay(lp));
                    }
                }

                Some(MusicRenderer::Upnp(UpnpRenderer::from_registry(
                    info, registry,
                )))
            }
            RendererProtocol::OpenHomeOnly => None,
            RendererProtocol::ChromecastOnly => None,
        }
    }

    pub fn openhome_playlist_snapshot(&self) -> Result<OpenHomePlaylistSnapshot> {
        self.fetch_openhome_playlist_snapshot()
    }

    pub(crate) fn fetch_openhome_playlist_snapshot(&self) -> Result<OpenHomePlaylistSnapshot> {
        match self {
            MusicRenderer::OpenHome(renderer) => renderer.snapshot_openhome_playlist(),
            _ => Err(op_not_supported(
                "openhome_playlist_snapshot",
                self.unsupported_backend_name(),
            )),
        }
    }

    pub fn openhome_playlist_len(&self) -> Result<usize> {
        match self {
            MusicRenderer::OpenHome(renderer) => renderer.openhome_playlist_len(),
            _ => Err(op_not_supported(
                "openhome_playlist_len",
                self.unsupported_backend_name(),
            )),
        }
    }

    pub fn openhome_playlist_ids(&self) -> Result<Vec<u32>> {
        match self {
            MusicRenderer::OpenHome(renderer) => renderer.openhome_playlist_ids(),
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
            MusicRenderer::OpenHome(renderer) => renderer.clear_openhome_playlist(),
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
            MusicRenderer::OpenHome(_) => {
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
            MusicRenderer::Upnp(_)
            | MusicRenderer::Chromecast(_)
            | MusicRenderer::LinkPlay(_)
            | MusicRenderer::ArylicTcp(_)
            | MusicRenderer::HybridUpnpArylic { .. } => {
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
            MusicRenderer::OpenHome(_) => {
                if let Some(provider) = OPENHOME_QUEUE_PROVIDER.get() {
                    // Fetch fresh playlist snapshot and update cache
                    provider.invalidate_openhome_cache(self.id())?;
                }
                Ok(())
            }
            MusicRenderer::Upnp(_)
            | MusicRenderer::Chromecast(_)
            | MusicRenderer::LinkPlay(_)
            | MusicRenderer::ArylicTcp(_)
            | MusicRenderer::HybridUpnpArylic { .. } => {
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
            MusicRenderer::OpenHome(_) => {
                // For OpenHome: clear the playlist on the renderer itself
                self.openhome_playlist_clear()
            }
            MusicRenderer::Upnp(_)
            | MusicRenderer::Chromecast(_)
            | MusicRenderer::LinkPlay(_)
            | MusicRenderer::ArylicTcp(_)
            | MusicRenderer::HybridUpnpArylic { .. } => {
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
            MusicRenderer::OpenHome(_) => {
                let track_id = self.openhome_playlist_add_track(uri, metadata, after_id, play)?;
                Ok(Some(track_id))
            }
            MusicRenderer::Upnp(_)
            | MusicRenderer::Chromecast(_)
            | MusicRenderer::LinkPlay(_)
            | MusicRenderer::ArylicTcp(_)
            | MusicRenderer::HybridUpnpArylic { .. } => Err(anyhow!(
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
            MusicRenderer::OpenHome(_) => self.openhome_playlist_play_id(track_id),
            MusicRenderer::Upnp(_)
            | MusicRenderer::Chromecast(_)
            | MusicRenderer::LinkPlay(_)
            | MusicRenderer::ArylicTcp(_)
            | MusicRenderer::HybridUpnpArylic { .. } => Err(anyhow!(
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
            MusicRenderer::OpenHome(_) => {
                let oh_snapshot = self.fetch_openhome_playlist_snapshot()?;

                // Convert OpenHome tracks to PlaybackItems
                let items: Vec<PlaybackItem> = oh_snapshot.tracks.iter().map(|track| {
                    Self::playback_item_from_openhome_track(self.id(), track)
                }).collect();

                let snapshot = QueueSnapshot {
                    items,
                    current_index: oh_snapshot.current_index,
                };

                Ok(Some(snapshot))
            }
            MusicRenderer::Upnp(_)
            | MusicRenderer::Chromecast(_)
            | MusicRenderer::LinkPlay(_)
            | MusicRenderer::ArylicTcp(_)
            | MusicRenderer::HybridUpnpArylic { .. } => {
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
            MusicRenderer::OpenHome(_) => {
                // Get the current OpenHome playlist snapshot
                let snapshot = self.fetch_openhome_playlist_snapshot()?;

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
                        track_id,
                        "Calling openhome_playlist_play_id to start playback"
                    );
                    self.openhome_playlist_play_id(track_id)?;
                    info!(
                        renderer = self.id().0.as_str(),
                        track_id,
                        "Successfully called openhome_playlist_play_id"
                    );
                    Ok(())
                } else {
                    Err(anyhow!("No track to play in OpenHome playlist"))
                }
            }
            MusicRenderer::Upnp(_)
            | MusicRenderer::Chromecast(_)
            | MusicRenderer::LinkPlay(_)
            | MusicRenderer::ArylicTcp(_)
            | MusicRenderer::HybridUpnpArylic { .. } => Err(anyhow!(
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
            MusicRenderer::OpenHome(_) => {
                // Get the current OpenHome playlist snapshot
                let snapshot = self.fetch_openhome_playlist_snapshot()?;

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
                    Err(anyhow!("No track available to advance to in OpenHome playlist"))
                }
            }
            MusicRenderer::Upnp(_)
            | MusicRenderer::Chromecast(_)
            | MusicRenderer::LinkPlay(_)
            | MusicRenderer::ArylicTcp(_)
            | MusicRenderer::HybridUpnpArylic { .. } => Err(anyhow!(
                "play_next_from_backend_queue is not supported for {} backend (no persistent queue)",
                self.unsupported_backend_name()
            )),
        }
    }

    /// Convert an OpenHome playlist track to a PlaybackItem.
    fn playback_item_from_openhome_track(
        renderer_id: &RendererId,
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
            MusicRenderer::Upnp(_) => "UPnP",
            MusicRenderer::OpenHome(_) => "OpenHome",
            MusicRenderer::LinkPlay(_) => "LinkPlay",
            MusicRenderer::ArylicTcp(_) => "ArylicTcp",
            MusicRenderer::Chromecast(_) => "Chromecast",
            MusicRenderer::HybridUpnpArylic { .. } => "HybridUpnpArylic",
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
            MusicRenderer::OpenHome(renderer) => {
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
            MusicRenderer::OpenHome(renderer) => renderer.play_openhome_track_id(id),
            _ => Err(op_not_supported(
                "openhome_playlist_play_id",
                self.unsupported_backend_name(),
            )),
        }
    }

    fn playback_item_from_params(
        renderer_id: &RendererId,
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
impl TransportControl for MusicRenderer {
    fn play_uri(&self, uri: &str, meta: &str) -> Result<()> {
        match self {
            MusicRenderer::Upnp(upnp) => upnp.play_uri(uri, meta),
            MusicRenderer::OpenHome(oh) => oh.play_uri(uri, meta),
            MusicRenderer::LinkPlay(lp) => lp.play_uri(uri, meta),
            MusicRenderer::ArylicTcp(_) => Err(op_not_supported("play_uri", "ArylicTcp")),
            MusicRenderer::Chromecast(cc) => cc.play_uri(uri, meta),
            MusicRenderer::HybridUpnpArylic { upnp, .. } => upnp.play_uri(uri, meta),
        }
    }

    fn play(&self) -> Result<()> {
        match self {
            MusicRenderer::Upnp(upnp) => upnp.play(),
            MusicRenderer::OpenHome(oh) => oh.play(),
            MusicRenderer::LinkPlay(lp) => lp.play(),
            MusicRenderer::ArylicTcp(ary) => ary.play(),
            MusicRenderer::Chromecast(cc) => cc.play(),
            MusicRenderer::HybridUpnpArylic { arylic, .. } => arylic.play(),
        }
    }

    fn pause(&self) -> Result<()> {
        match self {
            MusicRenderer::Upnp(upnp) => upnp.pause(),
            MusicRenderer::OpenHome(oh) => oh.pause(),
            MusicRenderer::LinkPlay(lp) => lp.pause(),
            MusicRenderer::ArylicTcp(ary) => ary.pause(),
            MusicRenderer::Chromecast(cc) => cc.pause(),
            MusicRenderer::HybridUpnpArylic { arylic, .. } => arylic.pause(),
        }
    }

    fn stop(&self) -> Result<()> {
        match self {
            MusicRenderer::Upnp(upnp) => upnp.stop(),
            MusicRenderer::OpenHome(oh) => oh.stop(),
            MusicRenderer::LinkPlay(lp) => lp.stop(),
            MusicRenderer::ArylicTcp(ary) => ary.stop(),
            MusicRenderer::Chromecast(cc) => cc.stop(),
            MusicRenderer::HybridUpnpArylic { arylic, .. } => arylic.stop(),
        }
    }

    fn seek_rel_time(&self, hhmmss: &str) -> Result<()> {
        match self {
            MusicRenderer::Upnp(upnp) => upnp.seek_rel_time(hhmmss),
            MusicRenderer::OpenHome(oh) => oh.seek_rel_time(hhmmss),
            MusicRenderer::LinkPlay(lp) => lp.seek_rel_time(hhmmss),
            MusicRenderer::ArylicTcp(_) => Err(op_not_supported("seek_rel_time", "ArylicTcp")),
            MusicRenderer::Chromecast(cc) => cc.seek_rel_time(hhmmss),
            MusicRenderer::HybridUpnpArylic { upnp, .. } => upnp.seek_rel_time(hhmmss),
        }
    }
}

/// Volume and mute controls exposed via the façade.
///
/// Hybrid backends may read via Arylic TCP and write via UPnP, but callers
/// always depend on a single [`VolumeControl`] entry point.
impl VolumeControl for MusicRenderer {
    fn volume(&self) -> Result<u16> {
        match self {
            MusicRenderer::HybridUpnpArylic { arylic, .. } => arylic.volume(),
            MusicRenderer::ArylicTcp(ary) => ary.volume(),
            MusicRenderer::OpenHome(oh) => oh.volume(),
            MusicRenderer::Upnp(upnp) => upnp.volume(),
            MusicRenderer::LinkPlay(lp) => lp.volume(),
            MusicRenderer::Chromecast(cc) => cc.volume(),
        }
    }

    fn set_volume(&self, vol: u16) -> Result<()> {
        match self {
            MusicRenderer::HybridUpnpArylic { upnp, .. } => upnp.set_volume(vol),
            MusicRenderer::ArylicTcp(ary) => ary.set_volume(vol),
            MusicRenderer::OpenHome(oh) => oh.set_volume(vol),
            MusicRenderer::Upnp(upnp) => upnp.set_volume(vol),
            MusicRenderer::LinkPlay(lp) => lp.set_volume(vol),
            MusicRenderer::Chromecast(cc) => cc.set_volume(vol),
        }
    }

    fn mute(&self) -> Result<bool> {
        match self {
            MusicRenderer::HybridUpnpArylic { arylic, .. } => arylic.mute(),
            MusicRenderer::OpenHome(r) => r.mute(),
            MusicRenderer::Upnp(r) => r.get_master_mute(),
            MusicRenderer::LinkPlay(r) => r.mute(),
            MusicRenderer::ArylicTcp(r) => r.mute(),
            MusicRenderer::Chromecast(cc) => cc.mute(),
        }
    }

    fn set_mute(&self, m: bool) -> Result<()> {
        match self {
            MusicRenderer::HybridUpnpArylic { arylic, .. } => arylic.set_mute(m),
            MusicRenderer::OpenHome(r) => r.set_mute(m),
            MusicRenderer::Upnp(r) => r.set_master_mute(m),
            MusicRenderer::LinkPlay(r) => r.set_mute(m),
            MusicRenderer::ArylicTcp(r) => r.set_mute(m),
            MusicRenderer::Chromecast(cc) => cc.set_mute(m),
        }
    }
}

/// Playback-state queries sourced from the backend best suited for the job.
///
/// Each backend reports into [`PlaybackState`], ensuring consumers never have
/// to reason about protocol-specific state machines.
impl PlaybackStatus for MusicRenderer {
    fn playback_state(&self) -> Result<PlaybackState> {
        match self {
            MusicRenderer::Upnp(r) => PlaybackStatus::playback_state(r),
            MusicRenderer::OpenHome(r) => PlaybackStatus::playback_state(r),
            MusicRenderer::LinkPlay(r) => r.playback_state(),
            MusicRenderer::ArylicTcp(r) => r.playback_state(),
            MusicRenderer::Chromecast(cc) => cc.playback_state(),
            MusicRenderer::HybridUpnpArylic { arylic, .. } => arylic.playback_state(),
        }
    }
}

/// Playback-position queries that always yield a [`PlaybackPositionInfo`]
/// regardless of the backend providing the raw transport data.
impl PlaybackPosition for MusicRenderer {
    fn playback_position(&self) -> Result<PlaybackPositionInfo> {
        match self {
            MusicRenderer::Upnp(r) => r.playback_position(),
            MusicRenderer::OpenHome(r) => r.playback_position(),
            MusicRenderer::LinkPlay(r) => r.playback_position(),
            MusicRenderer::ArylicTcp(r) => r.playback_position(),
            MusicRenderer::Chromecast(cc) => cc.playback_position(),
            MusicRenderer::HybridUpnpArylic { arylic, .. } => arylic.playback_position(),
        }
    }
}
