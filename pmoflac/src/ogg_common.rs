//! # Common Ogg Container Parsing
//!
//! This module provides shared functionality for parsing Ogg containers,
//! used by both Ogg/Vorbis and Ogg/Opus decoders.
//!
//! ## Features
//!
//! - **Streaming packet assembly**: Reads Ogg pages and assembles multi-page packets
//! - **Optional CRC32 validation**: Can validate page integrity
//! - **Optional sync search**: Can search for "OggS" pattern in streams with garbage
//! - **Shared error type**: Uses `OggContainerError` for consistent error reporting
//!
//! ## Architecture
//!
//! The `OggPacketReader` reads Ogg pages incrementally:
//! 1. Optionally searches for "OggS" sync pattern
//! 2. Reads 27-byte page headers
//! 3. Optionally validates CRC32 checksums
//! 4. Reads segment tables and page data
//! 5. Assembles packets from segments (handling multi-page packets)
//! 6. Returns complete packets to the decoder

use std::{
    collections::VecDeque,
    io::{self, Read},
};

use crate::{common::ChannelReader, decoder_common::DecoderError};

/// Maximum number of bytes to scan when searching for Ogg sync pattern.
///
/// This prevents unbounded memory growth when processing streams with
/// large amounts of garbage data before the first valid Ogg page.
const MAX_SYNC_SEARCH: usize = 64 * 1024;

/// Configuration options for Ogg packet reader.
#[derive(Clone, Debug)]
pub struct OggReaderOptions {
    /// Whether to validate CRC32 checksums of Ogg pages.
    ///
    /// Vorbis typically validates CRC, Opus often doesn't.
    pub validate_crc: bool,

    /// Whether to search for "OggS" sync pattern at start of stream.
    ///
    /// Useful for streams that may have garbage bytes before valid data.
    pub find_sync: bool,

    /// Maximum bytes to search for sync pattern (only used if find_sync is true).
    pub max_sync_search: usize,
}

impl Default for OggReaderOptions {
    fn default() -> Self {
        Self {
            validate_crc: true,
            find_sync: true,
            max_sync_search: MAX_SYNC_SEARCH,
        }
    }
}

pub type OggContainerError = DecoderError;

/// Streaming Ogg packet reader that assembles packets from Ogg pages.
///
/// This reader parses Ogg pages manually without requiring seek operations,
/// making it suitable for truly streaming scenarios.
///
/// # Features
///
/// - Searches for Ogg sync pattern ("OggS")
/// - Parses page headers and segment tables
/// - Validates CRC32 checksums (optional)
/// - Assembles multi-page packets
/// - Detects end-of-stream
/// - Enforces single logical bitstream
pub struct OggPacketReader {
    reader: ChannelReader<OggContainerError>,
    current_packet: Vec<u8>,
    queue: VecDeque<Vec<u8>>,
    finished: bool,
    stream_serial: Option<u32>,
    sync_buffer: Vec<u8>,
    synced: bool,
    options: OggReaderOptions,
}

impl OggPacketReader {
    /// Creates a new Ogg packet reader with the given options.
    pub fn new(reader: ChannelReader<OggContainerError>, options: OggReaderOptions) -> Self {
        Self {
            reader,
            current_packet: Vec::new(),
            queue: VecDeque::new(),
            finished: false,
            stream_serial: None,
            sync_buffer: Vec::new(),
            synced: !options.find_sync, // If we don't need to find sync, we're already synced
            options,
        }
    }

    /// Returns the next complete packet, or None if the stream has ended.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - I/O error occurs
    /// - Ogg page structure is invalid
    /// - CRC32 validation fails (if enabled)
    /// - Multiple logical bitstreams detected
    pub fn next_packet(&mut self) -> Result<Option<Vec<u8>>, OggContainerError> {
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
    fn read_exact_from_source(&mut self, buf: &mut [u8]) -> Result<bool, OggContainerError> {
        let mut offset = 0;
        while offset < buf.len() {
            let n = self
                .read_bytes(&mut buf[offset..])
                .map_err(OggContainerError::from)?;
            if n == 0 {
                return if offset == 0 {
                    Ok(false) // Clean EOF
                } else {
                    Err(OggContainerError::Decode(
                        "unexpected EOF while reading page".into(),
                    ))
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
    /// It buffers up to max_sync_search bytes while searching.
    fn find_sync(&mut self) -> Result<(), OggContainerError> {
        if self.synced {
            return Ok(());
        }

        while self.sync_buffer.len() < self.options.max_sync_search {
            let mut chunk = [0u8; 1024];
            let n = Read::read(&mut self.reader, &mut chunk).map_err(OggContainerError::from)?;
            if n == 0 {
                return Err(OggContainerError::Decode(
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
            if self.sync_buffer.len() >= self.options.max_sync_search {
                let keep_len = 3.min(self.sync_buffer.len());
                self.sync_buffer.drain(..self.sync_buffer.len() - keep_len);
            }
        }

        Err(OggContainerError::Decode(format!(
            "No Ogg sync pattern found in first {} bytes",
            self.options.max_sync_search
        )))
    }

    /// Reads a single Ogg page and processes its packets.
    ///
    /// This method:
    /// 1. Ensures we're synced to "OggS" pattern (if enabled)
    /// 2. Reads the 27-byte page header
    /// 3. Validates the CRC32 checksum (if enabled)
    /// 4. Reads the segment table
    /// 5. Reads the page data
    /// 6. Assembles packets from segments
    fn read_page(&mut self) -> Result<(), OggContainerError> {
        // Ensure we've found the sync pattern (if required)
        if self.options.find_sync {
            self.find_sync()?;
        }

        // Read 27-byte page header
        let mut header = [0u8; 27];
        if !self.read_exact_from_source(&mut header)? {
            self.finished = true;
            return Ok(());
        }

        // Validate Ogg page header
        if &header[0..4] != b"OggS" {
            return Err(OggContainerError::Decode(
                "invalid Ogg capture pattern".into(),
            ));
        }
        if header[4] != 0 {
            return Err(OggContainerError::Decode("unsupported Ogg version".into()));
        }

        let header_type = header[5];
        let bitstream_serial = u32::from_le_bytes([header[14], header[15], header[16], header[17]]);

        // Enforce single bitstream
        if let Some(serial) = self.stream_serial {
            if serial != bitstream_serial {
                return Err(OggContainerError::Decode(
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

        // Validate CRC32 if enabled
        if self.options.validate_crc {
            let expected_crc = u32::from_le_bytes([header[22], header[23], header[24], header[25]]);
            let mut crc_header = header;
            crc_header[22..26].copy_from_slice(&[0, 0, 0, 0]); // Zero out CRC field

            let mut crc = crc::vorbis_crc32_update(0, &crc_header);
            crc = crc::vorbis_crc32_update(crc, &segment_table);
            crc = crc::vorbis_crc32_update(crc, &data);

            if crc != expected_crc {
                return Err(OggContainerError::Decode(format!(
                    "CRC32 mismatch: expected 0x{expected_crc:08x}, got 0x{crc:08x}"
                )));
            }
        }

        // Validate continuation flags
        if header_type & 0x01 != 0 && self.current_packet.is_empty() {
            return Err(OggContainerError::Decode(
                "unexpected continuation flag without existing packet".into(),
            ));
        }
        if header_type & 0x01 == 0 && !self.current_packet.is_empty() {
            return Err(OggContainerError::Decode(
                "dangling packet without continuation flag".into(),
            ));
        }

        // Assemble packets from segments
        let mut offset: usize = 0;
        for &seg_len in &segment_table {
            let len = seg_len as usize;
            let end = offset
                .checked_add(len)
                .ok_or_else(|| OggContainerError::Decode("segment length overflow".into()))?;
            if end > data.len() {
                return Err(OggContainerError::Decode(
                    "segment exceeds page data".into(),
                ));
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
            return Err(OggContainerError::Decode(
                "page data not fully consumed".into(),
            ));
        }

        // Check for end-of-stream
        if header_type & 0x04 != 0 && self.current_packet.is_empty() {
            self.finished = true;
        }

        Ok(())
    }
}

/// CRC32 calculation for Ogg pages.
///
/// This module implements the CRC32 algorithm used by the Ogg container format.
/// The polynomial is 0x04c11db7 with initial value 0 and no final XOR.
pub(crate) mod crc {
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
