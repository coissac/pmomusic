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
use crate::music_renderer::time_utils::parse_time_flexible;

/// Returns true if `new_dur` < `old_dur` (both parseable as HH:MM:SS/MM:SS/SS).
/// Used to protect stream durations from decreasing for the same track.
pub(super) fn stream_duration_decreased(old_dur: &str, new_dur: &str) -> bool {
    match (parse_time_flexible(old_dur).ok(), parse_time_flexible(new_dur).ok()) {
        (Some(old_secs), Some(new_secs)) => new_secs < old_secs,
        _ => false,
    }
}

/// Returns true if `new_dur` > `old_dur` (both parseable as HH:MM:SS/MM:SS/SS).
pub(super) fn stream_duration_increased(old_dur: &str, new_dur: &str) -> bool {
    match (parse_time_flexible(old_dur).ok(), parse_time_flexible(new_dur).ok()) {
        (Some(old_secs), Some(new_secs)) => new_secs > old_secs,
        _ => false,
    }
}

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
