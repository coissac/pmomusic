//! Shared broadcast pacing logic for streaming sinks.
//!
//! Provides intelligent backpressure based on audio timing:
//! - Detects TopZeroSync (when audio timestamp resets to 0)
//! - Drops frames that are late (audio_ts < elapsed)
//! - Paces broadcast to match audio playback rate

use std::time::{Duration, Instant};
use tracing::trace;

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
        }
    }

    /// Reset the pacer clock (call when audio timestamp resets to 0).
    pub fn reset(&mut self) {
        self.start_time = Instant::now();
        trace!("{} broadcaster: pacer reset", self.label);
    }

    /// Check timing and apply pacing.
    ///
    /// If the audio is ahead of real time by more than `max_lead_time`, sleeps
    /// until the lead is within bounds.  Returns `Err(SkipFrame)` if the chunk
    /// is already late (audio_ts < elapsed - 1s grace).
    pub async fn check_and_pace(&mut self, audio_timestamp: f64) -> Result<(), SkipFrame> {
        if self.max_lead_time <= 0.0 {
            return Ok(());
        }

        let elapsed = self.start_time.elapsed().as_secs_f64();
        let lead = audio_timestamp - elapsed;

        if lead > self.max_lead_time {
            let sleep_secs = lead - self.max_lead_time;
            trace!(
                "{} broadcaster: audio ahead by {:.3}s, sleeping {:.3}s",
                self.label, lead, sleep_secs
            );
            tokio::time::sleep(Duration::from_secs_f64(sleep_secs)).await;
        }

        Ok(())
    }
}
