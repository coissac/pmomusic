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

use std::{
    collections::VecDeque,
    io::{self, Read},
    pin::Pin,
    task::{Context, Poll},
};

use lewton::{
    audio::{self, read_audio_packet_generic, PreviousWindowRight},
    header::{self, read_header_comment, read_header_ident, read_header_setup, CommentHeader},
    samples::InterleavedSamples,
};
use tokio::{
    io::{AsyncRead, ReadBuf},
    sync::{mpsc, oneshot},
};

use crate::{
    common::ChannelReader,
    decoder_common::{spawn_ingest_task, spawn_writer_task, CHANNEL_CAPACITY, DUPLEX_BUFFER_SIZE},
    pcm::StreamInfo,
    stream::ManagedAsyncReader,
};

/// Maximum number of bytes to scan when searching for Ogg sync pattern.
///
/// This prevents unbounded memory growth when processing streams with
/// large amounts of garbage data before the first valid Ogg page.
const MAX_SYNC_SEARCH: usize = 64 * 1024;

/// Errors that can occur while decoding Ogg/Vorbis data.
#[derive(thiserror::Error, Debug, Clone)]
pub enum OggError {
    #[error("I/O error ({kind:?}): {message}")]
    Io {
        kind: io::ErrorKind,
        message: String,
    },
    #[error("Ogg/Vorbis decode error: {0}")]
    Decode(String),
    #[error("internal channel closed unexpectedly")]
    ChannelClosed,
    #[error("{role} task failed: {details}")]
    TaskJoin { role: &'static str, details: String },
}

impl From<io::Error> for OggError {
    fn from(err: io::Error) -> Self {
        OggError::Io {
            kind: err.kind(),
            message: err.to_string(),
        }
    }
}

impl From<header::HeaderReadError> for OggError {
    fn from(err: header::HeaderReadError) -> Self {
        OggError::Decode(err.to_string())
    }
}

impl From<audio::AudioReadError> for OggError {
    fn from(err: audio::AudioReadError) -> Self {
        OggError::Decode(err.to_string())
    }
}

impl From<String> for OggError {
    fn from(value: String) -> Self {
        OggError::Decode(value)
    }
}

/// An async stream that decodes Ogg/Vorbis audio into PCM samples.
///
/// This struct implements `AsyncRead`, allowing you to read decoded PCM data
/// as it becomes available. The decoding happens in a background task.
pub struct OggDecodedStream {
    info: StreamInfo,
    reader: ManagedAsyncReader<OggError>,
}

impl OggDecodedStream {
    /// Returns metadata about the decoded Ogg/Vorbis stream.
    ///
    /// This includes sample rate, channel count, bits per sample, and block sizes.
    pub fn info(&self) -> &StreamInfo {
        &self.info
    }

    /// Consumes the stream and returns its components.
    ///
    /// Useful for chaining with encoders without buffering the PCM data.
    pub fn into_parts(self) -> (StreamInfo, ManagedAsyncReader<OggError>) {
        (self.info, self.reader)
    }

    /// Waits for the background decoding task to complete.
    ///
    /// This should be called after reading all data to ensure proper cleanup
    /// and to catch any errors that occurred during decoding.
    pub async fn wait(self) -> Result<(), OggError> {
        self.reader.wait().await
    }
}

impl AsyncRead for OggDecodedStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.reader).poll_read(cx, buf)
    }
}

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
        let channel_reader = ChannelReader::<OggError>::new(ingest_rx);
        let mut packet_reader = StreamingPacketReader::new(channel_reader);

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

    Ok(OggDecodedStream { info, reader })
}

/// Streaming packet reader that assembles Vorbis packets from Ogg pages.
///
/// This reader parses Ogg pages manually without requiring seek operations,
/// making it suitable for truly streaming scenarios. It handles:
/// - Searching for Ogg sync pattern ("OggS")
/// - Parsing page headers and segment tables
/// - Validating CRC32 checksums
/// - Assembling multi-page packets
/// - Detecting end-of-stream
struct StreamingPacketReader<E>
where
    E: std::error::Error + std::fmt::Display,
{
    reader: ChannelReader<E>,
    current_packet: Vec<u8>,
    queue: VecDeque<Vec<u8>>,
    finished: bool,
    eos_seen: bool,
    stream_serial: Option<u32>,
    sync_buffer: Vec<u8>,
    synced: bool,
}

impl<E> StreamingPacketReader<E>
where
    E: std::error::Error + std::fmt::Display,
{
    fn new(reader: ChannelReader<E>) -> Self {
        Self {
            reader,
            current_packet: Vec::new(),
            queue: VecDeque::new(),
            finished: false,
            eos_seen: false,
            stream_serial: None,
            sync_buffer: Vec::new(),
            synced: false,
        }
    }

    /// Returns the next complete Vorbis packet, or None if the stream has ended.
    fn next_packet(&mut self) -> Result<Option<Vec<u8>>, OggError> {
        loop {
            if let Some(packet) = self.queue.pop_front() {
                return Ok(Some(packet));
            }
            if self.finished {
                return Ok(None);
            }
            self.read_page()?;
        }
    }

    /// Reads bytes, first from sync_buffer then from the underlying reader.
    fn read_bytes(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }

        let mut total = 0;

        // First, consume from sync_buffer
        if !self.sync_buffer.is_empty() {
            let to_copy = buf.len().min(self.sync_buffer.len());
            buf[..to_copy].copy_from_slice(&self.sync_buffer[..to_copy]);
            self.sync_buffer.drain(..to_copy);
            total += to_copy;
            if total == buf.len() {
                return Ok(total);
            }
        }

        // Then read from underlying reader
        while total < buf.len() {
            match Read::read(&mut self.reader, &mut buf[total..])? {
                0 => break,
                n => total += n,
            }
        }

        Ok(total)
    }

    /// Reads exactly buf.len() bytes or returns error.
    fn read_exact_from_source(&mut self, buf: &mut [u8]) -> Result<bool, OggError> {
        let mut offset = 0;
        while offset < buf.len() {
            let n = self.read_bytes(&mut buf[offset..])?;
            if n == 0 {
                return if offset == 0 {
                    Ok(false) // Clean EOF
                } else {
                    Err(OggError::Decode("unexpected EOF while reading page".into()))
                };
            }
            offset += n;
        }
        Ok(true)
    }

    /// Searches for the Ogg sync pattern ("OggS") in the stream.
    ///
    /// This is called before reading the first page to handle streams that
    /// have garbage bytes at the beginning (e.g., HTTP headers, ID3 tags).
    /// It buffers up to MAX_SYNC_SEARCH bytes while searching.
    fn find_sync(&mut self) -> Result<(), OggError> {
        if self.synced {
            return Ok(());
        }

        while self.sync_buffer.len() < MAX_SYNC_SEARCH {
            let mut chunk = [0u8; 1024];
            let n = Read::read(&mut self.reader, &mut chunk)?;
            if n == 0 {
                return Err(OggError::Decode(
                    "EOF reached while searching for Ogg sync pattern".into(),
                ));
            }
            self.sync_buffer.extend_from_slice(&chunk[..n]);

            // Search for "OggS" pattern
            if let Some(pos) = self
                .sync_buffer
                .windows(4)
                .position(|window| window == b"OggS")
            {
                // Found sync! Remove garbage bytes before it
                self.sync_buffer.drain(..pos);
                self.synced = true;
                return Ok(());
            }

            // If buffer is getting large and still no sync, keep only last 3 bytes
            // (in case "OggS" is split across chunk boundary)
            if self.sync_buffer.len() >= MAX_SYNC_SEARCH {
                let keep_len = 3.min(self.sync_buffer.len());
                self.sync_buffer.drain(..self.sync_buffer.len() - keep_len);
            }
        }

        Err(OggError::Decode(format!(
            "No Ogg sync pattern found in first {} bytes",
            MAX_SYNC_SEARCH
        )))
    }

    /// Reads a single Ogg page and processes its packets.
    ///
    /// This method:
    /// 1. Ensures we're synced to "OggS" pattern
    /// 2. Reads the 27-byte page header
    /// 3. Validates the CRC32 checksum
    /// 4. Reads the segment table
    /// 5. Reads the page data
    /// 6. Assembles packets from segments
    fn read_page(&mut self) -> Result<(), OggError> {
        // Ensure we've found the sync pattern
        self.find_sync()?;

        // Read 27-byte page header
        let mut header = [0u8; 27];
        if !self.read_exact_from_source(&mut header)? {
            self.finished = true;
            return Ok(());
        }

        // Validate Ogg page header
        if &header[0..4] != b"OggS" {
            return Err(OggError::Decode("invalid Ogg capture pattern".into()));
        }
        if header[4] != 0 {
            return Err(OggError::Decode("unsupported Ogg version".into()));
        }

        let header_type = header[5];
        let bitstream_serial = u32::from_le_bytes([header[14], header[15], header[16], header[17]]);

        // Enforce single bitstream
        if let Some(serial) = self.stream_serial {
            if serial != bitstream_serial {
                return Err(OggError::Decode(
                    "multiple logical streams are not supported".into(),
                ));
            }
        } else {
            self.stream_serial = Some(bitstream_serial);
        }

        // Read segment table
        let page_segments = header[26] as usize;
        let mut segment_table = vec![0u8; page_segments];
        self.read_exact_from_source(&mut segment_table)?;

        // Calculate page data length
        let data_len: usize = segment_table.iter().map(|&v| v as usize).sum();
        let mut data = vec![0u8; data_len];
        self.read_exact_from_source(&mut data)?;

        // Validate CRC32
        let expected_crc = u32::from_le_bytes([header[22], header[23], header[24], header[25]]);
        let mut crc_header = header;
        crc_header[22..26].copy_from_slice(&[0, 0, 0, 0]); // Zero out CRC field

        let mut crc = crc::vorbis_crc32_update(0, &crc_header);
        crc = crc::vorbis_crc32_update(crc, &segment_table);
        crc = crc::vorbis_crc32_update(crc, &data);

        if crc != expected_crc {
            return Err(OggError::Decode(format!(
                "CRC32 mismatch: expected 0x{:08x}, got 0x{:08x}",
                expected_crc, crc
            )));
        }

        // Validate continuation flags
        if header_type & 0x01 != 0 && self.current_packet.is_empty() {
            return Err(OggError::Decode(
                "unexpected continuation flag without existing packet".into(),
            ));
        }
        if header_type & 0x01 == 0 && !self.current_packet.is_empty() {
            return Err(OggError::Decode(
                "dangling packet without continuation flag".into(),
            ));
        }

        // Assemble packets from segments
        let mut offset: usize = 0;
        for &seg_len in &segment_table {
            let len = seg_len as usize;
            let end = offset
                .checked_add(len)
                .ok_or_else(|| OggError::Decode("segment length overflow".into()))?;
            if end > data.len() {
                return Err(OggError::Decode("segment exceeds page data".into()));
            }
            self.current_packet.extend_from_slice(&data[offset..end]);
            offset = end;

            // Packet complete when segment is less than 255 bytes
            if seg_len < 255 {
                let packet = std::mem::take(&mut self.current_packet);
                self.queue.push_back(packet);
            }
        }

        if offset != data.len() {
            return Err(OggError::Decode("page data not fully consumed".into()));
        }

        // Check for end-of-stream
        if header_type & 0x04 != 0 {
            self.eos_seen = true;
            if self.current_packet.is_empty() {
                self.finished = true;
            }
        }

        Ok(())
    }
}

/// CRC32 calculation for Ogg pages.
///
/// This module implements the CRC32 algorithm used by the Ogg container format.
/// The polynomial is 0x04c11db7 with initial value 0 and no final XOR.
mod crc {
    /// Precomputed CRC32 lookup table for Ogg.
    ///
    /// Generated using the polynomial 0x04c11db7.
    const fn get_tbl_elem(idx: u32) -> u32 {
        let mut r: u32 = idx << 24;
        let mut i = 0;
        while i < 8 {
            r = (r << 1) ^ (-(((r >> 31) & 1) as i32) as u32 & 0x04c11db7);
            i += 1;
        }
        r
    }

    const fn lookup_array() -> [u32; 0x100] {
        let mut lup_arr: [u32; 0x100] = [0; 0x100];
        let mut i = 0;
        while i < 0x100 {
            lup_arr[i] = get_tbl_elem(i as u32);
            i += 1;
        }
        lup_arr
    }

    static CRC_LOOKUP_ARRAY: &[u32] = &lookup_array();

    /// Updates the CRC32 value with new data.
    ///
    /// # Arguments
    ///
    /// * `cur` - Current CRC32 value (use 0 for initial call)
    /// * `array` - Data to include in CRC calculation
    ///
    /// # Returns
    ///
    /// Updated CRC32 value
    pub fn vorbis_crc32_update(cur: u32, array: &[u8]) -> u32 {
        let mut ret: u32 = cur;
        for av in array {
            ret = (ret << 8) ^ CRC_LOOKUP_ARRAY[(*av as u32 ^ (ret >> 24)) as usize];
        }
        ret
    }
}
