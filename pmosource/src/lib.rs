//! # PMOSource
//!
//! Common traits and types for PMOMusic sources.
//!
//! This crate provides the foundational abstractions for different music sources
//! in the PMOMusic ecosystem, such as Radio Paradise, Qobuz, etc.

use std::fmt::Debug;

/// Standard size for default images (300x300 pixels)
pub const DEFAULT_IMAGE_SIZE: u32 = 300;

/// Error types for music source operations
#[derive(Debug, thiserror::Error)]
pub enum MusicSourceError {
    #[error("Failed to load default image: {0}")]
    ImageLoadError(String),

    #[error("Invalid image format: {0}")]
    InvalidImageFormat(String),

    #[error("Source not available: {0}")]
    SourceUnavailable(String),
}

/// Result type for music source operations
pub type Result<T> = std::result::Result<T, MusicSourceError>;

/// Main trait for music sources
///
/// This trait defines the common interface that all music sources must implement.
/// It provides methods for:
/// - Getting the source name and identification
/// - Retrieving default images/logos
/// - Other common operations (to be extended)
pub trait MusicSource: Debug + Send + Sync {
    /// Returns the human-readable name of the music source
    ///
    /// # Examples
    ///
    /// ```ignore
    /// assert_eq!(source.name(), "Radio Paradise");
    /// ```
    fn name(&self) -> &str;

    /// Returns a unique identifier for the music source
    ///
    /// This is typically a lowercase, hyphenated version of the name
    /// suitable for use in URLs, file names, etc.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// assert_eq!(source.id(), "radio-paradise");
    /// ```
    fn id(&self) -> &str;

    /// Returns the default image/logo for this source as WebP bytes
    ///
    /// The image should be square (300x300 pixels) and in WebP format.
    /// This is embedded in the binary for offline availability.
    ///
    /// # Returns
    ///
    /// A byte slice containing the WebP-encoded image data
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let image_data = source.default_image();
    /// assert!(image_data.len() > 0);
    /// ```
    fn default_image(&self) -> &[u8];

    /// Returns the MIME type of the default image
    ///
    /// By default, this returns "image/webp" since all default images
    /// should be in WebP format.
    fn default_image_mime_type(&self) -> &str {
        "image/webp"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct TestSource;

    impl MusicSource for TestSource {
        fn name(&self) -> &str {
            "Test Source"
        }

        fn id(&self) -> &str {
            "test-source"
        }

        fn default_image(&self) -> &[u8] {
            &[]
        }
    }

    #[test]
    fn test_music_source_trait() {
        let source = TestSource;
        assert_eq!(source.name(), "Test Source");
        assert_eq!(source.id(), "test-source");
        assert_eq!(source.default_image_mime_type(), "image/webp");
    }
}
