mod backend;
mod interne;
mod music_queue;
mod openhome;
mod snapshot;

use std::sync::{Arc, Mutex};

pub use backend::{EnqueueMode, QueueBackend};
pub use music_queue::MusicQueue;
pub use snapshot::{PlaybackItem, QueueSnapshot};

// Internal queue implementations - not part of the public API
pub(crate) use interne::InternalQueue;
pub(crate) use openhome::OpenHomeQueue;

use crate::{RendererInfo, errors::ControlPointError};

pub trait QueueFromRendererInfo {
    fn from_renderer_info(renderer: &RendererInfo) -> Result<Self, ControlPointError>
    where
        Self: Sized;

    fn to_backend(self) -> MusicQueue;

    fn build_from_renderer_info(renderer: &RendererInfo) -> Result<MusicQueue, ControlPointError>
    where
        Self: Sized,
    {
        let instance = Self::from_renderer_info(renderer)?;
        Ok(instance.to_backend())
    }

    fn make_from_renderer_info(
        renderer: &RendererInfo,
    ) -> Result<Arc<Mutex<MusicQueue>>, ControlPointError>
    where
        Self: Sized,
    {
        let backend = Self::build_from_renderer_info(renderer)?;
        Ok(Arc::new(Mutex::new(backend)))
    }
}
