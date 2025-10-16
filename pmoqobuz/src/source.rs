//! Music source implementation for Qobuz
//!
//! This module implements the [`pmosource::MusicSource`] trait for Qobuz,
//! providing access to the service's default image and identification information.

use pmosource::MusicSource;

/// Default image for Qobuz (300x300 WebP, embedded in binary)
const DEFAULT_IMAGE: &[u8] = include_bytes!("../assets/default.webp");

/// Qobuz music source
///
/// This struct implements the [`MusicSource`] trait to provide
/// standardized access to Qobuz's identification and branding.
///
/// # Examples
///
/// ```
/// use pmoqobuz::QobuzSource;
/// use pmosource::MusicSource;
///
/// let source = QobuzSource;
/// assert_eq!(source.name(), "Qobuz");
/// assert_eq!(source.id(), "qobuz");
///
/// // Get default image as WebP bytes
/// let image_data = source.default_image();
/// assert!(image_data.len() > 0);
/// ```
#[derive(Debug, Clone, Copy, Default)]
pub struct QobuzSource;

impl MusicSource for QobuzSource {
    fn name(&self) -> &str {
        "Qobuz"
    }

    fn id(&self) -> &str {
        "qobuz"
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
        let source = QobuzSource;
        assert_eq!(source.name(), "Qobuz");
        assert_eq!(source.id(), "qobuz");
        assert_eq!(source.default_image_mime_type(), "image/webp");
    }

    #[test]
    fn test_default_image_present() {
        let source = QobuzSource;
        let image = source.default_image();
        assert!(image.len() > 0, "Default image should not be empty");

        // Check WebP magic bytes (RIFF...WEBP)
        assert!(image.len() >= 12, "Image too small to be valid WebP");
        assert_eq!(&image[0..4], b"RIFF", "Missing RIFF header");
        assert_eq!(&image[8..12], b"WEBP", "Missing WEBP signature");
    }
}
