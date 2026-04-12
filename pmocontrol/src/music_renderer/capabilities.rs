// pmocontrol/src/capabilities.rs
use std::sync::{Arc, Mutex};

use crate::queue::{MusicQueue, QueueBackend};
use crate::{errors::ControlPointError, model::PlaybackState, PlaybackItem};

/// Marker trait for renderer backends that own a `MusicQueue`.
///
/// Implementing this trait automatically provides the full `QueueBackend`
/// blanket implementation (see `queue/backend.rs`).  Backends only need to
/// return a reference to their `Arc<Mutex<MusicQueue>>` field.
pub trait HasQueue {
    fn queue(&self) -> &Arc<Mutex<MusicQueue>>;
}

/// Marker trait for renderer backends that track stream continuity.
///
/// The flag is `true` while the renderer is playing a continuous stream
/// (e.g. an internet radio station) and `false` for bounded media files.
/// It is used by the watcher to decide whether auto-advance should be
/// suppressed when playback stops.
pub trait HasContinuousStream {
    fn continuous_stream(&self) -> &Arc<Mutex<bool>>;
}

/// Queue-aware transport control operations.
///
/// These operations combine queue management with transport control,
/// allowing navigation (next/previous) and track selection from the queue.
#[allow(dead_code)]
pub trait QueueTransportControl: HasQueue + HasContinuousStream {
    /// Play a specific item from the queue (backend-specific implementation).
    fn play_item(&self, item: &PlaybackItem) -> Result<(), ControlPointError>;

    /// Play from the queue at the current index (or initialize to 0 if not set).
    /// This is the default implementation that handles queue navigation.
    fn play_from_queue(&self) -> Result<(), ControlPointError> {
        let mut queue = self.queue().lock().expect("queue mutex poisoned");

        let current_index = match queue.current_index()? {
            Some(idx) => idx,
            None => {
                if queue.len()? > 0 {
                    queue.set_index(Some(0))?;
                    0
                } else {
                    return Err(ControlPointError::QueueError("Queue is empty".into()));
                }
            }
        };

        let item = queue
            .get_item(current_index)?
            .ok_or_else(|| ControlPointError::QueueError("Current item not found".into()))?;

        drop(queue);

        let is_stream = crate::music_renderer::is_continuous_stream(item.metadata.as_ref(), &item.uri);
        *self.continuous_stream().lock().expect("continuous_stream mutex poisoned") = is_stream;

        self.play_item(&item)
    }

    /// Play the next track from the queue.
    fn play_next(&self) -> Result<(), ControlPointError> {
        {
            let mut queue = self.queue().lock().expect("queue mutex poisoned");
            if !queue.advance()? {
                return Err(ControlPointError::QueueError("No next track".into()));
            }
        }
        self.play_from_queue()
    }

    /// Play the previous track from the queue.
    fn play_previous(&self) -> Result<(), ControlPointError> {
        {
            let mut queue = self.queue().lock().expect("queue mutex poisoned");
            if !queue.rewind()? {
                return Err(ControlPointError::QueueError("No previous track".into()));
            }
        }
        self.play_from_queue()
    }

    /// Play from a specific index in the queue.
    fn play_from_index(&self, index: usize) -> Result<(), ControlPointError> {
        {
            let mut queue = self.queue().lock().expect("queue mutex poisoned");
            queue.set_index(Some(index))?;
        }
        self.play_from_queue()
    }
}

/// Logical playback position across backends.
///
/// Times peuvent être soit en secondes, soit en "HH:MM:SS" selon ce que
/// tu préfères pour la façade; ici je reste en String pour garder la
/// même granularité que UPnP sans parser.
#[derive(Clone, Debug)]
pub struct PlaybackPositionInfo {
    pub track: Option<u32>,
    pub rel_time: Option<String>,       // position courante
    pub abs_time: Option<String>,       // si pertinent
    pub track_duration: Option<String>, // durée totale
    pub track_metadata: Option<String>, // DIDL-Lite XML from GetPositionInfo
    pub track_uri: Option<String>,      // Current track URI
}
/// Provides the current playback position and track metadata.
///
/// All time fields use the format `"HH:MM:SS"` (or `None` when unavailable).
/// `track_metadata` carries a raw DIDL-Lite XML fragment returned by the device;
/// callers that only need structured metadata should use `extract_track_metadata`
/// from the watcher module instead.
pub trait PlaybackPosition {
    /// Returns the current playback position information.
    ///
    /// Returns `Err` if the renderer is unreachable or the query fails.
    fn playback_position(&self) -> Result<PlaybackPositionInfo, ControlPointError>;
}

/// Generic abstraction for the current transport state.
///
/// # Implementations
///
/// - **UPnP AV**: backed by `AVTransport::GetTransportInfo`.
/// - **OpenHome**: adapted from OH `Info` / `Time` services.
/// - **LinkPlay / Arylic**: mapped from the vendor status response.
///
/// # Postconditions
///
/// The returned `PlaybackState` must be one of the canonical values defined by
/// the `PlaybackState` enum.  Backend-specific states that have no canonical
/// equivalent should be mapped to the closest approximation (e.g. "BUFFERING"
/// → `PlaybackState::Transitioning`).
pub trait PlaybackStatus {
    /// Returns the current transport state of the renderer.
    fn playback_state(&self) -> Result<PlaybackState, ControlPointError>;
}

/// Generic transport control abstraction (play / pause / stop / seek),
/// independent of the underlying protocol (UPnP AV, OpenHome, …).
///
/// # Invariants
///
/// - `play_uri` sets the active resource and begins playback atomically from the
///   caller's perspective.  Implementations may split this into two protocol steps
///   (e.g. `SetAVTransportURI` + `Play` for UPnP AV) but the caller should not
///   need to know.
/// - `play` / `pause` / `stop` operate on whatever resource is currently loaded;
///   they do not change the queue pointer.
/// - `seek_rel_time` uses the format `"HH:MM:SS"`.  Backends that do not support
///   seeking should return `ControlPointError::NotSupported`.
///
/// # Relation to `QueueTransportControl`
///
/// `TransportControl` knows nothing about the queue.  `QueueTransportControl`
/// extends it with queue-aware navigation (`play_next`, `play_previous`, …).
pub trait TransportControl {
    /// Load a resource (URI + DIDL-Lite metadata) and begin playback.
    ///
    /// Depending on the backend this may execute as a single atomic operation or as
    /// two sequential commands (set resource, then play).
    fn play_uri(&self, uri: &str, meta: &str) -> Result<(), ControlPointError>;

    /// Start or resume playback of the currently loaded resource.
    fn play(&self) -> Result<(), ControlPointError>;

    /// Pause the current playback.
    fn pause(&self) -> Result<(), ControlPointError>;

    /// Stop the current playback and release the loaded resource.
    fn stop(&self) -> Result<(), ControlPointError>;

    /// Seek to a relative time position expressed as `"HH:MM:SS"`.
    ///
    /// Returns `ControlPointError::NotSupported` when the backend does not
    /// implement seeking.
    fn seek_rel_time(&self, hhmmss: &str) -> Result<(), ControlPointError>;
}

/// Generic volume and mute control abstraction.
///
/// # Volume scale
///
/// Volume values are expressed on the native scale of each renderer.
/// UPnP AV and OpenHome renderers typically use 0–100.  Callers should
/// not assume any particular scale; use the values returned by `volume()`
/// as the baseline for relative adjustments.
pub trait VolumeControl {
    /// Returns the current logical volume (renderer-specific scale).
    fn volume(&self) -> Result<u16, ControlPointError>;

    /// Sets the logical volume (renderer-specific scale).
    fn set_volume(&self, v: u16) -> Result<(), ControlPointError>;

    /// Returns `true` when the renderer is muted.
    fn mute(&self) -> Result<bool, ControlPointError>;

    /// Enables (`true`) or disables (`false`) mute.
    fn set_mute(&self, m: bool) -> Result<(), ControlPointError>;
}
