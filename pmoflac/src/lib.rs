//! # pmoflac
//!
//! Asynchronous FLAC encoding and decoding library for Rust.
//!
//! This library provides streaming FLAC encoding and decoding with a Tokio-based async API.
//! The key feature is **true streaming**: data is processed incrementally without buffering
//! entire files in memory.
//!
//! ## Features
//!
//! - **Async streaming API**: Built on Tokio's `AsyncRead` trait
//! - **Low memory footprint**: Processes data in chunks, not entire files
//! - **Zero-copy where possible**: Efficient buffer management
//! - **Thread-safe**: Uses channels for inter-task communication
//!
//! ## Example: Decode FLAC to PCM
//!
//! ```no_run
//! use pmoflac::decode_flac_stream;
//! use tokio::fs::File;
//! use tokio::io::AsyncReadExt;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let file = File::open("audio.flac").await?;
//!     let mut stream = decode_flac_stream(file).await?;
//!
//!     let info = stream.info();
//!     println!("Sample rate: {} Hz", info.sample_rate);
//!     println!("Channels: {}", info.channels);
//!
//!     let mut pcm_data = Vec::new();
//!     stream.read_to_end(&mut pcm_data).await?;
//!     stream.wait().await?;
//!
//!     Ok(())
//! }
//! ```
//!
//! ## Example: Encode PCM to FLAC
//!
//! ```no_run
//! use pmoflac::{encode_flac_stream, EncoderOptions, PcmFormat};
//! use tokio::io::AsyncReadExt;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let pcm_data: &[u8] = &[/* PCM samples */];
//!     let format = PcmFormat {
//!         sample_rate: 44_100,
//!         channels: 2,
//!         bits_per_sample: 16,
//!     };
//!
//!     let mut stream = encode_flac_stream(pcm_data, format, EncoderOptions::default()).await?;
//!     let mut flac_data = Vec::new();
//!     stream.read_to_end(&mut flac_data).await?;
//!     stream.wait().await?;
//!
//!     Ok(())
//! }
//! ```

pub mod decoder;
pub mod encoder;
pub mod error;
mod pcm;
mod stream;
mod util;

pub use decoder::{decode_flac_stream, FlacDecodedStream};
pub use encoder::{encode_flac_stream, EncoderOptions, FlacEncodedStream};
pub use error::FlacError;
pub use pcm::{PcmFormat, StreamInfo};
