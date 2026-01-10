//! Sleep timer functionality for auto-stop feature.
//!
//! Provides a sleep timer that can automatically stop playback after a configured duration.
//! Maximum duration is 2 hours (7200 seconds).

use std::time::Instant;

/// Sleep timer state for auto-stop functionality.
#[derive(Debug, Clone)]
pub struct SleepTimer {
    /// When the timer expires (None if no timer active).
    end_time: Option<Instant>,
    /// Total duration in seconds configured for the timer.
    duration_seconds: u32,
}

impl Default for SleepTimer {
    fn default() -> Self {
        Self {
            end_time: None,
            duration_seconds: 0,
        }
    }
}

impl SleepTimer {
    /// Maximum timer duration in seconds (2 hours).
    pub const MAX_DURATION: u32 = 7200;

    /// Creates a new inactive timer.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the remaining seconds, or None if no timer is active.
    pub fn remaining_seconds(&self) -> Option<u32> {
        self.end_time.map(|end| {
            let now = Instant::now();
            if now >= end {
                0
            } else {
                end.duration_since(now).as_secs() as u32
            }
        })
    }

    /// Returns the configured duration in seconds.
    pub fn duration_seconds(&self) -> u32 {
        self.duration_seconds
    }

    /// Returns true if the timer is active.
    pub fn is_active(&self) -> bool {
        self.end_time.is_some()
    }

    /// Returns true if the timer has expired.
    pub fn is_expired(&self) -> bool {
        self.end_time
            .map(|end| Instant::now() >= end)
            .unwrap_or(false)
    }

    /// Starts or restarts the timer with the given duration in seconds.
    /// Maximum duration is 2 hours (7200 seconds).
    ///
    /// # Errors
    /// Returns an error if:
    /// - duration is 0
    /// - duration exceeds MAX_DURATION (7200 seconds)
    pub fn start(&mut self, duration_seconds: u32) -> Result<(), String> {
        if duration_seconds == 0 {
            return Err("Duration must be greater than 0".to_string());
        }

        if duration_seconds > Self::MAX_DURATION {
            return Err(format!(
                "Duration cannot exceed {} seconds (2 hours)",
                Self::MAX_DURATION
            ));
        }

        self.duration_seconds = duration_seconds;
        self.end_time =
            Some(Instant::now() + std::time::Duration::from_secs(duration_seconds as u64));
        Ok(())
    }

    /// Updates the timer duration. Resets the timer to the new duration from now.
    ///
    /// # Errors
    /// Returns an error if:
    /// - duration is 0
    /// - duration exceeds MAX_DURATION (7200 seconds)
    pub fn update(&mut self, duration_seconds: u32) -> Result<(), String> {
        if duration_seconds == 0 {
            return Err("Duration must be greater than 0".to_string());
        }

        if duration_seconds > Self::MAX_DURATION {
            return Err(format!(
                "Duration cannot exceed {} seconds (2 hours)",
                Self::MAX_DURATION
            ));
        }

        self.duration_seconds = duration_seconds;
        self.end_time =
            Some(Instant::now() + std::time::Duration::from_secs(duration_seconds as u64));
        Ok(())
    }

    /// Cancels the timer.
    pub fn cancel(&mut self) {
        self.end_time = None;
        self.duration_seconds = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_timer_creation() {
        let timer = SleepTimer::new();
        assert!(!timer.is_active());
        assert_eq!(timer.remaining_seconds(), None);
        assert_eq!(timer.duration_seconds(), 0);
    }

    #[test]
    fn test_timer_start() {
        let mut timer = SleepTimer::new();
        assert!(timer.start(60).is_ok());
        assert!(timer.is_active());
        assert_eq!(timer.duration_seconds(), 60);

        let remaining = timer.remaining_seconds().unwrap();
        assert!(remaining <= 60 && remaining > 58); // Account for execution time
    }

    #[test]
    fn test_timer_validation() {
        let mut timer = SleepTimer::new();

        // Zero duration should fail
        assert!(timer.start(0).is_err());

        // Exceeding max duration should fail
        assert!(timer.start(SleepTimer::MAX_DURATION + 1).is_err());

        // Valid durations should succeed
        assert!(timer.start(1).is_ok());
        assert!(timer.start(SleepTimer::MAX_DURATION).is_ok());
    }

    #[test]
    fn test_timer_expiration() {
        let mut timer = SleepTimer::new();
        timer.start(1).unwrap();
        assert!(timer.is_active());
        assert!(!timer.is_expired());

        thread::sleep(Duration::from_millis(1100));

        assert!(timer.is_expired());
        assert_eq!(timer.remaining_seconds().unwrap(), 0);
    }

    #[test]
    fn test_timer_update() {
        let mut timer = SleepTimer::new();
        timer.start(60).unwrap();

        thread::sleep(Duration::from_millis(500));

        // Update should reset the timer
        timer.update(30).unwrap();
        assert_eq!(timer.duration_seconds(), 30);

        let remaining = timer.remaining_seconds().unwrap();
        assert!(remaining <= 30 && remaining > 28);
    }

    #[test]
    fn test_timer_cancel() {
        let mut timer = SleepTimer::new();
        timer.start(60).unwrap();
        assert!(timer.is_active());

        timer.cancel();
        assert!(!timer.is_active());
        assert_eq!(timer.remaining_seconds(), None);
        assert_eq!(timer.duration_seconds(), 0);
    }
}
