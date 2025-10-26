//! Constants for Radio Paradise orchestration layer.
//!
//! This module defines all the hardcoded parameters for the Radio Paradise
//! integration. These values are based on empirical testing and Radio Paradise's
//! infrastructure characteristics.

use std::time::Duration;

// ============================================================================
// Activity Lifecycle
// ============================================================================

/// Cooling timeout after all clients disconnect (seconds)
///
/// After the last client disconnects, the channel enters a "cooling" state
/// where it remains active for this duration before shutting down completely.
/// This avoids rapid start/stop cycles if clients reconnect quickly.
///
/// Value: 180 seconds (3 minutes) - good balance between responsiveness and stability
pub const COOLING_TIMEOUT_SECONDS: u64 = 180;

// ============================================================================
// Polling Intervals
// ============================================================================

/// High buffer polling interval (seconds)
///
/// When the playlist buffer has 3+ blocks, poll less frequently to reduce
/// API load and network usage.
///
/// Value: 120 seconds (2 minutes)
pub const POLLING_INTERVAL_HIGH_BUFFER: u64 = 120;

/// Medium buffer polling interval (seconds)
///
/// When the playlist buffer has 2 blocks, poll at moderate frequency.
///
/// Value: 60 seconds (1 minute)
pub const POLLING_INTERVAL_MEDIUM_BUFFER: u64 = 60;

/// Low buffer polling interval (seconds)
///
/// When the playlist buffer has less than 2 blocks, poll frequently to
/// ensure continuous playback.
///
/// Value: 20 seconds
pub const POLLING_INTERVAL_LOW_BUFFER: u64 = 20;

/// Helper to get high buffer polling interval as Duration
pub fn polling_high_interval() -> Duration {
    Duration::from_secs(POLLING_INTERVAL_HIGH_BUFFER)
}

/// Helper to get medium buffer polling interval as Duration
pub fn polling_medium_interval() -> Duration {
    Duration::from_secs(POLLING_INTERVAL_MEDIUM_BUFFER)
}

/// Helper to get low buffer polling interval as Duration
pub fn polling_low_interval() -> Duration {
    Duration::from_secs(POLLING_INTERVAL_LOW_BUFFER)
}

// ============================================================================
// Polling Backoff (on API errors)
// ============================================================================

/// Initial backoff delay on API error (seconds)
///
/// When an API request fails, we wait this duration before retrying.
///
/// Value: 20 seconds
pub const BACKOFF_INITIAL_SECONDS: u64 = 20;

/// Maximum backoff delay (seconds)
///
/// Backoff is capped at this value to avoid waiting too long.
///
/// Value: 300 seconds (5 minutes)
pub const BACKOFF_MAX_SECONDS: u64 = 300;

/// Backoff multiplier
///
/// After each failure, the delay is multiplied by this factor.
/// Example: 20s → 40s → 80s → 160s → 300s (capped)
///
/// Value: 2.0 (exponential backoff)
pub const BACKOFF_MULTIPLIER: f32 = 2.0;

// ============================================================================
// Cache Tuning
// ============================================================================

/// Maximum number of blocks to remember in the worker
///
/// This prevents unbounded memory growth by limiting how many block event IDs
/// we track to avoid re-processing.
///
/// Calculation: (4 channels + 1 buffer) × 3 blocks per channel = 15 blocks
/// Each block is ~20 minutes of audio, so 15 blocks ≈ 5 hours of history
///
/// Value: 15 blocks
pub const MAX_BLOCKS_REMEMBERED: usize = 15;

/// Number of bytes to use for track ID hashing
///
/// Track IDs are constructed by hashing block content and track position.
/// This value defines how much of the FLAC data we read for hashing.
///
/// Value: 512 bytes - sufficient for unique identification without excessive I/O
pub const TRACK_ID_HASH_BYTES: usize = 512;

// ============================================================================
// History
// ============================================================================

/// Default maximum number of tracks to keep in history
///
/// This is used as the default if not configured via pmoconfig.
/// Users can override this value in their configuration.
///
/// Value: 100 tracks - represents ~5-8 hours of playback history
pub const HISTORY_DEFAULT_MAX_TRACKS: usize = 100;

// ============================================================================
// Streaming
// ============================================================================

/// Stream buffer size (bytes)
///
/// Buffer size for audio streaming. 64KB provides good balance between
/// latency and buffering efficiency.
///
/// Value: 64 KB
pub const STREAM_BUFFER_SIZE_BYTES: usize = 64 * 1024;

/// Enable gapless playback
///
/// Radio Paradise blocks are designed for gapless playback - each block
/// transitions seamlessly to the next without audio gaps.
///
/// Value: true (always enabled)
pub const STREAM_GAPLESS: bool = true;

// Note: Metadata format is always ICY (Icecast/SHOUTcast metadata)
// No enum or constant needed as it's the only supported format

// ============================================================================
// API Configuration
// ============================================================================

/// Radio Paradise API base URL
///
/// Base URL for all Radio Paradise API requests.
/// This is hardcoded as Radio Paradise's API endpoint doesn't change.
///
/// Value: https://api.radioparadise.com
pub const API_BASE_URL: &str = "https://api.radioparadise.com";

/// API request timeout (seconds)
///
/// Maximum time to wait for an API response before considering it failed.
///
/// Value: 30 seconds
pub const API_TIMEOUT_SECONDS: u64 = 30;

/// User agent for API requests
///
/// Identifies PMOMusic in HTTP requests to Radio Paradise's servers.
///
/// Value: PMO-RadioParadise/1.0
pub const API_USER_AGENT: &str = "PMO-RadioParadise/1.0";

/// Helper to get API timeout as Duration
pub fn api_timeout() -> Duration {
    Duration::from_secs(API_TIMEOUT_SECONDS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_duration_helpers() {
        assert_eq!(polling_high_interval(), Duration::from_secs(120));
        assert_eq!(polling_medium_interval(), Duration::from_secs(60));
        assert_eq!(polling_low_interval(), Duration::from_secs(20));
        assert_eq!(api_timeout(), Duration::from_secs(30));
    }

    #[test]
    fn test_constants_sanity() {
        // Polling intervals should be ordered
        assert!(POLLING_INTERVAL_LOW_BUFFER < POLLING_INTERVAL_MEDIUM_BUFFER);
        assert!(POLLING_INTERVAL_MEDIUM_BUFFER < POLLING_INTERVAL_HIGH_BUFFER);

        // Backoff should be reasonable
        assert!(BACKOFF_INITIAL_SECONDS < BACKOFF_MAX_SECONDS);
        assert!(BACKOFF_MULTIPLIER > 1.0);

        // Cache limits should be positive
        assert!(MAX_BLOCKS_REMEMBERED > 0);
        assert!(TRACK_ID_HASH_BYTES > 0);

        // History should be reasonable
        assert!(HISTORY_DEFAULT_MAX_TRACKS > 0);
    }
}
