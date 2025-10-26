//! # pmoparadise - Radio Paradise Client for Rust
//!
//! `pmoparadise` is an idiomatic Rust client library for accessing Radio Paradise's
//! streaming API. It provides metadata retrieval, block streaming, and optional
//! per-track extraction from FLAC blocks.
//!
//! ## Features
//!
//! - **Metadata Access**: Get current and historical block metadata with song information
//! - **Block Streaming**: Stream continuous FLAC blocks with automatic prefetching
//! - **FLAC Quality**: Lossless CD quality or better
//! - **Per-Track Extraction** (optional): Extract individual tracks from FLAC blocks
//! - **Async/Await**: Built on tokio for efficient async I/O
//! - **Type-Safe**: Strongly typed API with comprehensive error handling
//!
//! ## Quick Start
//!
//! ```no_run
//! use pmoparadise::RadioParadiseClient;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Create a client
//!     let client = RadioParadiseClient::new().await?;
//!
//!     // Get what's currently playing
//!     let now_playing = client.now_playing().await?;
//!
//!     if let Some(song) = &now_playing.current_song {
//!         println!("Now Playing: {} - {}", song.artist, song.title);
//!         if let Some(album) = &song.album {
//!             println!("Album: {}", album);
//!         }
//!     }
//!
//!     // Get all songs in the current block
//!     for (index, song) in now_playing.block.songs_ordered() {
//!         println!("  {}. {} - {} ({}s)",
//!                  index,
//!                  song.artist,
//!                  song.title,
//!                  song.duration / 1000);
//!     }
//!
//!     Ok(())
//! }
//! ```
//!
//! ## Streaming Blocks
//!
//! Radio Paradise broadcasts music in continuous "blocks" - each block is a single
//! FLAC file containing multiple songs with metadata indicating timing offsets.
//!
//! ```no_run
//! use pmoparadise::RadioParadiseClient;
//! use futures::StreamExt;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let client = RadioParadiseClient::new().await?;
//!     let block = client.get_block(None).await?;
//!
//!     // Stream the block
//!     let mut stream = client.stream_block_from_metadata(&block).await?;
//!
//!     while let Some(chunk) = stream.next().await {
//!         let bytes = chunk?;
//!         // Feed to audio player, write to file, etc.
//!     }
//!
//!     Ok(())
//! }
//! ```
//!
//! ## Per-Track Extraction (Feature: `per-track`)
//!
//! **Important**: This is an advanced feature with significant tradeoffs.
//! See the [`track`] module documentation for details.
//!
//! Most applications should stream blocks and use player-based seeking instead.
//!
//! ```no_run
//! # #[cfg(feature = "per-track")]
//! # {
//! use pmoparadise::RadioParadiseClient;
//! use std::path::Path;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let client = RadioParadiseClient::new().await?;
//!     let block = client.get_block(None).await?;
//!
//!     // Extract first track to WAV
//!     let mut track = client.open_track_stream(&block, 0).await?;
//!     track.export_wav(Path::new("track.wav"))?;
//!
//!     // Or get position for player-based seeking (recommended)
//!     let (start, duration) = client.track_position_seconds(&block, 0)?;
//!     println!("Play with: mpv --start={} --length={} {}", start, duration, block.url);
//!
//!     Ok(())
//! }
//! # }
//! ```
//!
//! ## Architecture
//!
//! The API is organized into several modules:
//!
//! - [`client`]: Main HTTP client for API access
//! - [`models`]: Data structures for blocks, songs, and metadata
//! - [`stream`]: Block streaming functionality
//! - [`track`]: Per-track extraction (feature-gated)
//! - [`error`]: Error types and result aliases
//!
//! ## Radio Paradise Block Format
//!
//! Radio Paradise streams use a block-based format:
//!
//! - Each block is a single FLAC audio file
//! - Blocks contain multiple songs (typically 10-15 minutes total)
//! - Metadata includes timing offsets (`song[i].elapsed` in ms) for each song
//! - Block URLs follow the pattern: `https://apps.radioparadise.com/blocks/chan/0/4/<start>-<end>.flac`
//! - The `end_event` of one block is the `event` of the next, enabling seamless transitions
//!
//! ## Best Practices
//!
//! ### For Continuous Playback
//!
//! 1. Get current block with `get_block(None)`
//! 2. Stream block with `stream_block_from_metadata()`
//! 3. Use `prefetch_next()` to prepare the next block
//! 4. When current block ends, stream the next block seamlessly
//!
//! ### For Per-Song Seeking
//!
//! **Recommended approach** (efficient):
//! ```bash
//! # Use your audio player's seek capability
//! mpv --start=123.5 --length=234.0 <block_url>
//! ```
//!
//! **Alternative** (resource-intensive, requires `per-track` feature):
//! - Download and decode block
//! - Extract specific track to PCM/WAV
//!
//! ## Error Handling
//!
//! All operations return `Result<T, Error>` with detailed error types:
//!
//! ```no_run
//! use pmoparadise::{RadioParadiseClient, Error};
//!
//! #[tokio::main]
//! async fn main() {
//!     let client = RadioParadiseClient::new().await.unwrap();
//!
//!     match client.get_block(Some(99999999)).await {
//!         Ok(block) => println!("Got block: {}", block.event),
//!         Err(Error::Http(e)) => eprintln!("Network error: {}", e),
//!         Err(Error::Json(e)) => eprintln!("Parse error: {}", e),
//!         Err(e) => eprintln!("Other error: {}", e),
//!     }
//! }
//! ```
//!
//! ## Caching Support (Feature: `cache`)
//!
//! `pmoparadise` can optionally integrate with `pmocovers` and `pmoaudiocache` to cache
//! cover images and audio tracks locally:
//!
//! ```no_run
//! # #[cfg(feature = "cache")]
//! # {
//! use pmoparadise::{RadioParadiseClient, RadioParadiseSource};
//! use std::sync::Arc;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Create caches
//!     let cover_cache = Arc::new(pmocovers::cache::new_cache("./cache/covers", 500)?);
//!     let audio_cache = Arc::new(pmoaudiocache::cache::new_cache("./cache/audio", 100)?);
//!
//!     // Create client and source with caching
//!     let client = RadioParadiseClient::new().await?;
//!     let source = RadioParadiseSource::new(
//!         client,
//!         50,
//!         cover_cache,
//!         audio_cache,
//!     );
//!
//!     println!("Source ready: {}", source.name());
//!
//!     Ok(())
//! }
//! # }
//! ```
//!
//! **Benefits**:
//! - Cover images are automatically downloaded and converted to WebP
//! - Audio tracks are cached as FLAC with metadata preserved
//! - Subsequent access is instant (no re-download)
//! - URIs returned by `resolve_uri()` point to cached versions
//!
//! See the `with_cache` example for a complete demonstration.
//!
//! ## Cargo Features
//!
//! - `default = ["metadata-only"]`: Standard metadata and streaming (no FLAC decoding)
//! - `per-track`: Enable FLAC decoding and per-track extraction (adds `claxon`, `hound`, `tempfile`)
//! - `logging`: Enable tracing logs for debugging
//! - `mediaserver`: Enable UPnP/DLNA Media Server (adds `pmoupnp`, `pmoserver`, `pmodidl`)
//! - `cache`: Enable cover and audio caching support (adds `pmocovers`, `pmoaudiocache`, enables `logging`)
//!
//! ## See Also
//!
//! - [Radio Paradise](https://radioparadise.com) - Official website
//! - [Radio Paradise API](https://api.radioparadise.com) - API documentation

pub mod client;
pub mod error;
pub mod models;
pub mod paradise;
pub mod source;
pub mod stream;
pub mod streaming;

#[cfg(feature = "per-track")]
pub mod track;

#[cfg(feature = "mediaserver")]
pub mod mediaserver;

#[cfg(feature = "pmoserver")]
pub mod pmoserver_ext;

// Re-exports for convenience
pub use client::{ClientBuilder, RadioParadiseClient};
pub use error::{Error, Result};
pub use models::{Block, DurationMs, EventId, NowPlaying, Song};
pub use source::RadioParadiseSource;
pub use stream::BlockStream;

#[cfg(feature = "per-track")]
pub use track::{TrackMetadata, TrackStream};

#[cfg(feature = "mediaserver")]
pub use mediaserver::{MediaServerBuilder, RadioParadiseMediaServer};

#[cfg(feature = "pmoserver")]
pub use pmoserver_ext::{
    create_api_router, RadioParadiseApiDoc, RadioParadiseExt, RadioParadiseState,
};

// Version information
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version() {
        assert!(!VERSION.is_empty());
    }
}
