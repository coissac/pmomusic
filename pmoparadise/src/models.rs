//! Data models for Radio Paradise API responses

use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Number;
use std::collections::HashMap;
use url::Url;

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
        Number(Number),
    }

    fn to_milliseconds(value: f64) -> u64 {
        if value >= 100_000.0 {
            value.round() as u64
        } else {
            (value * 1000.0).round() as u64
        }
    }

    match StringOrNumber::deserialize(deserializer)? {
        StringOrNumber::String(s) => {
            let value = s.parse::<f64>().map_err(D::Error::custom)?;
            Ok(to_milliseconds(value))
        }
        StringOrNumber::Number(n) => {
            if let Some(int_value) = n.as_u64() {
                Ok(to_milliseconds(int_value as f64))
            } else if let Some(float_value) = n.as_f64() {
                Ok(to_milliseconds(float_value))
            } else {
                Err(D::Error::custom("Invalid number for block length"))
            }
        }
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

    /// Gapless URL for individual song FLAC
    /// This URL points to a FLAC file containing only this song
    #[serde(default)]
    pub gapless_url: Option<String>,

    /// Scheduled playback time on Radio Paradise (Unix timestamp in milliseconds, UTC)
    #[serde(default)]
    pub sched_time_millis: Option<u64>,

    /// Radio Paradise song ID (unique identifier)
    #[serde(default)]
    pub song_id: Option<String>,

    /// Radio Paradise artist ID (for building artist URLs)
    #[serde(default)]
    pub artist_id: Option<String>,

    /// Large cover image path (best quality)
    #[serde(default)]
    pub cover_large: Option<String>,

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

    /// Calcule le timestamp de fin de diffusion (sched_time + duration)
    pub fn sched_end_time_ms(&self) -> Option<u64> {
        self.sched_time_millis.map(|start| start + self.duration)
    }

    /// Vérifie si la chanson est encore en lecture ou à venir
    pub fn is_still_playing(&self, now_ms: u64) -> bool {
        self.sched_end_time_ms()
            .map(|end| end >= now_ms)
            .unwrap_or(false)
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

    /// Scheduled start time for this block (Unix timestamp in milliseconds, UTC)
    #[serde(default)]
    pub sched_time_millis: Option<u64>,

    /// Map of song index (as string) to Song metadata
    /// Keys are "0", "1", "2", etc.
    #[serde(default)]
    pub song: HashMap<String, Song>,

    /// Additional metadata
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

impl Block {
    /// Scheduled start time in milliseconds if available.
    pub fn start_time_millis(&self) -> Option<u64> {
        if let Some(ts) = self.sched_time_millis {
            return Some(ts);
        }
        self.songs_ordered()
            .into_iter()
            .find_map(|(_, song)| song.sched_time_millis)
    }

    /// Get songs in order by index
    pub fn songs_ordered(&self) -> Vec<(usize, &Song)> {
        let mut songs: Vec<_> = self
            .song
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
        let base = self.image_base.as_ref()?;
        let base_url = Url::parse(base).ok()?;
        base_url.join(cover_path).ok().map(|url| url.to_string())
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
        let (current_song_index, current_song) = block
            .get_song(0)
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
            gapless_url: todo!(),
            sched_time_millis: todo!(),
            song_id: todo!(),
            artist_id: todo!(),
            cover_large: todo!(),
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

    #[test]
    fn test_block_length_from_seconds_string() {
        let json = serde_json::json!({
            "event": 1,
            "end_event": 2,
            "length": "1715.54",
            "url": "https://example.com/block.flac",
            "song": {}
        });

        let block: Block = serde_json::from_value(json).unwrap();
        assert_eq!(block.length, 1_715_540);
    }

    #[test]
    fn test_block_length_from_seconds_integer() {
        let json = serde_json::json!({
            "event": 1,
            "end_event": 2,
            "length": 1800,
            "url": "https://example.com/block.flac",
            "song": {}
        });

        let block: Block = serde_json::from_value(json).unwrap();
        assert_eq!(block.length, 1_800_000);
    }

    #[test]
    fn test_block_length_from_milliseconds_integer() {
        let json = serde_json::json!({
            "event": 1,
            "end_event": 2,
            "length": 900_000,
            "url": "https://example.com/block.flac",
            "song": {}
        });

        let block: Block = serde_json::from_value(json).unwrap();
        assert_eq!(block.length, 900_000);
    }

    #[test]
    fn test_block_length_from_milliseconds_float() {
        let json = serde_json::json!({
            "event": 1,
            "end_event": 2,
            "length": 900_000.0,
            "url": "https://example.com/block.flac",
            "song": {}
        });

        let block: Block = serde_json::from_value(json).unwrap();
        assert_eq!(block.length, 900_000);
    }
}
