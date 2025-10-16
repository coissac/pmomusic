//! Music source implementation for Radio Paradise
//!
//! This module implements the [`pmosource::MusicSource`] trait for Radio Paradise,
//! providing access to the service's default image and identification information.

use pmosource::MusicSource;

/// Default image for Radio Paradise (300x300 WebP, embedded in binary)
const DEFAULT_IMAGE: &[u8] = include_bytes!("../assets/default.webp");

/// Radio Paradise music source
///
/// This struct implements the [`MusicSource`] trait to provide
/// standardized access to Radio Paradise's identification and branding.
///
/// # Examples
///
/// ```
/// use pmoparadise::RadioParadiseSource;
/// use pmosource::MusicSource;
///
/// let source = RadioParadiseSource;
/// assert_eq!(source.name(), "Radio Paradise");
/// assert_eq!(source.id(), "radio-paradise");
///
/// // Get default image as WebP bytes
/// let image_data = source.default_image();
/// assert!(image_data.len() > 0);
/// ```
#[derive(Debug, Clone, Copy, Default)]
pub struct RadioParadiseSource;

impl MusicSource for RadioParadiseSource {
    fn name(&self) -> &str {
        "Radio Paradise"
    }

    fn id(&self) -> &str {
        "radio-paradise"
    }

    fn default_image(&self) -> &[u8] {
        DEFAULT_IMAGE
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_source_info() {
        let source = RadioParadiseSource;
        assert_eq!(source.name(), "Radio Paradise");
        assert_eq!(source.id(), "radio-paradise");
        assert_eq!(source.default_image_mime_type(), "image/webp");
    }

    #[test]
    fn test_default_image_present() {
        let source = RadioParadiseSource;
        let image = source.default_image();
        assert!(image.len() > 0, "Default image should not be empty");

        // Check WebP magic bytes (RIFF...WEBP)
        assert!(image.len() >= 12, "Image too small to be valid WebP");
        assert_eq!(&image[0..4], b"RIFF", "Missing RIFF header");
        assert_eq!(&image[8..12], b"WEBP", "Missing WEBP signature");
    }
}
