//! Watcher module for MusicRenderer state surveillance.
//!
//! This module provides the infrastructure for each MusicRenderer to maintain
//! its own polling/watching thread, instead of relying on a centralized
//! polling loop in ControlPoint.
//!
//! ## Architecture
//!
//! Each MusicRenderer can have an associated watcher thread that:
//! - Polls the backend at regular intervals (or receives push notifications)
//! - Detects state changes by comparing with cached values
//! - Emits events when changes are detected
//! - Handles auto-advance logic when playback stops

use std::time::Duration;

use crate::model::{PlaybackState, TrackMetadata};
use crate::music_renderer::capabilities::PlaybackPositionInfo;
use crate::music_renderer::musicrenderer::MusicRendererBackend;

/// Strategy for monitoring renderer state changes.
///
/// Different backends may support different monitoring strategies:
/// - Pure polling for simple protocols (UPnP AVTransport, LinkPlay, Arylic)
/// - Push notifications for more advanced protocols (OpenHome subscriptions, Chromecast)
/// - Hybrid approaches combining both
#[derive(Clone, Debug)]
pub enum WatchStrategy {
    /// Poll the backend at regular intervals.
    /// Used for UPnP, LinkPlay, Arylic backends.
    Polling { interval_ms: u64 },

    /// Backend supports push notifications (future implementation).
    /// The watcher thread would wait on a channel instead of polling.
    Push,

    /// Hybrid: use push when available, fall back to polling.
    /// Used for OpenHome and Chromecast which support subscriptions
    /// but may need polling as a fallback.
    Hybrid { polling_interval_ms: u64 },
}

impl WatchStrategy {
    /// Returns the recommended strategy for a given backend type.
    pub fn for_backend(backend: &MusicRendererBackend) -> Self {
        match backend {
            MusicRendererBackend::Upnp(_) => WatchStrategy::Polling { interval_ms: 500 },
            MusicRendererBackend::LinkPlay(_) => WatchStrategy::Polling { interval_ms: 500 },
            MusicRendererBackend::ArylicTcp(_) => WatchStrategy::Polling { interval_ms: 500 },
            MusicRendererBackend::HybridUpnpArylic { .. } => {
                WatchStrategy::Polling { interval_ms: 500 }
            }
            // Future push support - for now use hybrid with polling fallback
            MusicRendererBackend::OpenHome(_) => WatchStrategy::Hybrid {
                polling_interval_ms: 500,
            },
            MusicRendererBackend::Chromecast(_) => WatchStrategy::Hybrid {
                polling_interval_ms: 500,
            },
        }
    }

    /// Returns the polling interval if this strategy involves polling.
    /// Returns None for pure Push strategy.
    pub fn polling_interval(&self) -> Option<Duration> {
        match self {
            WatchStrategy::Polling { interval_ms } => Some(Duration::from_millis(*interval_ms)),
            WatchStrategy::Hybrid {
                polling_interval_ms,
            } => Some(Duration::from_millis(*polling_interval_ms)),
            WatchStrategy::Push => None,
        }
    }
}

/// Cached state from last poll, used for change detection.
///
/// The watcher maintains this state to detect changes between polls
/// and only emit events when something actually changed.
#[derive(Clone, Default, Debug)]
pub struct WatchedState {
    /// Last known playback state (Playing, Paused, Stopped, etc.)
    pub state: Option<PlaybackState>,
    /// Last known playback position info
    pub position: Option<PlaybackPositionInfo>,
    /// Last known volume level (0-100)
    pub volume: Option<u16>,
    /// Last known mute state
    pub mute: Option<bool>,
    /// Last known track metadata
    pub metadata: Option<TrackMetadata>,
}

// ============================================================================
// Helper functions for change detection
// ============================================================================

/// Compares two PlaybackState values for equality.
///
/// Handles the Unknown variant specially by comparing the inner string.
pub fn playback_state_equal(a: &PlaybackState, b: &PlaybackState) -> bool {
    match (a, b) {
        (PlaybackState::Unknown(lhs), PlaybackState::Unknown(rhs)) => lhs == rhs,
        _ => std::mem::discriminant(a) == std::mem::discriminant(b),
    }
}

/// Compares two PlaybackPositionInfo values for equality.
pub fn playback_position_equal(a: &PlaybackPositionInfo, b: &PlaybackPositionInfo) -> bool {
    a.track == b.track
        && a.rel_time == b.rel_time
        && a.abs_time == b.abs_time
        && a.track_duration == b.track_duration
        && a.track_metadata == b.track_metadata
        && a.track_uri == b.track_uri
}

/// Compute a logical playback state by combining the raw AVTransport state
/// with previous and current position information.
///
/// This is designed to compensate for buggy LinkPlay/Arylic devices that
/// report:
///   - STOPPED while the time actually advances,
///   - NO_MEDIA_PRESENT while track duration is known.
pub fn compute_logical_playback_state(
    raw: &PlaybackState,
    prev_position: Option<&PlaybackPositionInfo>,
    current_position: Option<&PlaybackPositionInfo>,
) -> PlaybackState {
    // Rule 1: Arylic / LinkPlay sometimes report STOPPED while the stream is
    // actually playing. If we detect that the relative time advances between
    // two polls, we treat this as Playing.
    if let PlaybackState::Stopped = raw {
        if let (Some(prev), Some(curr)) = (prev_position, current_position) {
            if let (Some(prev_rel), Some(curr_rel)) = (
                parse_optional_hms_to_secs(&prev.rel_time),
                parse_optional_hms_to_secs(&curr.rel_time),
            ) {
                if curr_rel > prev_rel {
                    let delta = curr_rel - prev_rel;
                    // Our poll loop runs every 500ms; accept small jitter in the delta.
                    if delta <= 5 {
                        return PlaybackState::Playing;
                    }
                }
            }
        }
    }

    // Rule 2: Some devices report NO_MEDIA_PRESENT while exposing a non-zero
    // track duration. In practice this behaves like a stopped transport with
    // a loaded track.
    if let PlaybackState::NoMedia = raw {
        let duration_secs = current_position
            .and_then(|p| parse_optional_hms_to_secs(&p.track_duration))
            .or_else(|| prev_position.and_then(|p| parse_optional_hms_to_secs(&p.track_duration)));

        if matches!(duration_secs, Some(d) if d > 0) {
            return PlaybackState::Stopped;
        }
    }

    // Fallback: keep the raw (already normalized) state.
    raw.clone()
}

/// Parse an optional HH:MM:SS time string to seconds.
fn parse_optional_hms_to_secs(value: &Option<String>) -> Option<u64> {
    value.as_ref().and_then(|s| parse_hms_to_secs(s))
}

/// Parse "HH:MM:SS" style time strings to seconds.
///
/// Returns None for empty or sentinel values such as "NOT_IMPLEMENTED" or "-:--:--".
fn parse_hms_to_secs(s: &str) -> Option<u64> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }

    // Common sentinel values for "no information" in UPnP implementations.
    if s == "NOT_IMPLEMENTED" || s == "-:--:--" {
        return None;
    }

    let parts: Vec<_> = s.split(':').collect();
    if parts.len() != 3 {
        return None;
    }

    let hours: u64 = parts[0].parse().ok()?;
    let minutes: u64 = parts[1].parse().ok()?;
    let seconds: u64 = parts[2].parse().ok()?;

    Some(hours * 3600 + minutes * 60 + seconds)
}

/// Extract TrackMetadata from DIDL-Lite XML in PlaybackPositionInfo.
pub fn extract_track_metadata(position: &PlaybackPositionInfo) -> Option<TrackMetadata> {
    let didl_xml = position.track_metadata.as_ref()?;

    // Parse DIDL-Lite XML
    let didl = match pmodidl::parse_metadata::<pmodidl::DIDLLite>(didl_xml) {
        Ok(parsed) => parsed.data,
        Err(_) => return None,
    };

    // Extract first item metadata
    let item = didl.items.first()?;

    Some(TrackMetadata {
        title: Some(item.title.clone()),
        artist: item.artist.clone(),
        album: item.album.clone(),
        genre: item.genre.clone(),
        album_art_uri: item.album_art.clone(),
        date: item.date.clone(),
        track_number: item.original_track_number.clone(),
        creator: item.creator.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_hms_to_secs() {
        assert_eq!(parse_hms_to_secs("00:00:00"), Some(0));
        assert_eq!(parse_hms_to_secs("00:01:30"), Some(90));
        assert_eq!(parse_hms_to_secs("01:00:00"), Some(3600));
        assert_eq!(parse_hms_to_secs("01:30:45"), Some(5445));
        assert_eq!(parse_hms_to_secs("NOT_IMPLEMENTED"), None);
        assert_eq!(parse_hms_to_secs("-:--:--"), None);
        assert_eq!(parse_hms_to_secs(""), None);
        assert_eq!(parse_hms_to_secs("invalid"), None);
    }

    #[test]
    fn test_playback_state_equal() {
        assert!(playback_state_equal(
            &PlaybackState::Playing,
            &PlaybackState::Playing
        ));
        assert!(playback_state_equal(
            &PlaybackState::Stopped,
            &PlaybackState::Stopped
        ));
        assert!(!playback_state_equal(
            &PlaybackState::Playing,
            &PlaybackState::Stopped
        ));
        assert!(playback_state_equal(
            &PlaybackState::Unknown("foo".to_string()),
            &PlaybackState::Unknown("foo".to_string())
        ));
        assert!(!playback_state_equal(
            &PlaybackState::Unknown("foo".to_string()),
            &PlaybackState::Unknown("bar".to_string())
        ));
    }

    #[test]
    fn test_watch_strategy_polling_interval() {
        let polling = WatchStrategy::Polling { interval_ms: 500 };
        assert_eq!(polling.polling_interval(), Some(Duration::from_millis(500)));

        let hybrid = WatchStrategy::Hybrid {
            polling_interval_ms: 1000,
        };
        assert_eq!(hybrid.polling_interval(), Some(Duration::from_millis(1000)));

        let push = WatchStrategy::Push;
        assert_eq!(push.polling_interval(), None);
    }
}
