//! UPnP Media Server for Radio Paradise
//!
//! This module provides a UPnP/DLNA Media Server implementation that exposes
//! Radio Paradise blocks and songs as a browsable media library.
//!
//! # Features
//!
//! - ContentDirectory service for browsing blocks and songs
//! - ConnectionManager service for protocol info
//! - DIDL-Lite metadata for songs
//! - Support for multiple quality levels
//! - Live streaming URLs
//!
//! # Architecture
//!
//! ```text
//! RadioParadiseMediaServer
//!   └── Device (urn:schemas-upnp-org:device:MediaServer:1)
//!       ├── ContentDirectory service
//!       │   ├── Browse action
//!       │   ├── Search action (optional)
//!       │   └── GetSearchCapabilities
//!       └── ConnectionManager service
//!           ├── GetProtocolInfo
//!           └── GetCurrentConnectionIDs
//! ```
//!
//! # Example
//!
//! ```no_run
//! # #[cfg(feature = "mediaserver")]
//! # {
//! use pmoparadise::mediaserver::RadioParadiseMediaServer;
//! use pmoparadise::Bitrate;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let server = RadioParadiseMediaServer::new()
//!         .with_bitrate(Bitrate::Flac)
//!         .with_friendly_name("Radio Paradise FLAC")
//!         .build()
//!         .await?;
//!
//!     server.run().await?;
//!     Ok(())
//! }
//! # }
//! ```

#[cfg(feature = "mediaserver")]
mod connection_manager;
#[cfg(feature = "mediaserver")]
mod content_directory;
#[cfg(feature = "mediaserver")]
mod server;

#[cfg(feature = "mediaserver")]
pub use server::{MediaServerBuilder, RadioParadiseMediaServer};
