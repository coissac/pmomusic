//! Audio file metadata extraction using lofty.
//!
//! This module provides utilities to extract both technical audio properties
//! (sample rate, channels, bit depth) and artistic tags (title, artist, album)
//! from audio files in various formats.
//!
//! # Examples
//!
//! ```no_run
//! use pmoflac::AudioFileMetadata;
//! use std::path::Path;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let metadata = AudioFileMetadata::from_file(Path::new("audio.flac"))?;
//!
//! println!("Title: {:?}", metadata.title);
//! println!("Artist: {:?}", metadata.artist);
//! println!("Sample rate: {:?} Hz", metadata.sample_rate);
//! println!("Duration: {:?} seconds", metadata.duration_secs);
//! # Ok(())
//! # }
//! ```

use lofty::{config::ParseOptions, prelude::*, probe::Probe};
use std::{io::Cursor, path::Path};

/// Comprehensive audio file metadata including both technical properties and artistic tags.
///
/// This struct is populated from audio file tags using the lofty library,
/// which supports FLAC, MP3, Ogg Vorbis, Opus, WAV, AIFF, and other formats.
#[derive(Debug, Clone, Default)]
pub struct AudioFileMetadata {
    // Artistic tags
    /// Track title
    pub title: Option<String>,
    /// Artist name
    pub artist: Option<String>,
    /// Album name
    pub album: Option<String>,
    /// Year of release
    pub year: Option<u32>,
    /// Music genre
    pub genre: Option<String>,
    /// Track number in the album
    pub track_number: Option<u32>,
    /// Total number of tracks in the album
    pub track_total: Option<u32>,
    /// Disc number (for multi-disc albums)
    pub disc_number: Option<u32>,
    /// Total number of discs
    pub disc_total: Option<u32>,

    // Technical audio properties
    /// Duration in seconds
    pub duration_secs: Option<u64>,
    /// Sample rate in Hz (e.g., 44100, 48000)
    pub sample_rate: Option<u32>,
    /// Number of audio channels (1 = mono, 2 = stereo, etc.)
    pub channels: Option<u8>,
    /// Bitrate in bits per second
    pub bitrate: Option<u32>,
}

impl AudioFileMetadata {
    /// Extracts metadata from an audio file on disk.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the audio file
    ///
    /// # Returns
    ///
    /// Returns `AudioFileMetadata` on success, or a `lofty::error::LoftyError` if
    /// the file cannot be read or parsed.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use pmoflac::AudioFileMetadata;
    /// use std::path::Path;
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let metadata = AudioFileMetadata::from_file(Path::new("song.flac"))?;
    /// println!("Duration: {:?}s", metadata.duration_secs);
    /// # Ok(())
    /// # }
    /// ```
    pub fn from_file(path: &Path) -> Result<Self, lofty::error::LoftyError> {
        tracing::debug!(path = %path.display(), "Extracting metadata from file");

        let tagged_file = Probe::open(path)?.options(ParseOptions::new()).read()?;

        let metadata = Self::from_tagged_file(tagged_file);

        tracing::debug!(
            path = %path.display(),
            title = ?metadata.title,
            artist = ?metadata.artist,
            duration_secs = ?metadata.duration_secs,
            "Metadata extracted successfully"
        );

        Ok(metadata)
    }

    /// Extracts metadata from audio file bytes in memory.
    ///
    /// # Arguments
    ///
    /// * `data` - Audio file data as a byte slice
    ///
    /// # Returns
    ///
    /// Returns `AudioFileMetadata` on success, or a `lofty::error::LoftyError` if
    /// the data cannot be parsed.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use pmoflac::AudioFileMetadata;
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let audio_bytes: &[u8] = &[/* ... */];
    /// let metadata = AudioFileMetadata::from_bytes(audio_bytes)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn from_bytes(data: &[u8]) -> Result<Self, lofty::error::LoftyError> {
        tracing::debug!(size = data.len(), "Extracting metadata from bytes");

        let cursor = Cursor::new(data);
        let tagged_file = Probe::new(cursor)
            .guess_file_type()?
            .options(ParseOptions::new())
            .read()?;

        Ok(Self::from_tagged_file(tagged_file))
    }

    /// Internal helper to extract metadata from a lofty `TaggedFile`.
    fn from_tagged_file(tagged_file: lofty::file::TaggedFile) -> Self {
        let properties = tagged_file.properties();

        // Try to get the primary tag, or fall back to the first available tag
        let tag = tagged_file
            .primary_tag()
            .or_else(|| tagged_file.first_tag());

        // Initialize with technical properties
        let mut metadata = Self {
            title: None,
            artist: None,
            album: None,
            year: None,
            genre: None,
            track_number: None,
            track_total: None,
            disc_number: None,
            disc_total: None,
            duration_secs: Some(properties.duration().as_secs()),
            sample_rate: properties.sample_rate(),
            channels: properties.channels(),
            bitrate: properties.audio_bitrate(),
        };

        // Extract artistic tags if available
        if let Some(tag) = tag {
            metadata.title = tag.title().map(|s| s.to_string());
            metadata.artist = tag.artist().map(|s| s.to_string());
            metadata.album = tag.album().map(|s| s.to_string());
            metadata.year = tag.year();
            metadata.genre = tag.genre().map(|s| s.to_string());
            metadata.track_number = tag.track();
            metadata.track_total = tag.track_total();
            metadata.disc_number = tag.disk();
            metadata.disc_total = tag.disk_total();
        } else {
            tracing::warn!("No tags found in audio file");
        }

        metadata
    }

    /// Returns a formatted duration string in H:MM:SS format (DIDL-Lite compatible).
    ///
    /// # Examples
    ///
    /// ```
    /// use pmoflac::AudioFileMetadata;
    ///
    /// let mut metadata = AudioFileMetadata {
    ///     duration_secs: Some(3665), // 1 hour, 1 minute, 5 seconds
    ///     title: None,
    ///     artist: None,
    ///     album: None,
    ///     year: None,
    ///     genre: None,
    ///     track_number: None,
    ///     track_total: None,
    ///     disc_number: None,
    ///     disc_total: None,
    ///     sample_rate: None,
    ///     channels: None,
    ///     bitrate: None,
    /// };
    ///
    /// assert_eq!(metadata.duration_formatted(), "1:01:05");
    /// ```
    pub fn duration_formatted(&self) -> String {
        match self.duration_secs {
            Some(secs) => {
                let hours = secs / 3600;
                let minutes = (secs % 3600) / 60;
                let seconds = secs % 60;
                format!("{}:{:02}:{:02}", hours, minutes, seconds)
            }
            None => "0:00:00".to_string(),
        }
    }

    /// Returns a collection key in the format "artist:album" for grouping tracks.
    ///
    /// This is useful for organizing tracks into albums in a music library.
    ///
    /// # Examples
    ///
    /// ```
    /// use pmoflac::AudioFileMetadata;
    ///
    /// let metadata = AudioFileMetadata {
    ///     artist: Some("Pink Floyd".to_string()),
    ///     album: Some("The Dark Side of the Moon".to_string()),
    ///     title: None,
    ///     year: None,
    ///     genre: None,
    ///     track_number: None,
    ///     track_total: None,
    ///     disc_number: None,
    ///     disc_total: None,
    ///     duration_secs: None,
    ///     sample_rate: None,
    ///     channels: None,
    ///     bitrate: None,
    /// };
    ///
    /// assert_eq!(metadata.collection_key(), "Pink Floyd:The Dark Side of the Moon");
    /// ```
    pub fn collection_key(&self) -> String {
        match (&self.artist, &self.album) {
            (Some(artist), Some(album)) => format!("{}:{}", artist, album),
            (Some(artist), None) => artist.clone(),
            (None, Some(album)) => album.clone(),
            (None, None) => "Unknown".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_duration_formatted() {
        let mut meta = AudioFileMetadata::default();

        meta.duration_secs = Some(65);
        assert_eq!(meta.duration_formatted(), "0:01:05");

        meta.duration_secs = Some(3665);
        assert_eq!(meta.duration_formatted(), "1:01:05");

        meta.duration_secs = Some(0);
        assert_eq!(meta.duration_formatted(), "0:00:00");

        meta.duration_secs = None;
        assert_eq!(meta.duration_formatted(), "0:00:00");
    }

    #[test]
    fn test_collection_key() {
        let mut meta = AudioFileMetadata::default();

        meta.artist = Some("Artist".to_string());
        meta.album = Some("Album".to_string());
        assert_eq!(meta.collection_key(), "Artist:Album");

        meta.album = None;
        assert_eq!(meta.collection_key(), "Artist");

        meta.artist = None;
        meta.album = Some("Album".to_string());
        assert_eq!(meta.collection_key(), "Album");

        meta.album = None;
        assert_eq!(meta.collection_key(), "Unknown");
    }
}
