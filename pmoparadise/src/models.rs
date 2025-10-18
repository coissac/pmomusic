//! Data models for Radio Paradise API responses

use serde::{Deserialize, Deserializer, Serialize};
use std::collections::HashMap;

/// Deserialize a string or number into a u64
fn deserialize_string_or_u64<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: Deserializer<'de>,
{
    use serde::de::Error;

    #[derive(Deserialize)]
    #[serde(untagged)]
    enum StringOrU64 {
        String(String),
        Number(u64),
    }

    match StringOrU64::deserialize(deserializer)? {
        StringOrU64::String(s) => s.parse::<u64>().map_err(D::Error::custom),
        StringOrU64::Number(n) => Ok(n),
    }
}

/// Deserialize a string or number into a f64, then convert to u64 milliseconds
fn deserialize_length<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: Deserializer<'de>,
{
    use serde::de::Error;

    #[derive(Deserialize)]
    #[serde(untagged)]
    enum StringOrNumber {
        String(String),
        Float(f64),
        Int(u64),
    }

    match StringOrNumber::deserialize(deserializer)? {
        StringOrNumber::String(s) => {
            let seconds = s.parse::<f64>().map_err(D::Error::custom)?;
            Ok((seconds * 1000.0) as u64)
        }
        StringOrNumber::Float(f) => Ok((f * 1000.0) as u64),
        StringOrNumber::Int(i) => Ok(i),
    }
}

/// Deserialize an optional string or number into Option<u32>
fn deserialize_optional_string_or_u32<'de, D>(deserializer: D) -> Result<Option<u32>, D::Error>
where
    D: Deserializer<'de>,
{
    use serde::de::Error;

    #[derive(Deserialize)]
    #[serde(untagged)]
    enum StringOrU32 {
        String(String),
        Number(u32),
    }

    let opt = Option::<StringOrU32>::deserialize(deserializer)?;
    match opt {
        None => Ok(None),
        Some(StringOrU32::String(s)) => {
            if s.is_empty() {
                Ok(None)
            } else {
                s.parse::<u32>().map(Some).map_err(D::Error::custom)
            }
        }
        Some(StringOrU32::Number(n)) => Ok(Some(n)),
    }
}

/// Deserialize an optional string or number into Option<f32>
fn deserialize_optional_string_or_f32<'de, D>(deserializer: D) -> Result<Option<f32>, D::Error>
where
    D: Deserializer<'de>,
{
    use serde::de::Error;

    #[derive(Deserialize)]
    #[serde(untagged)]
    enum StringOrF32 {
        String(String),
        Float(f32),
        Int(i32),
    }

    let opt = Option::<StringOrF32>::deserialize(deserializer)?;
    match opt {
        None => Ok(None),
        Some(StringOrF32::String(s)) => {
            if s.is_empty() {
                Ok(None)
            } else {
                s.parse::<f32>().map(Some).map_err(D::Error::custom)
            }
        }
        Some(StringOrF32::Float(f)) => Ok(Some(f)),
        Some(StringOrF32::Int(i)) => Ok(Some(i as f32)),
    }
}

/// Bitrate quality levels for Radio Paradise streams
///
/// Radio Paradise offers 5 quality levels:
/// - 0: 128 kbps MP3
/// - 1: AAC 64 kbps
/// - 2: AAC 128 kbps
/// - 3: AAC 320 kbps
/// - 4: FLAC lossless (CD quality or better)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum Bitrate {
    /// 128 kbps MP3
    Mp3_128 = 0,
    /// AAC 64 kbps
    Aac64 = 1,
    /// AAC 128 kbps
    Aac128 = 2,
    /// AAC 320 kbps
    Aac320 = 3,
    /// FLAC lossless
    Flac = 4,
}

impl Bitrate {
    /// Convert from u8 value
    pub fn from_u8(value: u8) -> Result<Self, crate::error::Error> {
        match value {
            0 => Ok(Self::Mp3_128),
            1 => Ok(Self::Aac64),
            2 => Ok(Self::Aac128),
            3 => Ok(Self::Aac320),
            4 => Ok(Self::Flac),
            _ => Err(crate::error::Error::InvalidBitrate(value)),
        }
    }

    /// Convert to u8 value
    pub fn as_u8(self) -> u8 {
        self as u8
    }

    /// Get human-readable description
    pub fn description(&self) -> &'static str {
        match self {
            Self::Mp3_128 => "MP3 128 kbps",
            Self::Aac64 => "AAC 64 kbps",
            Self::Aac128 => "AAC 128 kbps",
            Self::Aac320 => "AAC 320 kbps",
            Self::Flac => "FLAC Lossless",
        }
    }
}

impl Default for Bitrate {
    fn default() -> Self {
        Self::Flac
    }
}

/// Duration in milliseconds
pub type DurationMs = u64;

/// Event ID for block identification
pub type EventId = u64;

/// Information about a song/track within a block
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Song {
    /// Artist name
    pub artist: String,

    /// Song title
    pub title: String,

    /// Album name (may be missing for promos/announcements)
    #[serde(default)]
    pub album: Option<String>,

    /// Year of release
    /// Note: API returns this as a string, we deserialize to u32
    #[serde(default, deserialize_with = "deserialize_optional_string_or_u32")]
    pub year: Option<u32>,

    /// Elapsed time from start of block in milliseconds
    pub elapsed: DurationMs,

    /// Duration of the track in milliseconds
    pub duration: DurationMs,

    /// Cover image filename/path
    #[serde(default)]
    pub cover: Option<String>,

    /// Rating (0-10)
    /// Note: API returns this as a string, we deserialize to f32
    #[serde(default, deserialize_with = "deserialize_optional_string_or_f32")]
    pub rating: Option<f32>,

    /// Additional metadata
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

impl Song {
    /// Get the end time of this song in the block (elapsed + duration)
    pub fn end_time_ms(&self) -> DurationMs {
        self.elapsed + self.duration
    }

    /// Check if a given timestamp (ms) falls within this song
    pub fn contains_timestamp(&self, timestamp_ms: DurationMs) -> bool {
        timestamp_ms >= self.elapsed && timestamp_ms < self.end_time_ms()
    }
}

/// Image information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageInfo {
    /// Base URL for images
    pub base: String,
}

/// A block of songs from Radio Paradise
///
/// Radio Paradise streams music in "blocks" - continuous FLAC files
/// containing multiple songs. Each block contains metadata about all
/// songs within it and timing information for seeking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Block {
    /// Event ID for this block (start event)
    /// Note: API returns this as a string, we deserialize to u64
    #[serde(deserialize_with = "deserialize_string_or_u64")]
    pub event: EventId,

    /// Event ID for the next block (end event)
    /// Note: API returns this as a string, we deserialize to u64
    #[serde(deserialize_with = "deserialize_string_or_u64")]
    pub end_event: EventId,

    /// Total length of the block in milliseconds
    /// Note: API returns this as a string in seconds (e.g., "1715.54"), we convert to ms
    #[serde(deserialize_with = "deserialize_length")]
    pub length: DurationMs,

    /// URL to stream this block
    pub url: String,

    /// Base URL for cover images
    #[serde(default)]
    pub image_base: Option<String>,

    /// Map of song index (as string) to Song metadata
    /// Keys are "0", "1", "2", etc.
    #[serde(default)]
    pub song: HashMap<String, Song>,

    /// Additional metadata
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

impl Block {
    /// Get songs in order by index
    pub fn songs_ordered(&self) -> Vec<(usize, &Song)> {
        let mut songs: Vec<_> = self.song
            .iter()
            .filter_map(|(k, v)| k.parse::<usize>().ok().map(|idx| (idx, v)))
            .collect();
        songs.sort_by_key(|(idx, _)| *idx);
        songs
    }

    /// Get a song by index
    pub fn get_song(&self, index: usize) -> Option<&Song> {
        self.song.get(&index.to_string())
    }

    /// Get the number of songs in this block
    pub fn song_count(&self) -> usize {
        self.song.len()
    }

    /// Get the full URL for a cover image
    pub fn cover_url(&self, cover_path: &str) -> Option<String> {
        self.image_base.as_ref().map(|base| format!("{}{}", base, cover_path))
    }

    /// Find which song is playing at a given timestamp (ms from block start)
    pub fn song_at_timestamp(&self, timestamp_ms: DurationMs) -> Option<(usize, &Song)> {
        self.songs_ordered()
            .into_iter()
            .find(|(_, song)| song.contains_timestamp(timestamp_ms))
    }

    /// Parse the block URL to get start and end event IDs
    ///
    /// Block URLs follow the pattern:
    /// `https://apps.radioparadise.com/blocks/chan/0/4/<start>-<end>.flac`
    pub fn parse_url_events(&self) -> Option<(EventId, EventId)> {
        let url_path = self.url.split('/').last()?;
        let filename = url_path.strip_suffix(".flac")?;
        let mut parts = filename.split('-');
        let start = parts.next()?.parse::<EventId>().ok()?;
        let end = parts.next()?.parse::<EventId>().ok()?;
        Some((start, end))
    }
}

/// Currently playing information
#[derive(Debug, Clone)]
pub struct NowPlaying {
    /// The current block
    pub block: Block,

    /// Current song index (if determinable)
    pub current_song_index: Option<usize>,

    /// Current song
    pub current_song: Option<Song>,

    /// Approximate elapsed time in current block (ms)
    /// Note: This is estimated and may not be perfectly accurate
    pub block_elapsed_ms: Option<DurationMs>,
}

impl NowPlaying {
    /// Create from a block (assumes starting from beginning)
    pub fn from_block(block: Block) -> Self {
        let (current_song_index, current_song) = block.get_song(0)
            .map(|s| (Some(0), Some(s.clone())))
            .unwrap_or((None, None));

        Self {
            block,
            current_song_index,
            current_song,
            block_elapsed_ms: Some(0),
        }
    }

    /// Get URL for the current block stream
    pub fn stream_url(&self) -> &str {
        &self.block.url
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bitrate_conversion() {
        assert_eq!(Bitrate::from_u8(0).unwrap(), Bitrate::Mp3_128);
        assert_eq!(Bitrate::from_u8(4).unwrap(), Bitrate::Flac);
        assert!(Bitrate::from_u8(5).is_err());
    }

    #[test]
    fn test_song_timing() {
        let song = Song {
            artist: "Test Artist".to_string(),
            title: "Test Song".to_string(),
            album: Some("Test Album".to_string()),
            year: Some(2024),
            elapsed: 1000,
            duration: 5000,
            cover: None,
            rating: None,
            extra: HashMap::new(),
        };

        assert_eq!(song.end_time_ms(), 6000);
        assert!(song.contains_timestamp(3000));
        assert!(!song.contains_timestamp(7000));
        assert!(!song.contains_timestamp(500));
    }

    #[test]
    fn test_block_parse() {
        let json = r#"{
            "event": 1234,
            "end_event": 5678,
            "length": 900000,
            "url": "https://apps.radioparadise.com/blocks/chan/0/4/1234-5678.flac",
            "image_base": "https://img.radioparadise.com/covers/l/",
            "song": {
                "0": {
                    "artist": "Miles Davis",
                    "title": "So What",
                    "album": "Kind of Blue",
                    "year": 1959,
                    "elapsed": 0,
                    "duration": 540000,
                    "cover": "B00000I0JF.jpg"
                },
                "1": {
                    "artist": "John Coltrane",
                    "title": "Giant Steps",
                    "album": "Giant Steps",
                    "year": 1960,
                    "elapsed": 540000,
                    "duration": 360000,
                    "cover": "B000002I4U.jpg"
                }
            }
        }"#;

        let block: Block = serde_json::from_str(json).unwrap();
        assert_eq!(block.event, 1234);
        assert_eq!(block.end_event, 5678);
        assert_eq!(block.song_count(), 2);

        let songs = block.songs_ordered();
        assert_eq!(songs.len(), 2);
        assert_eq!(songs[0].1.title, "So What");
        assert_eq!(songs[1].1.title, "Giant Steps");

        let (start, end) = block.parse_url_events().unwrap();
        assert_eq!(start, 1234);
        assert_eq!(end, 5678);

        let (idx, song) = block.song_at_timestamp(600000).unwrap();
        assert_eq!(idx, 1);
        assert_eq!(song.title, "Giant Steps");
    }
}
