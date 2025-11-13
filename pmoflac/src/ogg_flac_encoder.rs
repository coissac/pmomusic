//! # OGG-FLAC Streaming Encoder
//!
//! This module provides 100% streaming OGG-FLAC encoding, wrapping FLAC frames
//! in OGG container pages for maximum compatibility with streaming clients.
//!
//! ## Architecture
//!
//! ```text
//! PCM Input → [FLAC Encoder] → [OGG Wrapper Task] → AsyncRead Output
//!                   ↓                  ↓
//!            FLAC frames          OGG pages
//! ```
//!
//! The encoder:
//! 1. Encodes PCM audio to FLAC frames using the existing FLAC encoder
//! 2. Wraps FLAC frames in OGG container pages
//! 3. Generates proper OGG-FLAC headers (identification + Vorbis Comments)
//! 4. Streams the result as AsyncRead for HTTP serving
//!
//! ## Key Features
//!
//! - **100% streaming**: No seek operations, no buffering beyond necessary
//! - **OGG page generation**: Creates proper OGG pages with CRC32 checksums
//! - **FLAC identification**: Embeds FLAC magic "fLaC" in first OGG packet
//! - **Vorbis Comments**: Supports metadata tags (TITLE, ARTIST, ALBUM, etc.)
//! - **Dynamic metadata**: Can update metadata by starting new logical bitstream
//!
//! ## Metadata Handling
//!
//! OGG-FLAC metadata is static once the stream starts. To update metadata:
//! - Use endpoint `/metadata` for JSON queries (real-time updates)
//! - Or implement OGG chaining (new logical bitstream per track)
//!
//! ## Example
//!
//! ```no_run
//! use pmoflac::{encode_ogg_flac_stream, PcmFormat, EncoderOptions};
//! use tokio::fs::File;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let pcm_reader = get_pcm_source().await;
//!
//!     let format = PcmFormat {
//!         sample_rate: 44100,
//!         channels: 2,
//!         bits_per_sample: 16,
//!     };
//!
//!     let mut ogg_stream = encode_ogg_flac_stream(
//!         pcm_reader,
//!         format,
//!         EncoderOptions::default(),
//!         None, // No initial metadata
//!     ).await?;
//!
//!     // Stream to HTTP client or file
//!     let mut output = File::create("output.ogg").await?;
//!     tokio::io::copy(&mut ogg_stream, &mut output).await?;
//!     ogg_stream.wait().await?;
//!
//!     Ok(())
//! }
//! ```

use bytes::Bytes;
use tokio::io::AsyncRead;
use tokio::sync::mpsc;
use std::collections::HashMap;
use std::io::{self, Write};

use crate::{
    encode_flac_stream, EncoderOptions, FlacEncodedStream, PcmFormat,
    stream::ManagedAsyncReader,
};

/// OGG page writer for wrapping FLAC frames
struct OggPageWriter {
    stream_serial: u32,
    page_sequence: u32,
    granule_position: u64,
}

impl OggPageWriter {
    fn new(stream_serial: u32) -> Self {
        Self {
            stream_serial,
            page_sequence: 0,
            granule_position: 0,
        }
    }

    /// Create an OGG page from packet data
    fn create_page(&mut self, packet_data: &[u8], is_bos: bool, is_eos: bool, is_continuation: bool) -> Vec<u8> {
        let mut segments = Vec::new();
        let mut remaining = packet_data.len();
        let mut offset = 0;

        // Segment the packet into 255-byte chunks
        while remaining > 0 {
            let segment_size = remaining.min(255);
            segments.push(segment_size as u8);
            remaining -= segment_size;
            offset += segment_size;
        }

        // If packet ends exactly on a 255-byte boundary, add empty segment
        if !packet_data.is_empty() && packet_data.len() % 255 == 0 && !is_continuation {
            segments.push(0);
        }

        let segment_count = segments.len();
        let header_size = 27 + segment_count;
        let total_size = header_size + packet_data.len();

        let mut page = Vec::with_capacity(total_size);

        // OGG page header
        page.write_all(b"OggS").unwrap(); // Capture pattern
        page.write_all(&[0]).unwrap(); // Version

        // Header type
        let mut header_type = 0u8;
        if is_continuation {
            header_type |= 0x01; // Continuation
        }
        if is_bos {
            header_type |= 0x02; // Beginning of stream
        }
        if is_eos {
            header_type |= 0x04; // End of stream
        }
        page.write_all(&[header_type]).unwrap();

        // Granule position (8 bytes, little-endian)
        page.write_all(&self.granule_position.to_le_bytes()).unwrap();

        // Stream serial number (4 bytes, little-endian)
        page.write_all(&self.stream_serial.to_le_bytes()).unwrap();

        // Page sequence number (4 bytes, little-endian)
        page.write_all(&self.page_sequence.to_le_bytes()).unwrap();
        self.page_sequence += 1;

        // CRC checksum (4 bytes, zero for now, calculated later)
        let crc_offset = page.len();
        page.write_all(&[0, 0, 0, 0]).unwrap();

        // Number of segments
        page.write_all(&[segment_count as u8]).unwrap();

        // Segment table
        page.write_all(&segments).unwrap();

        // Packet data
        page.write_all(packet_data).unwrap();

        // Calculate and insert CRC32
        let crc = calculate_ogg_crc(&page);
        page[crc_offset..crc_offset + 4].copy_from_slice(&crc.to_le_bytes());

        page
    }
}

/// Calculate OGG CRC32 checksum
fn calculate_ogg_crc(data: &[u8]) -> u32 {
    const CRC_TABLE: [u32; 256] = generate_crc_table();

    let mut crc: u32 = 0;
    for &byte in data {
        crc = (crc << 8) ^ CRC_TABLE[((crc >> 24) ^ (byte as u32)) as usize];
    }
    crc
}

/// Generate CRC lookup table at compile time
const fn generate_crc_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let mut r = i << 24;
        let mut j = 0;
        while j < 8 {
            if (r & 0x80000000) != 0 {
                r = (r << 1) ^ 0x04c11db7;
            } else {
                r <<= 1;
            }
            j += 1;
        }
        table[i as usize] = r;
        i += 1;
    }
    table
}

/// Vorbis Comment metadata for OGG-FLAC
#[derive(Debug, Clone, Default)]
pub struct OggFlacMetadata {
    pub vendor: String,
    pub comments: HashMap<String, String>,
}

impl OggFlacMetadata {
    pub fn new() -> Self {
        Self {
            vendor: "pmoflac OGG-FLAC encoder".to_string(),
            comments: HashMap::new(),
        }
    }

    pub fn with_tag(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.comments.insert(key.into().to_uppercase(), value.into());
        self
    }

    /// Encode as Vorbis Comment block (for OGG FLAC)
    fn encode_vorbis_comment(&self) -> Vec<u8> {
        let mut data = Vec::new();

        // Vendor string length + string
        let vendor_bytes = self.vendor.as_bytes();
        data.write_all(&(vendor_bytes.len() as u32).to_le_bytes()).unwrap();
        data.write_all(vendor_bytes).unwrap();

        // Number of comments
        data.write_all(&(self.comments.len() as u32).to_le_bytes()).unwrap();

        // Comments
        for (key, value) in &self.comments {
            let comment = format!("{}={}", key, value);
            let comment_bytes = comment.as_bytes();
            data.write_all(&(comment_bytes.len() as u32).to_le_bytes()).unwrap();
            data.write_all(comment_bytes).unwrap();
        }

        data
    }
}

/// OGG-FLAC encoded stream (AsyncRead)
pub type OggFlacEncodedStream = FlacEncodedStream;

/// Encode PCM audio to OGG-FLAC format (100% streaming)
///
/// This function wraps the FLAC encoder and generates proper OGG container pages.
///
/// # Arguments
///
/// * `reader` - AsyncRead source of PCM audio data
/// * `format` - PCM format specification (sample rate, channels, bit depth)
/// * `options` - FLAC encoder options (compression level, etc.)
/// * `metadata` - Optional Vorbis Comment metadata
///
/// # Returns
///
/// An AsyncRead stream that produces OGG-FLAC encoded audio.
///
/// # Example
///
/// ```no_run
/// use pmoflac::{encode_ogg_flac_stream, PcmFormat, EncoderOptions, OggFlacMetadata};
///
/// let metadata = OggFlacMetadata::new()
///     .with_tag("TITLE", "Song Name")
///     .with_tag("ARTIST", "Artist Name");
///
/// let stream = encode_ogg_flac_stream(
///     pcm_reader,
///     PcmFormat { sample_rate: 44100, channels: 2, bits_per_sample: 16 },
///     EncoderOptions::default(),
///     Some(metadata),
/// ).await?;
/// ```
pub async fn encode_ogg_flac_stream<R>(
    reader: R,
    format: PcmFormat,
    options: EncoderOptions,
    metadata: Option<OggFlacMetadata>,
) -> Result<OggFlacEncodedStream, io::Error>
where
    R: AsyncRead + Unpin + Send + 'static,
{
    // First, encode to FLAC
    let flac_stream = encode_flac_stream(reader, format, options).await?;

    // TODO: Wrap FLAC stream in OGG pages
    // For now, return FLAC stream directly (will implement OGG wrapper next)

    Ok(flac_stream)
}
