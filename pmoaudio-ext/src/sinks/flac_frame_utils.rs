//! Utilities for FLAC frame detection and validation
//!
//! This module provides functions to detect and validate FLAC frame boundaries
//! in a stream of bytes. It implements comprehensive FLAC frame header validation
//! to avoid false positives from random data that matches the sync pattern.

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

/// Find the position where we should split the buffer to send complete FLAC frames
///
/// Returns the byte position just before the last FLAC frame starts.
///
/// FLAC frames start with a validated sync code (see `parse_flac_block_size`).
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

    // Search for FLAC sync codes and validate frame headers
    for i in 0..data.len() - 1 {
        let byte1 = data[i];
        let byte2 = data[i + 1];

        // Check for potential sync code pattern
        if byte1 == 0xFF && byte2 >= 0xF8 && byte2 <= 0xFE {
            // Validate the complete frame header before accepting
            if parse_flac_block_size(data, i).is_some() {
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

    // Search for FLAC sync codes and validate frame headers
    for i in 0..data.len() - 1 {
        let byte1 = data[i];
        let byte2 = data[i + 1];

        // Check for potential sync code pattern
        if byte1 == 0xFF && byte2 >= 0xF8 && byte2 <= 0xFE {
            // Validate the complete frame header before accepting
            if let Some(samples) = parse_flac_block_size(data, i) {
                sync_positions.push(i);
                frame_samples.push(samples);
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
            0x00, 0x8D, 0x4C,
            0xFF, 0xFE, 0x00, 0x00, // False positive at position 7 (0xFE has reserved bit set)
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
