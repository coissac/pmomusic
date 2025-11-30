// pmocontrol/src/music_renderer.rs

use crate::capabilities::{PlaybackPositionInfo, PlaybackStatus};
use crate::model::{RendererId, RendererInfo, RendererProtocol};
use crate::{
    DeviceRegistry, LinkPlayRenderer, PlaybackPosition, PlaybackState, TransportControl,
    UpnpRenderer, VolumeControl,
};
use anyhow::Result;
use tracing::warn;

/// Music view of a renderer, independent of the underlying protocol/backend.
///
/// Currently supported backends:
/// - [`UpnpRenderer`] (AVTransport + RenderingControl)
/// - [`LinkPlayRenderer`] (LinkPlay HTTP API)
/// Additional backends (e.g. OpenHome) can be integrated later.
#[derive(Clone, Debug)]
pub enum MusicRenderer {
    /// UPnP AV / DLNA backend.
    Upnp(UpnpRenderer),
    /// LinkPlay HTTP backend (Arylic and similar devices).
    LinkPlay(LinkPlayRenderer),
}

impl MusicRenderer {
    /// Renderer identifier (stable within the registry).
    pub fn id(&self) -> &RendererId {
        match self {
            MusicRenderer::Upnp(r) => r.id(),
            MusicRenderer::LinkPlay(r) => r.id(),
        }
    }

    /// Human-friendly name reported by the device.
    pub fn friendly_name(&self) -> &str {
        match self {
            MusicRenderer::Upnp(r) => r.friendly_name(),
            MusicRenderer::LinkPlay(r) => r.friendly_name(),
        }
    }

    /// Protocol classification (UPnP AV only, OpenHome only, hybrid).
    pub fn protocol(&self) -> &RendererProtocol {
        &self.info().protocol
    }

    /// Full static info as stored in the registry.
    pub fn info(&self) -> &RendererInfo {
        match self {
            MusicRenderer::Upnp(r) => &r.info,
            MusicRenderer::LinkPlay(r) => &r.info,
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
        registry: &DeviceRegistry,
    ) -> Option<MusicRenderer> {
        match info.protocol {
            RendererProtocol::UpnpAvOnly | RendererProtocol::Hybrid => {
                if info.capabilities.has_linkplay_http {
                    match LinkPlayRenderer::from_renderer_info(info.clone()) {
                        Ok(renderer) => return Some(MusicRenderer::LinkPlay(renderer)),
                        Err(err) => warn!(
                            "Failed to build LinkPlay backend for {}: {}. Falling back to UPnP.",
                            info.friendly_name, err
                        ),
                    }
                }
                Some(MusicRenderer::Upnp(UpnpRenderer::from_registry(info, registry)))
            }
            RendererProtocol::OpenHomeOnly => {
                // OpenHome-only backend not implemented yet.
                None
            }
        }
    }
}

/// Implémentation générique de `TransportControl` pour [`MusicRenderer`].
///
/// Les variantes UPnP et LinkPlay délèguent aux backends correspondants.
impl TransportControl for MusicRenderer {
    fn play_uri(&self, uri: &str, meta: &str) -> Result<()> {
        match self {
            MusicRenderer::Upnp(r) => r.play_uri(uri, meta),
            MusicRenderer::LinkPlay(r) => r.play_uri(uri, meta),
        }
    }

    fn play(&self) -> Result<()> {
        match self {
            MusicRenderer::Upnp(r) => TransportControl::play(r),
            MusicRenderer::LinkPlay(r) => r.play(),
        }
    }

    fn pause(&self) -> Result<()> {
        match self {
            MusicRenderer::Upnp(r) => r.pause(),
            MusicRenderer::LinkPlay(r) => r.pause(),
        }
    }

    fn stop(&self) -> Result<()> {
        match self {
            MusicRenderer::Upnp(r) => r.stop(),
            MusicRenderer::LinkPlay(r) => r.stop(),
        }
    }

    fn seek_rel_time(&self, hhmmss: &str) -> Result<()> {
        match self {
            MusicRenderer::Upnp(r) => r.seek_rel_time(hhmmss),
            MusicRenderer::LinkPlay(r) => r.seek_rel_time(hhmmss),
        }
    }
}

/// Implémentation générique de `VolumeControl` pour [`MusicRenderer`].
///
/// Les variantes UPnP et LinkPlay délèguent aux backends correspondants.
impl VolumeControl for MusicRenderer {
    fn volume(&self) -> Result<u16> {
        match self {
            MusicRenderer::Upnp(r) => r.get_master_volume(),
            MusicRenderer::LinkPlay(r) => r.volume(),
        }
    }

    fn set_volume(&self, v: u16) -> Result<()> {
        match self {
            MusicRenderer::Upnp(r) => r.set_master_volume(v),
            MusicRenderer::LinkPlay(r) => r.set_volume(v),
        }
    }

    fn mute(&self) -> Result<bool> {
        match self {
            MusicRenderer::Upnp(r) => r.get_master_mute(),
            MusicRenderer::LinkPlay(r) => r.mute(),
        }
    }

    fn set_mute(&self, m: bool) -> Result<()> {
        match self {
            MusicRenderer::Upnp(r) => r.set_master_mute(m),
            MusicRenderer::LinkPlay(r) => r.set_mute(m),
        }
    }
}

/// Implémentation générique de `PlaybackStatus` pour [`MusicRenderer`].
///
/// Chaque backend fournit sa propre source d'état (UPnP AVTransport ou LinkPlay HTTP).
impl PlaybackStatus for MusicRenderer {
    fn playback_state(&self) -> Result<PlaybackState> {
        match self {
            MusicRenderer::Upnp(r) => PlaybackStatus::playback_state(r),
            MusicRenderer::LinkPlay(r) => r.playback_state(),
        }
    }
}

impl PlaybackPosition for MusicRenderer {
    fn playback_position(&self) -> Result<PlaybackPositionInfo> {
        match self {
            MusicRenderer::Upnp(r) => r.playback_position(),
            MusicRenderer::LinkPlay(r) => r.playback_position(),
        }
    }
}
