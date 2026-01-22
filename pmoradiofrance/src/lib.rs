//! Radio France client library for PMOMusic
//!
//! This crate provides a Rust client for accessing Radio France's public APIs,
//! including live metadata, station discovery, and stream URLs.
//!
//! # Features
//!
//! - **Station Discovery**: Discover all Radio France stations dynamically
//!   (main stations, webradios, and local France Bleu radios)
//! - **Live Metadata**: Get current show information, producers, visuals
//! - **Stream URLs**: Get HiFi stream URLs (AAC 192 kbps, HLS)
//! - **Polling Support**: Intelligent refresh delay based on API recommendations
//! - **Configuration Extension**: Cache station lists with configurable TTL
//!
//! # Supported Stations
//!
//! - **Main Stations**: France Inter, France Info, France Culture, France Musique,
//!   FIP, Mouv', France Bleu
//! - **Webradios**: FIP Rock, FIP Jazz, France Musique Baroque, etc.
//! - **Local Radios**: ~40 France Bleu local stations
//!
//! # Example
//!
//! ```no_run
//! use pmoradiofrance::{RadioFranceClient, ImageSize};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let client = RadioFranceClient::new().await?;
//!
//!     // Discover all stations
//!     let stations = client.discover_all_stations().await?;
//!     println!("Found {} stations", stations.len());
//!
//!     // Get live metadata
//!     let live = client.live_metadata("franceculture").await?;
//!     println!("Now: {} - {}",
//!         live.now.first_line.title_or_default(),
//!         live.now.second_line.title_or_default()
//!     );
//!
//!     // Get HiFi stream URL
//!     let stream_url = client.get_hifi_stream_url("franceculture").await?;
//!     println!("Stream: {}", stream_url);
//!
//!     Ok(())
//! }
//! ```
//!
//! # Configuration Extension
//!
//! When the `pmoconfig` feature is enabled, this crate provides a configuration
//! extension trait for caching station lists:
//!
//! ```no_run
//! use pmoconfig::get_config;
//! use pmoradiofrance::{RadioFranceConfigExt, RadioFranceClient};
//!
//! # #[tokio::main]
//! # async fn main() -> anyhow::Result<()> {
//! let config = get_config();
//!
//! // Check cached stations (default TTL: 7 days)
//! if let Some(stations) = config.get_radiofrance_stations_cached()? {
//!     println!("Using {} cached stations", stations.len());
//! } else {
//!     // Cache miss - need to discover
//!     let client = RadioFranceClient::new().await?;
//!     let stations = client.discover_all_stations().await?;
//!     config.set_radiofrance_cached_stations(&stations)?;
//! }
//! # Ok(())
//! # }
//! ```
//!
//! # API Rate Limiting
//!
//! Radio France's APIs don't have documented rate limits, but the `delayToRefresh`
//! field in responses indicates the recommended polling interval. Always use
//! `RadioFranceClient::next_refresh_delay()` to respect this.
//!
//! # Audio Quality
//!
//! This client focuses on HiFi quality only:
//! - **AAC 192 kbps**: Primary format (best quality)
//! - **HLS**: Adaptive streaming fallback
//!
//! Lower quality formats (lofi, midfi) are not prioritized but are available
//! in the `StreamSource` list if needed.

pub mod client;
pub mod error;
pub mod models;

#[cfg(feature = "pmoconfig")]
pub mod config_ext;

#[cfg(feature = "pmoconfig")]
pub mod stateful_client;

#[cfg(feature = "playlist")]
pub mod playlist;

// Re-exports
pub use client::{ClientBuilder, RadioFranceClient};
pub use error::{Error, Result};
pub use models::{
    BroadcastType, CachedStationList, EmbedImage, ImageSize, Line, LiveResponse, LocalRadio, Media,
    Release, ShowMetadata, Song, Station, StationType, StreamFormat, StreamSource, Visuals,
};

#[cfg(feature = "pmoconfig")]
pub use config_ext::RadioFranceConfigExt;

#[cfg(feature = "pmoconfig")]
pub use stateful_client::RadioFranceStatefulClient;

#[cfg(feature = "playlist")]
pub use playlist::{StationGroup, StationGroups, StationPlaylist};
