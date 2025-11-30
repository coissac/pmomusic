// pmocontrol/src/music_renderer.rs

use crate::capabilities::{PlaybackPositionInfo, PlaybackStatus};
use crate::model::{RendererId, RendererInfo, RendererProtocol};
use crate::{DeviceRegistry, PlaybackPosition, PlaybackState, PositionInfo, TransportControl, UpnpRenderer, VolumeControl};
use anyhow::Result;

/// Music view of a renderer, independent of the underlying protocol/backend.
///
/// For now, only UPnP AV renderers are supported via [`UpnpRenderer`],
/// but this type is designed to host additional backends (e.g. OpenHome)
/// later on.
#[derive(Clone, Debug)]
pub enum MusicRenderer {
    /// UPnP AV / DLNA backend.
    Upnp(UpnpRenderer),
    // Future backends could be added here, e.g.:
    // OpenHome(OpenHomeRenderer),
}

impl MusicRenderer {
    /// Renderer identifier (stable within the registry).
    pub fn id(&self) -> &RendererId {
        match self {
            MusicRenderer::Upnp(r) => r.id(),
        }
    }

    /// Human-friendly name reported by the device.
    pub fn friendly_name(&self) -> &str {
        match self {
            MusicRenderer::Upnp(r) => r.friendly_name(),
        }
    }

    /// Protocol classification (UPnP AV only, OpenHome only, hybrid).
    pub fn protocol(&self) -> &RendererProtocol {
        match self {
            MusicRenderer::Upnp(r) => &r.info.protocol,
        }
    }

    /// Full static info as stored in the registry.
    pub fn info(&self) -> &RendererInfo {
        match self {
            MusicRenderer::Upnp(r) => &r.info,
        }
    }

    /// Return a reference to the underlying UPnP backend, if any.
    pub fn as_upnp(&self) -> Option<&UpnpRenderer> {
        match self {
            MusicRenderer::Upnp(r) => Some(r),
        }
    }

    /// Construct a music renderer from a [`RendererInfo`] and the registry.
    ///
    /// Returns `None` when no supported backend can be built for this renderer.
    /// Currently, only UPnP AV / hybrid renderers are mapped to [`MusicRenderer::Upnp`].
    pub fn from_registry_info(
        info: RendererInfo,
        registry: &DeviceRegistry,
    ) -> Option<MusicRenderer> {
        match info.protocol {
            RendererProtocol::UpnpAvOnly | RendererProtocol::Hybrid => {
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
/// Pour l'instant, seule la variante [`MusicRenderer::Upnp`] est supportée,
/// et la délégation se fait vers l'implémentation UPnP AV.
impl TransportControl for MusicRenderer {
    fn play_uri(&self, uri: &str, meta: &str) -> Result<()> {
        match self {
            MusicRenderer::Upnp(r) => r.play_uri(uri, meta),
        }
    }

    fn play(&self) -> Result<()> {
        match self {
            MusicRenderer::Upnp(r) => TransportControl::play(r),
        }
    }

    fn pause(&self) -> Result<()> {
        match self {
            MusicRenderer::Upnp(r) => r.pause(),
        }
    }

    fn stop(&self) -> Result<()> {
        match self {
            MusicRenderer::Upnp(r) => r.stop(),
        }
    }

    fn seek_rel_time(&self, hhmmss: &str) -> Result<()> {
        match self {
            MusicRenderer::Upnp(r) => r.seek_rel_time(hhmmss),
        }
    }
}

/// Implémentation générique de `VolumeControl` pour [`MusicRenderer`].
///
/// Pour l'instant, seule la variante [`MusicRenderer::Upnp`] est supportée,
/// et la délégation se fait vers l'implémentation UPnP RenderingControl.
impl VolumeControl for MusicRenderer {
    fn volume(&self) -> Result<u16> {
        match self {
            MusicRenderer::Upnp(r) => r.get_master_volume(),
        }
    }

    fn set_volume(&self, v: u16) -> Result<()> {
        match self {
            MusicRenderer::Upnp(r) => r.set_master_volume(v),
        }
    }

    fn mute(&self) -> Result<bool> {
        match self {
            MusicRenderer::Upnp(r) => r.get_master_mute(),
        }
    }

    fn set_mute(&self, m: bool) -> Result<()> {
        match self {
            MusicRenderer::Upnp(r) => r.set_master_mute(m),
        }
    }
}

/// Implémentation générique de `PlaybackStatus` pour [`MusicRenderer`].
///
/// La variante UPnP délègue à [`UpnpRenderer`]. Les backends OpenHome
/// futurs mapperont l'état OH vers `PlaybackState`.
impl PlaybackStatus for MusicRenderer {
    fn playback_state(&self) -> Result<PlaybackState> {
        match self {
            MusicRenderer::Upnp(r) => PlaybackStatus::playback_state(r),
        }
    }
}

impl PlaybackPosition for MusicRenderer {
    fn playback_position(&self) -> Result<PlaybackPositionInfo> {
        match self {
            MusicRenderer::Upnp(r) => r.playback_position(),
            // MusicRenderer::OpenHome(r) => r.playback_position(), plus tard
        }
    }
}