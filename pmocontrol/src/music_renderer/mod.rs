mod arylic_tcp;
mod linkplay_renderer;

mod upnp_renderer;

mod openhome;
mod openhome_renderer;

mod capabilities;
mod chromecast_renderer;

mod musicrenderer;
mod sleep_timer;
pub mod time_utils;
pub mod watcher;

use std::sync::{Arc, Mutex};

pub use crate::music_renderer::capabilities::{
    PlaybackPosition, PlaybackPositionInfo, PlaybackStatus,
};
pub use crate::music_renderer::musicrenderer::{MusicRenderer, PlaylistBinding};
pub use crate::music_renderer::sleep_timer::SleepTimer;
use crate::{
    RendererInfo, errors::ControlPointError, music_renderer::musicrenderer::MusicRendererBackend,
};

pub trait RendererFromMediaRendererInfo {
    fn from_renderer_info(renderer: &RendererInfo) -> Result<Self, ControlPointError>
    where
        Self: Sized;

    fn to_backend(self) -> MusicRendererBackend
    where
        Self: Sized;

    fn build_from_renderer_info(
        renderer: &RendererInfo,
    ) -> Result<MusicRendererBackend, ControlPointError>
    where
        Self: Sized,
    {
        let instance = Self::from_renderer_info(renderer)?;
        Ok(instance.to_backend())
    }

    fn make_from_renderer_info(
        renderer: &RendererInfo,
    ) -> Result<Arc<Mutex<MusicRendererBackend>>, ControlPointError>
    where
        Self: Sized,
    {
        let backend = Self::build_from_renderer_info(renderer)?;
        Ok(Arc::new(Mutex::new(backend)))
    }
}
