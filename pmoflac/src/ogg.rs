//! # Ogg/Vorbis Streaming Decoder
//!
//! This module provides asynchronous streaming Ogg/Vorbis decoding capabilities.
//! It decodes Ogg/Vorbis audio streams into PCM data (16-bit little-endian interleaved),
//! which can then be fed directly into the FLAC encoder for transcoding.
//!
//! ## Key Features
//!
//! - **100% streaming**: No seek operations required, works with non-seekable streams
//! - **Manual Ogg parsing**: Custom implementation that only requires `Read` trait
//! - **CRC32 validation**: Optional integrity checking of Ogg pages
//! - **Automatic sync**: Searches for "OggS" magic pattern, handles garbage bytes
//! - **Low-level Vorbis decoding**: Uses lewton's audio API directly
//!
//! ## Architecture
//!
//! The decoder uses a multi-task pipeline for efficient streaming:
//!
//! ```text
//! Ogg Input → [Ingest Task] → [Decode Task] → [Writer Task] → PCM Output (AsyncRead)
//!                  ↓              ↓                ↓
//!              mpsc channel   blocking I/O    duplex stream
//! ```
//!
//! - **Ingest Task**: Reads Ogg data in chunks and sends it through a channel
//! - **Decode Task**: Parses Ogg pages, assembles packets, decodes Vorbis audio
//! - **Writer Task**: Writes decoded PCM data to a duplex stream
//!
//! This architecture ensures:
//! - True streaming with minimal memory footprint
//! - Non-blocking async I/O for the consumer
//! - Proper backpressure through bounded channels
//!
//! ## Limitations
//!
//! - Only single logical bitstream is supported (chained streams are rejected)
//! - Ogg Vorbis only (not Opus, FLAC, or other Ogg-encapsulated formats)
//! - CRC checking increases CPU usage slightly
//!
//! ## Example: Basic Ogg/Vorbis Decoding
//!
//! ```no_run
//! use pmoflac::decode_ogg_vorbis_stream;
//! use tokio::fs::File;
//! use tokio::io::AsyncReadExt;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let file = File::open("audio.ogg").await?;
//!     let mut stream = decode_ogg_vorbis_stream(file).await?;
//!
//!     // Get stream information
//!     let info = stream.info();
//!     println!("Sample rate: {} Hz", info.sample_rate);
//!     println!("Channels: {}", info.channels);
//!     println!("Bits per sample: {}", info.bits_per_sample);
//!
//!     // Read PCM data
//!     let mut pcm_buffer = Vec::new();
//!     stream.read_to_end(&mut pcm_buffer).await?;
//!
//!     // Wait for decoding to complete
//!     stream.wait().await?;
//!
//!     Ok(())
//! }
//! ```
//!
//! ## Example: Ogg/Vorbis to FLAC Transcoding
//!
//! ```no_run
//! use pmoflac::{decode_ogg_vorbis_stream, encode_flac_stream, PcmFormat, EncoderOptions};
//! use tokio::fs::File;
//! use tokio::io;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Decode Ogg/Vorbis
//!     let ogg_file = File::open("input.ogg").await?;
//!     let stream = decode_ogg_vorbis_stream(ogg_file).await?;
//!     let (info, pcm_reader) = stream.into_parts();
//!
//!     // Encode to FLAC
//!     let format = PcmFormat {
//!         sample_rate: info.sample_rate,
//!         channels: info.channels,
//!         bits_per_sample: info.bits_per_sample,
//!     };
//!     let mut flac_stream = encode_flac_stream(
//!         pcm_reader,
//!         format,
//!         EncoderOptions::default()
//!     ).await?;
//!
//!     // Write FLAC output
//!     let mut output = File::create("output.flac").await?;
//!     io::copy(&mut flac_stream, &mut output).await?;
//!     flac_stream.wait().await?;
//!
//!     Ok(())
//! }
//! ```

use lewton::{
    audio::{self, read_audio_packet_generic, PreviousWindowRight},
    header::{self, read_header_comment, read_header_ident, read_header_setup, CommentHeader},
    samples::InterleavedSamples,
};
use tokio::{
    io::AsyncRead,
    sync::{mpsc, oneshot},
};

use crate::{
    common::ChannelReader,
    decoder_common::{
        spawn_ingest_task, spawn_writer_task, DecodedStream, CHANNEL_CAPACITY, DUPLEX_BUFFER_SIZE,
    },
    ogg_common::{OggContainerError, OggPacketReader, OggReaderOptions},
    pcm::StreamInfo,
    stream::ManagedAsyncReader,
};

/// Shared error alias for the Vorbis decoder.
pub type OggError = OggContainerError;

impl From<header::HeaderReadError> for OggContainerError {
    fn from(err: header::HeaderReadError) -> Self {
        OggContainerError::Decode(err.to_string())
    }
}

impl From<audio::AudioReadError> for OggContainerError {
    fn from(err: audio::AudioReadError) -> Self {
        OggContainerError::Decode(err.to_string())
    }
}

/// Async stream alias for decoded Ogg/Vorbis audio.
pub type OggDecodedStream = DecodedStream<OggError>;

/// Decodes an Ogg/Vorbis stream into PCM audio (16-bit little-endian interleaved).
///
/// This function spawns background tasks to perform the decoding asynchronously.
/// The returned `OggDecodedStream` implements `AsyncRead` for streaming the PCM output.
///
/// # Implementation Details
///
/// The decoder:
/// 1. Searches for "OggS" magic pattern to find the first valid page
/// 2. Parses Ogg page headers and validates CRC32 checksums
/// 3. Assembles Vorbis packets from page segments
/// 4. Decodes the first 3 packets as Vorbis headers (identification, comment, setup)
/// 5. Decodes subsequent packets as audio using lewton's low-level API
/// 6. Streams interleaved PCM samples as they're decoded
///
/// # Arguments
///
/// * `reader` - Any async reader containing Ogg/Vorbis encoded data
///
/// # Returns
///
/// A `OggDecodedStream` that can be read to obtain PCM samples in little-endian
/// interleaved format. The stream's `info()` method provides metadata.
///
/// # Errors
///
/// Returns an error if:
/// - The input is not valid Ogg/Vorbis data
/// - No "OggS" pattern is found within the first 64KB
/// - CRC32 validation fails (indicates corruption)
/// - An I/O error occurs while reading
/// - The decoder encounters corrupted Vorbis data
/// - Multiple logical bitstreams are detected (not supported)
pub async fn decode_ogg_vorbis_stream<R>(reader: R) -> Result<OggDecodedStream, OggError>
where
    R: AsyncRead + Unpin + Send + 'static,
{
    let (ingest_tx, ingest_rx) = mpsc::channel(CHANNEL_CAPACITY);
    spawn_ingest_task(reader, ingest_tx);

    let (pcm_tx, pcm_rx) = mpsc::channel(CHANNEL_CAPACITY);
    let (pcm_reader, pcm_writer) = tokio::io::duplex(DUPLEX_BUFFER_SIZE);
    let (info_tx, info_rx) = oneshot::channel::<Result<StreamInfo, OggError>>();

    let blocking_handle = tokio::task::spawn_blocking(move || -> Result<(), OggError> {
        let channel_reader = ChannelReader::<OggContainerError>::new(ingest_rx);
        let mut packet_reader = OggPacketReader::new(channel_reader, OggReaderOptions::default());

        // Read Vorbis headers (3 packets: identification, comment, setup)
        let ident_packet = packet_reader
            .next_packet()?
            .ok_or_else(|| OggError::Decode("missing Vorbis identification header".into()))?;
        let ident_hdr = read_header_ident(&ident_packet)?;

        let comment_packet = packet_reader
            .next_packet()?
            .ok_or_else(|| OggError::Decode("missing Vorbis comment header".into()))?;
        let _comment_hdr: CommentHeader = read_header_comment(&comment_packet)?;

        let setup_packet = packet_reader
            .next_packet()?
            .ok_or_else(|| OggError::Decode("missing Vorbis setup header".into()))?;
        let setup_hdr = read_header_setup(
            &setup_packet,
            ident_hdr.audio_channels,
            (ident_hdr.blocksize_0, ident_hdr.blocksize_1),
        )?;

        let info = StreamInfo {
            sample_rate: ident_hdr.audio_sample_rate,
            channels: ident_hdr.audio_channels,
            bits_per_sample: 16,
            total_samples: None,
            max_block_size: 1 << ident_hdr.blocksize_1,
            min_block_size: 1 << ident_hdr.blocksize_0,
        };

        if info_tx.send(Ok(info.clone())).is_err() {
            return Ok(());
        }

        // Decode audio packets
        let mut pcm_bytes = Vec::new();
        let mut produced_audio = false;
        let mut pwr = PreviousWindowRight::new();

        while let Some(packet) = packet_reader.next_packet()? {
            let decoded: InterleavedSamples<i16> =
                read_audio_packet_generic(&ident_hdr, &setup_hdr, &packet, &mut pwr)?;

            if decoded.samples.is_empty() {
                continue;
            }

            produced_audio = true;

            // Reuse buffer capacity from previous iteration
            pcm_bytes.clear();
            pcm_bytes.reserve(decoded.samples.len() * 2);
            for sample in decoded.samples {
                pcm_bytes.extend_from_slice(&sample.to_le_bytes());
            }

            let chunk = std::mem::take(&mut pcm_bytes);
            if pcm_tx.blocking_send(Ok(chunk)).is_err() {
                break;
            }

            // Pre-allocate for next iteration
            pcm_bytes =
                Vec::with_capacity(info.max_block_size as usize * info.channels as usize * 2);
        }

        if !produced_audio {
            let err = OggError::Decode("stream contained no decodable Vorbis packets".into());
            let _ = pcm_tx.blocking_send(Err(err.clone()));
            return Err(err);
        }

        Ok(())
    });

    let writer_handle = spawn_writer_task(pcm_rx, pcm_writer, blocking_handle, "ogg-decode");

    let info = info_rx.await.map_err(|_| OggError::ChannelClosed)??;
    let reader = ManagedAsyncReader::new("ogg-decode-writer", pcm_reader, writer_handle);

    Ok(DecodedStream::new(info, reader))
}
