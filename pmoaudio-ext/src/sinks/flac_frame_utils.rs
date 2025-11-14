//! Utilities for FLAC frame detection and validation
//!
//! This module provides functions to detect and validate FLAC frame boundaries
//! in a stream of bytes. It implements comprehensive FLAC frame header validation
//! to avoid false positives from random data that matches the sync pattern.
//!
//! Frame header validation includes CRC-8 verification as per FLAC specification
//! to eliminate false positives that would cause decoder errors.

/// Validate and parse FLAC block size from frame header
///
/// Returns the number of samples in the frame if the header is valid, or None if:
/// - The header is truncated
/// - The sync code is incorrect
/// - Any reserved bits are set
/// - Sample rate, channel assignment, or bits per sample codes are invalid
///
/// This comprehensive validation is essential to avoid false positives, as the
/// FLAC sync pattern (0xFF 0xF8-0xFE) can appear randomly in compressed audio data.
pub(crate) fn parse_flac_block_size(data: &[u8], offset: usize) -> Option<u32> {
    if offset + 4 > data.len() {
        return None;
    }

    // FLAC frame header starts with sync code 0xFF 0xF8-0xFF
    if data[offset] != 0xFF || data[offset + 1] < 0xF8 {
        return None;
    }

    // Validate reserved bit (bit 1 of byte 1 must be 0)
    if (data[offset + 1] & 0x02) != 0 {
        return None; // Reserved bit set = not a valid frame header
    }

    let byte2 = data[offset + 2];
    let byte3 = data[offset + 3];

    // Byte 2 contains block size code in bits 4-7
    let block_size_code = (byte2 >> 4) & 0x0F;

    // Byte 2 bits 0-3 contain sample rate code
    let sample_rate_code = byte2 & 0x0F;

    // Validate sample rate code (0x0F is invalid)
    if sample_rate_code == 0x0F {
        return None; // Invalid sample rate = not a valid frame header
    }

    // Byte 3 bits 4-7 contain channel assignment
    let channel_assignment = (byte3 >> 4) & 0x0F;

    // Validate channel assignment (values 0x0B-0x0F are reserved/invalid)
    if channel_assignment >= 0x0B {
        return None; // Invalid channel assignment = not a valid frame header
    }

    // Byte 3 bits 1-3 contain bits per sample code
    let bits_per_sample = (byte3 >> 1) & 0x07;

    // Validate bits per sample (values 0x03 and 0x07 are reserved)
    if bits_per_sample == 0x03 || bits_per_sample == 0x07 {
        return None; // Invalid bits per sample = not a valid frame header
    }

    // Validate reserved bit in byte 3 (bit 0 must be 0)
    if (byte3 & 0x01) != 0 {
        return None; // Reserved bit set = not a valid frame header
    }

    // Decode block size according to FLAC spec
    let block_size = match block_size_code {
        0x00 => return None, // Reserved
        0x01 => 192,
        0x02..=0x05 => 576 * (1 << (block_size_code - 2)),
        0x06 => return None, // Get 8-bit value from end of header (not fully validated here)
        0x07 => return None, // Get 16-bit value from end of header (not fully validated here)
        0x08..=0x0F => 256 * (1 << (block_size_code - 8)),
        _ => return None,
    };

    Some(block_size)
}

/// Calculate FLAC CRC-8 checksum for frame header validation
///
/// The FLAC CRC-8 uses polynomial x^8 + x^2 + x^1 + x^0 (0x07)
/// and is initialized with 0. It covers all bytes of the frame header
/// including the sync code, up to but not including the CRC byte itself.
fn calculate_flac_crc8(data: &[u8]) -> u8 {
    const CRC8_TABLE: [u8; 256] = generate_flac_crc8_table();

    let mut crc: u8 = 0;
    for &byte in data {
        crc = CRC8_TABLE[(crc ^ byte) as usize];
    }
    crc
}

/// Generate FLAC CRC-8 lookup table at compile time
/// Polynomial: x^8 + x^2 + x^1 + x^0 = 0x07
const fn generate_flac_crc8_table() -> [u8; 256] {
    let mut table = [0u8; 256];
    let mut i = 0;
    while i < 256 {
        let mut crc = i as u8;
        let mut j = 0;
        while j < 8 {
            if (crc & 0x80) != 0 {
                crc = (crc << 1) ^ 0x07;
            } else {
                crc <<= 1;
            }
            j += 1;
        }
        table[i] = crc;
        i += 1;
    }
    table
}

/// Decode UTF-8 coded frame/sample number from FLAC frame header
/// Returns (value, bytes_consumed) or None if invalid
fn decode_utf8_number(data: &[u8], offset: usize) -> Option<(u64, usize)> {
    if offset >= data.len() {
        return None;
    }

    let first_byte = data[offset];

    // Determine number of bytes based on first byte
    let (num_bytes, mask) = if (first_byte & 0x80) == 0 {
        // 0xxxxxxx - 1 byte
        return Some((first_byte as u64, 1));
    } else if (first_byte & 0xE0) == 0xC0 {
        // 110xxxxx - 2 bytes
        (2, 0x1F)
    } else if (first_byte & 0xF0) == 0xE0 {
        // 1110xxxx - 3 bytes
        (3, 0x0F)
    } else if (first_byte & 0xF8) == 0xF0 {
        // 11110xxx - 4 bytes
        (4, 0x07)
    } else if (first_byte & 0xFC) == 0xF8 {
        // 111110xx - 5 bytes
        (5, 0x03)
    } else if (first_byte & 0xFE) == 0xFC {
        // 1111110x - 6 bytes
        (6, 0x01)
    } else if first_byte == 0xFE {
        // 11111110 - 7 bytes (max for FLAC)
        (7, 0x00)
    } else {
        return None; // Invalid UTF-8 pattern
    };

    // Check we have enough bytes
    if offset + num_bytes > data.len() {
        return None;
    }

    // Decode the value
    let mut value = (first_byte & mask) as u64;
    for i in 1..num_bytes {
        let byte = data[offset + i];
        // Continuation bytes must match 10xxxxxx
        if (byte & 0xC0) != 0x80 {
            return None;
        }
        value = (value << 6) | ((byte & 0x3F) as u64);
    }

    Some((value, num_bytes))
}

/// Get the complete frame header length including CRC-8
/// Returns Some(header_length) if valid, None otherwise
///
/// This function parses the complete frame header structure:
/// - Sync code (2 bytes)
/// - Block size + sample rate codes (1 byte)
/// - Channel assignment + bits per sample (1 byte)
/// - Frame/sample number UTF-8 (1-7 bytes)
/// - Optional block size (1-2 bytes if code is 6 or 7)
/// - Optional sample rate (1-2 bytes if code is 12, 13, or 14)
/// - CRC-8 (1 byte)
pub(crate) fn get_frame_header_length(data: &[u8], offset: usize) -> Option<usize> {
    if offset + 4 > data.len() {
        return None;
    }

    // Basic validation (reuse existing checks)
    if parse_flac_block_size(data, offset).is_none() {
        return None;
    }

    let byte2 = data[offset + 2];
    let block_size_code = (byte2 >> 4) & 0x0F;
    let sample_rate_code = byte2 & 0x0F;

    // Start after the fixed 4-byte header
    let mut pos = offset + 4;

    // Decode UTF-8 frame/sample number
    let (_number, utf8_bytes) = decode_utf8_number(data, pos)?;
    pos += utf8_bytes;

    // Optional block size (if code is 6 or 7)
    match block_size_code {
        0x06 => {
            if pos >= data.len() {
                return None;
            }
            pos += 1; // 8-bit block size - 1
        }
        0x07 => {
            if pos + 1 >= data.len() {
                return None;
            }
            pos += 2; // 16-bit block size - 1
        }
        _ => {}
    }

    // Optional sample rate (if code is 12, 13, or 14)
    match sample_rate_code {
        0x0C => {
            if pos >= data.len() {
                return None;
            }
            pos += 1; // 8-bit sample rate in kHz
        }
        0x0D => {
            if pos + 1 >= data.len() {
                return None;
            }
            pos += 2; // 16-bit sample rate in Hz
        }
        0x0E => {
            if pos + 1 >= data.len() {
                return None;
            }
            pos += 2; // 16-bit sample rate in 10 Hz
        }
        _ => {}
    }

    // CRC-8 is the last byte
    if pos >= data.len() {
        return None;
    }
    pos += 1; // CRC-8 byte

    Some(pos - offset)
}

/// Validate complete FLAC frame header including CRC-8
/// Returns true if the header is valid, false otherwise
pub(crate) fn validate_frame_header_crc(data: &[u8], offset: usize) -> bool {
    // Get the complete header length
    let header_length = match get_frame_header_length(data, offset) {
        Some(len) => len,
        None => return false,
    };

    if offset + header_length > data.len() {
        return false;
    }

    // The CRC-8 is the last byte of the header
    let stored_crc = data[offset + header_length - 1];

    // Calculate CRC-8 over all bytes except the CRC itself
    let header_bytes = &data[offset..offset + header_length - 1];
    let calculated_crc = calculate_flac_crc8(header_bytes);

    calculated_crc == stored_crc
}

/// Find the position where we should split the buffer to send complete FLAC frames
///
/// Returns the byte position just before the last FLAC frame starts.
///
/// FLAC frames start with a validated sync code with CRC-8 verification.
/// The sync code marks the START of a frame. To send complete frames, we find the
/// last sync code and send everything BEFORE it (which contains complete frames),
/// keeping the data from the last sync code onward for the next iteration.
///
/// We need at least 2 validated sync codes to identify one complete frame.
pub(crate) fn find_complete_frames_boundary(data: &[u8]) -> usize {
    if data.len() < 4 {
        return 0;
    }

    let mut sync_positions = Vec::new();

    // Search for FLAC sync codes and validate frame headers with CRC-8
    for i in 0..data.len() - 1 {
        let byte1 = data[i];
        let byte2 = data[i + 1];

        // Check for potential sync code pattern
        if byte1 == 0xFF && byte2 >= 0xF8 && byte2 <= 0xFE {
            // Validate the complete frame header including CRC-8
            if validate_frame_header_crc(data, i) {
                sync_positions.push(i);
            }
        }
    }

    // We need at least 2 sync codes to identify one complete frame
    // The last sync code marks the start of a potentially incomplete frame
    // Return the position of the last sync code - everything before it is complete
    if sync_positions.len() >= 2 {
        *sync_positions.last().unwrap()
    } else {
        0
    }
}

/// Find complete FLAC frames and calculate total samples
///
/// Returns (byte_position, total_samples) where:
/// - byte_position: Position of the last frame boundary (or 0 if less than 2 frames)
/// - total_samples: Sum of samples in all complete frames (excluding the last incomplete one)
///
/// This is useful for OGG-FLAC streams that need to track granule position.
pub(crate) fn find_complete_frames_with_samples(data: &[u8]) -> (usize, u64) {
    if data.len() < 4 {
        return (0, 0);
    }

    let mut sync_positions = Vec::new();
    let mut frame_samples = Vec::new();

    // Search for FLAC sync codes and validate frame headers with CRC-8
    for i in 0..data.len() - 1 {
        let byte1 = data[i];
        let byte2 = data[i + 1];

        // Check for potential sync code pattern
        if byte1 == 0xFF && byte2 >= 0xF8 && byte2 <= 0xFE {
            // Validate the complete frame header including CRC-8
            if validate_frame_header_crc(data, i) {
                // Also get the block size for this frame
                if let Some(samples) = parse_flac_block_size(data, i) {
                    sync_positions.push(i);
                    frame_samples.push(samples);
                }
            }
        }
    }

    // We need at least 2 sync codes to identify one complete frame
    if sync_positions.len() >= 2 {
        let boundary = *sync_positions.last().unwrap();
        // Sum samples for all complete frames (all except the last incomplete one)
        let total_samples: u64 = frame_samples
            .iter()
            .take(sync_positions.len() - 1)
            .map(|&s| s as u64)
            .sum();
        (boundary, total_samples)
    } else {
        (0, 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reject_false_positive_at_position_7() {
        // Real-world example: first frame at 0, false positive at 7
        let data = vec![
            0xFF, 0xF8, 0xC9, 0xA8, // Valid frame header at position 0
            0x00, 0x8D, 0x4C, 0xFF, 0xFE, 0x00,
            0x00, // False positive at position 7 (0xFE has reserved bit set)
        ];

        // Position 0 should be valid
        assert!(parse_flac_block_size(&data, 0).is_some());

        // Position 7 should be rejected (reserved bit validation)
        assert!(parse_flac_block_size(&data, 7).is_none());
    }

    #[test]
    fn test_boundary_detection_with_validation() {
        // Create data with valid frame at 0 and false positive at 7
        let mut data = vec![0xFF, 0xF8, 0xC9, 0xA8]; // Valid header
        data.extend_from_slice(&[0u8; 2000]); // Frame data
        data.extend_from_slice(&[0xFF, 0xF8, 0xC9, 0xA8]); // Second valid frame at ~2004

        let boundary = find_complete_frames_boundary(&data);

        // Should find 2 valid frames and return position of second one
        assert!(boundary > 2000);
    }
}
