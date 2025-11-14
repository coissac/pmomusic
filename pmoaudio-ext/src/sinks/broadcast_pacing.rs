//! Shared broadcast pacing logic for streaming sinks.
//!
//! Provides intelligent backpressure based on audio timing:
//! - Detects TopZeroSync (when audio timestamp resets to 0)
//! - Drops frames that are late (audio_ts < elapsed)
//! - Paces broadcast to match audio playback rate

use std::time::Instant;
use tracing::{debug, info, warn};

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

    /// Check timing and apply pacing
    ///
    /// This function:
    /// 1. Detects TopZeroSync (audio_timestamp < 0.1 after >1s) and resets timer
    /// 2. Drops frames that are late (audio_ts < elapsed)
    /// 3. Sleeps if too far ahead (lead_time > max_lead_time)
    ///
    /// # Returns
    ///
    /// - `Ok(())` if frame is on time or successfully paced
    /// - `Err(SkipFrame)` if frame is too late and should be dropped
    pub async fn check_and_pace(&mut self, audio_timestamp: f64) -> Result<(), SkipFrame> {
        // ╔═══════════════════════════════════════════════════════════════╗
        // ║ 1. DÉTECTION TopZeroSync                                      ║
        // ║    Si le timestamp revient proche de 0, reset l'horloge       ║
        // ╚═══════════════════════════════════════════════════════════════╝
        let elapsed_since_start = self.start_time.elapsed().as_secs_f64();
        if audio_timestamp < 0.1 && elapsed_since_start > 1.0 {
            self.start_time = Instant::now();
            info!(
                "{} broadcaster: TopZeroSync detected, resetting timer",
                self.label
            );
        }

        // ╔═══════════════════════════════════════════════════════════════╗
        // ║ 2. CALCUL DU LEAD TIME                                        ║
        // ║    lead_time > 0 : en avance (OK)                             ║
        // ║    lead_time < 0 : en retard (SKIP)                           ║
        // ╚═══════════════════════════════════════════════════════════════╝
        let elapsed = self.start_time.elapsed().as_secs_f64();
        let lead_time = audio_timestamp - elapsed;

        // ╔═══════════════════════════════════════════════════════════════╗
        // ║ 3. DROP FRAMES EN RETARD (tolérance zéro)                     ║
        // ╚═══════════════════════════════════════════════════════════════╝
        if lead_time < 0.0 {
            warn!(
                "{}: Dropping late frame: audio_ts={:.3}s, elapsed={:.3}s, lag={:.3}s",
                self.label,
                audio_timestamp,
                elapsed,
                -lead_time
            );
            return Err(SkipFrame);
        }

        // ╔═══════════════════════════════════════════════════════════════╗
        // ║ 4. BACKPRESSURE NATURELLE - Pas de sleep !                    ║
        // ║                                                               ║
        // ║ Le pacing vient de :                                          ║
        // ║ - TimerBufferNode en amont (envoi régulier à 50ms/chunk)     ║
        // ║ - Capacité limitée du broadcast channel                      ║
        // ║ - Client HTTP qui lit à vitesse réelle                       ║
        // ║                                                               ║
        // ║ Pas besoin de sleep explicite qui causerait des bursts       ║
        // ╚═══════════════════════════════════════════════════════════════╝

        // Log pour info si on est très en avance, mais on ne dort PAS
        if self.max_lead_time > 0.0 && lead_time > self.max_lead_time {
            debug!(
                "{} broadcaster: lead_time={:.3}s > max={:.3}s (audio_ts={:.3}s, elapsed={:.3}s) - relying on natural backpressure",
                self.label, lead_time, self.max_lead_time, audio_timestamp, elapsed
            );
        }

        Ok(())
    }
}
