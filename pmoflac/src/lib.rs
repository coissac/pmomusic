//! # pmoflac
//!
//! Asynchronous audio encoding and decoding library for Rust.
//!
//! This library provides streaming FLAC and MP3 decoding, as well as FLAC encoding,
//! with a Tokio-based async API. The key feature is **true streaming**: data is
//! processed incrementally without buffering entire files in memory.
//!
//! ## Features
//!
//! - **MP3 decoding**: Stream MP3 files to PCM data
//! - **FLAC encoding/decoding**: Bidirectional FLAC ↔ PCM conversion
//! - **Async streaming API**: Built on Tokio's `AsyncRead` trait
//! - **Low memory footprint**: Processes data in chunks, not entire files
//! - **Zero-copy where possible**: Efficient buffer management
//! - **Thread-safe**: Uses channels for inter-task communication
//! - **Composable**: Chain decoders and encoders (e.g., MP3 → PCM → FLAC)
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
//!
//! ## Example: Transcode MP3 to FLAC
//!
//! ```no_run
//! use pmoflac::{decode_mp3_stream, encode_flac_stream, PcmFormat, EncoderOptions};
//! use tokio::fs::File;
//! use tokio::io;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Decode MP3 to PCM stream
//!     let mp3_file = File::open("input.mp3").await?;
//!     let mp3_stream = decode_mp3_stream(mp3_file).await?;
//!     let (info, pcm_reader) = mp3_stream.into_parts();
//!
//!     // Encode PCM stream to FLAC
//!     let format = PcmFormat {
//!         sample_rate: info.sample_rate,
//!         channels: info.channels,
//!         bits_per_sample: info.bits_per_sample,
//!     };
//!     let mut flac_stream = encode_flac_stream(pcm_reader, format, EncoderOptions::default()).await?;
//!
//!     // Write to output file
//!     let mut output = File::create("output.flac").await?;
//!     io::copy(&mut flac_stream, &mut output).await?;
//!     flac_stream.wait().await?;
//!
//!     Ok(())
//! }
//! ```

pub mod aiff;
pub mod autodetect;
mod common;
pub mod decoder;
mod decoder_common;
pub mod encoder;
pub mod error;
pub mod mp3;
pub mod ogg;
mod ogg_common;
pub mod opus;
mod pcm;
mod stream;
pub mod transcode;
mod util;
pub mod wav;

pub use aiff::{decode_aiff_stream, AiffDecodedStream, AiffError};
pub use autodetect::{decode_audio_stream, DecodeAudioError, DecodedAudioStream, DecodedReader};
pub use decoder::{decode_flac_stream, FlacDecodedStream};
pub use encoder::{encode_flac_stream, EncoderOptions, FlacEncodedStream};
pub use error::FlacError;
pub use mp3::{decode_mp3_stream, Mp3DecodedStream, Mp3Error};
pub use ogg::{decode_ogg_vorbis_stream, OggDecodedStream, OggError};
pub use opus::{decode_ogg_opus_stream, OggOpusDecodedStream, OggOpusError};
pub use pcm::{PcmFormat, StreamInfo};
pub use transcode::{
    transcode_to_flac_stream, AudioCodec, FlacTranscodeStream, TranscodeError, TranscodeOptions,
    TranscodeToFlac,
};
pub use wav::{decode_wav_stream, WavDecodedStream, WavError};
