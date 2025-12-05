//! Backend-agnostic music renderer façade for PMOMusic.
//!
//! `MusicRenderer` wraps every supported backend (UPnP AV/DLNA, OpenHome,
//! LinkPlay HTTP, Arylic TCP, and the hybrid UPnP + Arylic pairing) behind a
//! single control surface. Higher layers in PMOMusic must only interact with
//! renderers through this type so that transport, volume, and state queries
//! stay backend-neutral.

use std::sync::{Arc, RwLock};

use crate::capabilities::{PlaybackPositionInfo, PlaybackStatus};
use crate::model::{RendererId, RendererInfo, RendererProtocol};
use crate::openhome_playlist::OpenHomePlaylistSnapshot;
use crate::{
    ArylicTcpRenderer, DeviceRegistry, LinkPlayRenderer, OpenHomeRenderer, PlaybackPosition,
    PlaybackState, TransportControl, UpnpRenderer, VolumeControl,
};
use anyhow::{Result, anyhow};
use tracing::warn;

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

impl MusicRenderer {
    /// Renderer identifier (stable within the registry).
    pub fn id(&self) -> &RendererId {
        match self {
            MusicRenderer::HybridUpnpArylic { arylic, .. } => arylic.id(),
            MusicRenderer::OpenHome(r) => r.id(),
            MusicRenderer::Upnp(r) => r.id(),
            MusicRenderer::LinkPlay(r) => r.id(),
            MusicRenderer::ArylicTcp(r) => r.id(),
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
        }
    }

    pub fn openhome_playlist_snapshot(&self) -> Result<OpenHomePlaylistSnapshot> {
        match self {
            MusicRenderer::OpenHome(renderer) => renderer.snapshot_openhome_playlist(),
            _ => Err(op_not_supported(
                "openhome_playlist_snapshot",
                self.unsupported_backend_name(),
            )),
        }
    }

    pub fn openhome_playlist_clear(&self) -> Result<()> {
        match self {
            MusicRenderer::OpenHome(renderer) => renderer.clear_openhome_playlist(),
            _ => Err(op_not_supported(
                "openhome_playlist_clear",
                self.unsupported_backend_name(),
            )),
        }
    }

    pub fn openhome_playlist_add_track(
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

    pub fn openhome_playlist_play_id(&self, id: u32) -> Result<()> {
        match self {
            MusicRenderer::OpenHome(renderer) => renderer.play_openhome_track_id(id),
            _ => Err(op_not_supported(
                "openhome_playlist_play_id",
                self.unsupported_backend_name(),
            )),
        }
    }

    fn unsupported_backend_name(&self) -> &'static str {
        match self {
            MusicRenderer::Upnp(_) => "UPnP",
            MusicRenderer::OpenHome(_) => "OpenHome",
            MusicRenderer::LinkPlay(_) => "LinkPlay",
            MusicRenderer::ArylicTcp(_) => "ArylicTcp",
            MusicRenderer::HybridUpnpArylic { .. } => "HybridUpnpArylic",
        }
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
            MusicRenderer::HybridUpnpArylic { upnp, .. } => upnp.play_uri(uri, meta),
        }
    }

    fn play(&self) -> Result<()> {
        match self {
            MusicRenderer::Upnp(upnp) => upnp.play(),
            MusicRenderer::OpenHome(oh) => oh.play(),
            MusicRenderer::LinkPlay(lp) => lp.play(),
            MusicRenderer::ArylicTcp(ary) => ary.play(),
            MusicRenderer::HybridUpnpArylic { arylic, .. } => arylic.play(),
        }
    }

    fn pause(&self) -> Result<()> {
        match self {
            MusicRenderer::Upnp(upnp) => upnp.pause(),
            MusicRenderer::OpenHome(oh) => oh.pause(),
            MusicRenderer::LinkPlay(lp) => lp.pause(),
            MusicRenderer::ArylicTcp(ary) => ary.pause(),
            MusicRenderer::HybridUpnpArylic { arylic, .. } => arylic.pause(),
        }
    }

    fn stop(&self) -> Result<()> {
        match self {
            MusicRenderer::Upnp(upnp) => upnp.stop(),
            MusicRenderer::OpenHome(oh) => oh.stop(),
            MusicRenderer::LinkPlay(lp) => lp.stop(),
            MusicRenderer::ArylicTcp(ary) => ary.stop(),
            MusicRenderer::HybridUpnpArylic { arylic, .. } => arylic.stop(),
        }
    }

    fn seek_rel_time(&self, hhmmss: &str) -> Result<()> {
        match self {
            MusicRenderer::Upnp(upnp) => upnp.seek_rel_time(hhmmss),
            MusicRenderer::OpenHome(oh) => oh.seek_rel_time(hhmmss),
            MusicRenderer::LinkPlay(lp) => lp.seek_rel_time(hhmmss),
            MusicRenderer::ArylicTcp(_) => Err(op_not_supported("seek_rel_time", "ArylicTcp")),
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
        }
    }

    fn set_volume(&self, vol: u16) -> Result<()> {
        match self {
            MusicRenderer::HybridUpnpArylic { upnp, .. } => upnp.set_volume(vol),
            MusicRenderer::ArylicTcp(ary) => ary.set_volume(vol),
            MusicRenderer::OpenHome(oh) => oh.set_volume(vol),
            MusicRenderer::Upnp(upnp) => upnp.set_volume(vol),
            MusicRenderer::LinkPlay(lp) => lp.set_volume(vol),
        }
    }

    fn mute(&self) -> Result<bool> {
        match self {
            MusicRenderer::HybridUpnpArylic { arylic, .. } => arylic.mute(),
            MusicRenderer::OpenHome(r) => r.mute(),
            MusicRenderer::Upnp(r) => r.get_master_mute(),
            MusicRenderer::LinkPlay(r) => r.mute(),
            MusicRenderer::ArylicTcp(r) => r.mute(),
        }
    }

    fn set_mute(&self, m: bool) -> Result<()> {
        match self {
            MusicRenderer::HybridUpnpArylic { arylic, .. } => arylic.set_mute(m),
            MusicRenderer::OpenHome(r) => r.set_mute(m),
            MusicRenderer::Upnp(r) => r.set_master_mute(m),
            MusicRenderer::LinkPlay(r) => r.set_mute(m),
            MusicRenderer::ArylicTcp(r) => r.set_mute(m),
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
            MusicRenderer::HybridUpnpArylic { arylic, .. } => arylic.playback_position(),
        }
    }
}
