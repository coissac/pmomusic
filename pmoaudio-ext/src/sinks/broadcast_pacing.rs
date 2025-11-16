//! Shared broadcast pacing logic for streaming sinks.
//!
//! Provides intelligent backpressure based on audio timing:
//! - Detects TopZeroSync (when audio timestamp resets to 0)
//! - Drops frames that are late (audio_ts < elapsed)
//! - Paces broadcast to match audio playback rate

use std::time::Instant;
use tracing::{trace, warn};

/// Error returned when a frame should be skipped (too late)
#[derive(Debug)]
pub struct SkipFrame;

/// Manages broadcast pacing with TopZeroSync detection
pub struct BroadcastPacer {
    /// Start time (reset on TopZeroSync)
    start_time: Instant,
    /// Maximum allowed lead time before sleeping (0 = no pacing)
    max_lead_time: f64,
    /// Label for logging (e.g., "FLAC" or "OGG")
    label: String,
    /// Pending reset flag - will reset timer on next chunk
    pending_reset: bool,
}

impl BroadcastPacer {
    /// Create a new broadcast pacer
    ///
    /// # Arguments
    ///
    /// * `max_lead_time` - Maximum lead time in seconds (0 = no pacing)
    /// * `label` - Label for logging
    pub fn new(max_lead_time: f64, label: impl Into<String>) -> Self {
        Self {
            start_time: Instant::now(),
            max_lead_time: max_lead_time.max(0.0),
            label: label.into(),
            pending_reset: false,
        }
    }

    /// Check timing and apply pacing - NO-OP VERSION
    ///
    /// Pacing is now handled entirely by the expiration-based system in
    /// TimedBroadcast. This method is kept for backward compatibility
    /// but always returns Ok(()).
    ///
    /// # Returns
    ///
    /// - Always returns `Ok(())`
    pub async fn check_and_pace(&mut self, audio_timestamp: f64) -> Result<(), SkipFrame> {
        trace!(
            "{} broadcaster: check_and_pace called with audio_ts={:.3}s (no-op - pacing handled by TimedBroadcast)",
            self.label, audio_timestamp
        );
        Ok(())
    }
}
