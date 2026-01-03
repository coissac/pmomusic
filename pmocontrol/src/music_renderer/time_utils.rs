//! Time formatting and parsing utilities for music renderers.
//!
//! This module provides utilities to convert between different time representations:
//! - HH:MM:SS format (UPnP standard)
//! - Seconds (u32/u64)
//! - Milliseconds
//!
//! All functions are designed to be robust and provide clear error messages.

use crate::errors::ControlPointError;

/// Formats a duration in seconds as HH:MM:SS.
///
/// # Examples
/// ```
/// # use pmocontrol::music_renderer::time_utils::format_hhmmss;
/// assert_eq!(format_hhmmss(0), "00:00:00");
/// assert_eq!(format_hhmmss(61), "00:01:01");
/// assert_eq!(format_hhmmss(3661), "01:01:01");
/// ```
pub fn format_hhmmss(seconds: u64) -> String {
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    let secs = seconds % 60;
    format!("{:02}:{:02}:{:02}", hours, minutes, secs)
}

/// Formats a duration in seconds as HH:MM:SS (u32 variant).
///
/// Convenience wrapper for u32 values.
pub fn format_hhmmss_u32(seconds: u32) -> String {
    format_hhmmss(seconds as u64)
}

/// Formats a duration in seconds as HH:MM:SS (f64 variant).
///
/// Used by Chromecast which returns floating point durations.
/// Rounds to nearest second.
pub fn format_hhmmss_f64(seconds: f64) -> String {
    format_hhmmss(seconds.round() as u64)
}

/// Parses a time string in HH:MM:SS, MM:SS, or SS format to seconds.
///
/// This is the most flexible parser, supporting multiple formats:
/// - "HH:MM:SS" → hours * 3600 + minutes * 60 + seconds
/// - "MM:SS" → minutes * 60 + seconds
/// - "SS" → seconds
///
/// # Examples
/// ```
/// # use pmocontrol::music_renderer::time_utils::parse_time_flexible;
/// assert_eq!(parse_time_flexible("01:02:03").unwrap(), 3723);
/// assert_eq!(parse_time_flexible("02:03").unwrap(), 123);
/// assert_eq!(parse_time_flexible("42").unwrap(), 42);
/// ```
///
/// # Errors
/// Returns an error if:
/// - The input has more than 3 parts
/// - Any part is not a valid u32
pub fn parse_time_flexible(input: &str) -> Result<u32, ControlPointError> {
    let parts: Vec<&str> = input.split(':').collect();

    if parts.is_empty() || parts.len() > 3 {
        return Err(ControlPointError::InvalidTimeFormat(
            format!("Invalid time format '{}': expected HH:MM:SS, MM:SS, or SS", input)
        ));
    }

    let mut total = 0u32;
    for part in parts {
        let value = part.parse::<u32>().map_err(|_| {
            ControlPointError::InvalidTimeFormat(
                format!("Invalid numeric value '{}' in time string '{}'", part, input)
            )
        })?;
        total = total * 60 + value;
    }

    Ok(total)
}

/// Parses a strict HH:MM:SS format to seconds.
///
/// This parser requires exactly 3 components separated by colons,
/// and validates that minutes and seconds are < 60.
///
/// # Examples
/// ```
/// # use pmocontrol::music_renderer::time_utils::parse_hhmmss_strict;
/// assert_eq!(parse_hhmmss_strict("01:02:03").unwrap(), 3723);
/// assert!(parse_hhmmss_strict("02:03").is_err());  // requires HH:MM:SS
/// assert!(parse_hhmmss_strict("00:61:00").is_err());  // minutes > 59
/// ```
///
/// # Errors
/// Returns an error if:
/// - The format is not exactly HH:MM:SS
/// - Minutes or seconds are >= 60
/// - Any component is not a valid u64
pub fn parse_hhmmss_strict(input: &str) -> Result<u64, ControlPointError> {
    let parts: Vec<&str> = input.split(':').collect();

    if parts.len() != 3 {
        return Err(ControlPointError::InvalidTimeFormat(
            format!("Invalid time format '{}': expected exactly HH:MM:SS", input)
        ));
    }

    let hours: u64 = parts[0].parse().map_err(|_| {
        ControlPointError::InvalidTimeFormat(
            format!("Invalid hour component in '{}'", input)
        )
    })?;

    let minutes: u64 = parts[1].parse().map_err(|_| {
        ControlPointError::InvalidTimeFormat(
            format!("Invalid minute component in '{}'", input)
        )
    })?;

    let seconds: u64 = parts[2].parse().map_err(|_| {
        ControlPointError::InvalidTimeFormat(
            format!("Invalid second component in '{}'", input)
        )
    })?;

    if minutes >= 60 || seconds >= 60 {
        return Err(ControlPointError::InvalidTimeFormat(
            format!("Invalid time '{}': minutes and seconds must be < 60", input)
        ));
    }

    Ok(hours * 3600 + minutes * 60 + seconds)
}

/// Converts milliseconds to seconds (rounding down).
#[inline]
pub fn ms_to_seconds(milliseconds: u64) -> u64 {
    milliseconds / 1000
}

/// Converts seconds to milliseconds.
#[inline]
pub fn seconds_to_ms(seconds: u64) -> u64 {
    seconds * 1000
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_hhmmss() {
        assert_eq!(format_hhmmss(0), "00:00:00");
        assert_eq!(format_hhmmss(1), "00:00:01");
        assert_eq!(format_hhmmss(60), "00:01:00");
        assert_eq!(format_hhmmss(61), "00:01:01");
        assert_eq!(format_hhmmss(3600), "01:00:00");
        assert_eq!(format_hhmmss(3661), "01:01:01");
        assert_eq!(format_hhmmss(86399), "23:59:59");
    }

    #[test]
    fn test_format_hhmmss_f64() {
        assert_eq!(format_hhmmss_f64(123.4), "00:02:03");
        assert_eq!(format_hhmmss_f64(123.6), "00:02:04");
    }

    #[test]
    fn test_parse_time_flexible() {
        // HH:MM:SS format
        assert_eq!(parse_time_flexible("01:02:03").unwrap(), 3723);
        assert_eq!(parse_time_flexible("00:00:00").unwrap(), 0);
        assert_eq!(parse_time_flexible("23:59:59").unwrap(), 86399);

        // MM:SS format
        assert_eq!(parse_time_flexible("02:03").unwrap(), 123);
        assert_eq!(parse_time_flexible("00:00").unwrap(), 0);

        // SS format
        assert_eq!(parse_time_flexible("42").unwrap(), 42);
        assert_eq!(parse_time_flexible("0").unwrap(), 0);

        // Errors
        assert!(parse_time_flexible("").is_err());
        assert!(parse_time_flexible("1:2:3:4").is_err());
        assert!(parse_time_flexible("abc").is_err());
        assert!(parse_time_flexible("1:abc").is_err());
    }

    #[test]
    fn test_parse_hhmmss_strict() {
        assert_eq!(parse_hhmmss_strict("01:02:03").unwrap(), 3723);
        assert_eq!(parse_hhmmss_strict("00:00:00").unwrap(), 0);
        assert_eq!(parse_hhmmss_strict("23:59:59").unwrap(), 86399);

        // Errors - wrong format
        assert!(parse_hhmmss_strict("02:03").is_err());
        assert!(parse_hhmmss_strict("42").is_err());

        // Errors - invalid values
        assert!(parse_hhmmss_strict("00:60:00").is_err());
        assert!(parse_hhmmss_strict("00:00:60").is_err());
        assert!(parse_hhmmss_strict("abc:00:00").is_err());
    }

    #[test]
    fn test_ms_conversions() {
        assert_eq!(ms_to_seconds(1000), 1);
        assert_eq!(ms_to_seconds(1500), 1);  // rounds down
        assert_eq!(ms_to_seconds(999), 0);

        assert_eq!(seconds_to_ms(1), 1000);
        assert_eq!(seconds_to_ms(0), 0);
    }
}
