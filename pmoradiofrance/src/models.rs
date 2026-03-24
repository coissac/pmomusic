//! Data models for Radio France API responses
//!
//! This module contains all the structures needed to deserialize
//! responses from Radio France's public APIs.

use serde::{Deserialize, Serialize};

// ============================================================================
// Station Discovery Models
// ============================================================================

/// A discovered Radio France station
///
/// Simplifié: juste slug + name, plus de distinction de type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Station {
    /// Unique slug identifier (e.g., "franceculture", "fip_rock")
    pub slug: String,
    /// Human-readable name (e.g., "France Culture", "FIP Rock")
    pub name: String,
}

impl Station {
    /// Create a new station
    pub fn new(slug: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            slug: slug.into(),
            name: name.into(),
        }
    }
}

// ============================================================================
// New livemeta/pull API Models (api.radiofrance.fr)
// ============================================================================

/// Response from https://api.radiofrance.fr/livemeta/pull/{stationId}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PullResponse {
    /// Numeric station ID
    pub station_id: u32,
    /// Map of stepId -> step metadata
    pub steps: std::collections::HashMap<String, PullStep>,
    /// Ordered levels (depth 1 = current show/song)
    pub levels: Vec<PullLevel>,
}

impl PullResponse {
    /// Get the current step at depth 1 (the "now playing" item)
    pub fn current_step(&self) -> Option<&PullStep> {
        // levels[0].items[0] is the current item at depth 1 (most relevant)
        let level = self.levels.first()?;
        let step_id = level.items.first()?;
        self.steps.get(step_id)
    }
}

/// A level in the livemeta response (groups steps by depth)
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PullLevel {
    /// Ordered list of step IDs at this level
    pub items: Vec<String>,
    /// Depth (1 = show/song, 2 = sub-item, 3 = deeper)
    pub position: u32,
}

/// A step (show, episode, or song) from the livemeta pull API
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PullStep {
    pub uuid: String,
    pub step_id: String,
    pub title: String,
    pub start: Option<u64>,
    pub end: Option<u64>,
    pub station_id: u32,
    pub embed_type: Option<String>, // "song", "expression", "concept"
    pub depth: u32,
    pub disc_jockey: Option<String>,
    /// For songs: authors
    #[serde(default)]
    pub authors: serde_json::Value, // can be String or Vec<String>
    #[serde(default)]
    pub performers: Option<String>,
    #[serde(default)]
    pub highlighted_artists: Vec<String>,
    pub song_id: Option<String>,
    pub titre_album: Option<String>,
    pub label: Option<String>,
    pub annee_edition_musique: Option<u32>,
    pub cover_uuid: Option<String>,
    /// Direct UUID for visual (new API uses UUID directly, not URL)
    pub visual: Option<String>,
    /// For shows: concept title (e.g. "La Science, CQFD")
    pub title_concept: Option<String>,
    /// For shows: producers list
    #[serde(default)]
    pub producers: Vec<PullProducer>,
    pub path: Option<String>,
    pub expression_description: Option<String>,
    pub description: Option<String>,
}

impl PullStep {
    /// Returns true if this step is a music track
    pub fn is_song(&self) -> bool {
        self.embed_type.as_deref() == Some("song")
    }

    /// Get artist display string
    pub fn artists_display(&self) -> String {
        if !self.highlighted_artists.is_empty() {
            return self.highlighted_artists.join(", ");
        }
        match &self.authors {
            serde_json::Value::String(s) => s.clone(),
            serde_json::Value::Array(arr) => arr
                .iter()
                .filter_map(|v| v.as_str())
                .collect::<Vec<_>>()
                .join(", "),
            _ => String::new(),
        }
    }
}

/// A producer in a PullStep
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PullProducer {
    pub uuid: String,
    pub name: String,
}

// ============================================================================
// Live API Response Models (internal representation)
// ============================================================================

/// Internal live metadata representation
/// Built from PullResponse (new API) or used directly in tests
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveResponse {
    /// Station slug
    pub station_name: String,
    /// Recommended delay before next refresh (milliseconds)
    pub delay_to_refresh: u64,
    /// Whether station has been migrated to new system
    #[serde(default)]
    pub migrated: bool,
    /// Current show/track metadata
    pub now: ShowMetadata,
    /// Next show/track metadata (if available)
    pub next: Option<ShowMetadata>,
}

impl LiveResponse {
    /// Get local radios (France Bleu only) - convenience accessor
    pub fn local_radios(&self) -> Option<&Vec<LocalRadio>> {
        self.now.local_radios.as_ref()
    }
}

/// Metadata for a show or track currently playing
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShowMetadata {
    /// Whether to display music program info
    #[serde(default)]
    pub print_prog_music: bool,
    /// Start time (Unix timestamp)
    pub start_time: Option<u64>,
    /// End time (Unix timestamp)
    pub end_time: Option<u64>,
    /// Producer name
    pub producer: Option<String>,
    /// First line (usually show title)
    #[serde(default)]
    pub first_line: Line,
    /// Second line (usually episode/track title)
    #[serde(default)]
    pub second_line: Line,
    /// Third line (optional subtitle)
    pub third_line: Option<Line>,
    /// Show description/intro
    pub intro: Option<String>,
    /// React availability flag
    #[serde(default)]
    pub react_available: bool,
    /// Background visual
    pub visual_background: Option<EmbedImage>,
    /// Song info (for music stations like FIP, France Musique)
    pub song: Option<Song>,
    /// Available media streams
    #[serde(default)]
    pub media: Media,
    /// Visual assets (card, player)
    pub visuals: Option<Visuals>,
    /// Local radios list (France Bleu only)
    #[serde(default)]
    pub local_radios: Option<Vec<LocalRadio>>,
}

/// A line of text with optional link
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Line {
    /// Text content
    pub title: Option<String>,
    /// UUID of the referenced object
    pub id: Option<String>,
    /// URL path to the referenced page
    pub path: Option<String>,
}

impl Line {
    /// Get the title or an empty string
    pub fn title_or_default(&self) -> &str {
        self.title.as_deref().unwrap_or("")
    }
}

/// Song information (for music stations)
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Song {
    /// Song UUID
    pub id: String,
    /// Release year
    pub year: Option<u32>,
    /// Artist names
    #[serde(default)]
    pub interpreters: Vec<String>,
    /// Album/release information
    #[serde(default)]
    pub release: Release,
}

impl Song {
    /// Get artists as a comma-separated string
    pub fn artists_display(&self) -> String {
        self.interpreters.join(", ")
    }
}

/// Album/release information
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Release {
    /// Record label
    pub label: Option<String>,
    /// Album title
    pub title: Option<String>,
    /// Catalog reference
    pub reference: Option<String>,
}

/// Available media streams
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Media {
    /// List of available stream sources
    #[serde(default)]
    pub sources: Vec<StreamSource>,
}

impl Media {
    /// Find the best HiFi stream (AAC ou MP3, bitrate maximum, jamais HLS)
    pub fn best_hifi_stream(&self) -> Option<&StreamSource> {
        // Priority: AAC 192 kbps > AAC autre bitrate > MP3 bitrate max > autre
        // JAMAIS HLS (incompatible avec beaucoup de lecteurs)
        self.sources
            .iter()
            .filter(|s| s.broadcast_type == BroadcastType::Live && s.format != StreamFormat::Hls)
            .max_by_key(|s| {
                // Priorité: format puis bitrate
                let format_priority = match s.format {
                    StreamFormat::Aac => 1000,
                    StreamFormat::Mp3 => 500,
                    StreamFormat::Hls => 0, // Filtré de toute façon
                };
                format_priority + s.bitrate
            })
    }

    /// Find a stream by format and broadcast type
    pub fn find_stream(
        &self,
        format: StreamFormat,
        broadcast_type: BroadcastType,
    ) -> Option<&StreamSource> {
        self.sources
            .iter()
            .find(|s| s.format == format && s.broadcast_type == broadcast_type)
    }

    /// Get all live streams
    pub fn live_streams(&self) -> impl Iterator<Item = &StreamSource> {
        self.sources
            .iter()
            .filter(|s| s.broadcast_type == BroadcastType::Live)
    }
}

/// A stream source with URL and format info
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamSource {
    /// Stream URL
    pub url: String,
    /// Broadcast type (live or timeshift)
    pub broadcast_type: BroadcastType,
    /// Stream format
    pub format: StreamFormat,
    /// Bitrate in kbps (0 for HLS adaptive)
    pub bitrate: u32,
}

/// Type of broadcast
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum BroadcastType {
    /// Live stream
    Live,
    /// Timeshift (replay) stream
    Timeshift,
}

/// Stream format
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum StreamFormat {
    /// MP3 format
    Mp3,
    /// AAC format
    Aac,
    /// HLS adaptive streaming
    Hls,
}

impl StreamFormat {
    /// Get the MIME type for this format
    pub fn mime_type(&self) -> &'static str {
        match self {
            StreamFormat::Mp3 => "audio/mpeg",
            StreamFormat::Aac => "audio/aac",
            StreamFormat::Hls => "application/vnd.apple.mpegurl",
        }
    }
}

/// An embedded image
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EmbedImage {
    /// Model type (usually "EmbedImage")
    #[serde(default)]
    pub model: String,
    /// Image URL or path
    pub src: String,
    /// Image width
    pub width: Option<u32>,
    /// Image height
    pub height: Option<u32>,
    /// Dominant color (hex)
    pub dominant: Option<String>,
    /// Copyright notice
    pub copyright: Option<String>,
}

impl EmbedImage {
    /// Extract the UUID from the image URL
    ///
    /// Pikapi URLs are in format: https://www.radiofrance.fr/pikapi/images/{uuid}[/size]
    pub fn extract_uuid(&self) -> Option<String> {
        let re = regex::Regex::new(r"/pikapi/images/([a-f0-9-]+)").ok()?;
        re.captures(&self.src)
            .and_then(|cap| cap.get(1))
            .map(|m| m.as_str().to_string())
    }
}

/// Visual assets for different display contexts
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Visuals {
    /// Card-sized image
    pub card: Option<EmbedImage>,
    /// Player-sized image
    pub player: Option<EmbedImage>,
}

/// A local France Bleu radio station
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalRadio {
    /// Internal ID
    pub id: u32,
    /// Display title (e.g., "ICI Alsace")
    pub title: String,
    /// Technical name (e.g., "francebleu_alsace")
    pub name: String,
    /// Whether the station is currently on air
    #[serde(default)]
    pub is_on_air: bool,
}

// ============================================================================
// Image Size Helpers
// ============================================================================

/// Available image sizes from Pikapi
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageSize {
    /// 88x88 pixels
    Tiny,
    /// 200x200 pixels
    Small,
    /// 420x720 pixels (portrait)
    Medium,
    /// 560x960 pixels (portrait)
    Large,
    /// 1200x680 pixels (landscape)
    XLarge,
    /// Original size
    Raw,
}

impl ImageSize {
    /// Get the size string for Pikapi URLs
    pub fn as_str(&self) -> &'static str {
        match self {
            ImageSize::Tiny => "88x88",
            ImageSize::Small => "200x200",
            ImageSize::Medium => "420x720",
            ImageSize::Large => "560x960",
            ImageSize::XLarge => "1200x680",
            ImageSize::Raw => "raw",
        }
    }

    /// Build a Pikapi image URL
    pub fn build_url(&self, uuid: &str) -> String {
        format!(
            "https://www.radiofrance.fr/pikapi/images/{}/{}",
            uuid,
            self.as_str()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_station_creation() {
        let station = Station::new("franceculture", "France Culture");
        assert_eq!(station.slug, "franceculture");
        assert_eq!(station.name, "France Culture");
    }

    #[test]
    fn test_image_size() {
        let uuid = "436430f7-5b2b-43f2-9f3c-28f2ad6cae39";
        let url = ImageSize::Small.build_url(uuid);
        assert_eq!(
            url,
            "https://www.radiofrance.fr/pikapi/images/436430f7-5b2b-43f2-9f3c-28f2ad6cae39/200x200"
        );
    }
}
