use std::cmp;

/// Describes the properties of a FLAC/PCM stream.
#[derive(Debug, Clone)]
pub struct StreamInfo {
    pub sample_rate: u32,
    pub channels: u8,
    pub bits_per_sample: u8,
    pub total_samples: Option<u64>,
    pub max_block_size: u16,
    pub min_block_size: u16,
}

impl StreamInfo {
    pub fn bytes_per_sample(&self) -> usize {
        bytes_per_sample(self.bits_per_sample)
    }
}

/// Describes the format of PCM audio data.
///
/// This is used when encoding PCM to FLAC to specify the audio properties.
#[derive(Debug, Clone, Copy)]
pub struct PcmFormat {
    /// Sample rate in Hz (e.g., 44100, 48000, 96000)
    pub sample_rate: u32,

    /// Number of audio channels (1 = mono, 2 = stereo, etc.)
    pub channels: u8,

    /// Bits per sample (typically 16 or 24)
    pub bits_per_sample: u8,
}

impl PcmFormat {
    /// Validates the PCM format parameters.
    ///
    /// # Errors
    ///
    /// Returns an error if any parameter is invalid or out of the supported range.
    ///
    /// # Warnings
    ///
    /// This function will log warnings (via the error message) for unusual but
    /// technically valid configurations.
    pub fn validate(&self) -> Result<(), String> {
        // Validate channels
        if self.channels == 0 {
            return Err("channel count must be greater than 0".into());
        }
        if self.channels > 8 {
            return Err("channel count greater than 8 is unsupported by FLAC".into());
        }

        // Validate sample rate
        if self.sample_rate == 0 {
            return Err("sample rate must be greater than 0".into());
        }
        if self.sample_rate > 655_350 {
            return Err("sample rate exceeds FLAC maximum (655350 Hz)".into());
        }

        // Warn about unusual sample rates
        const STANDARD_RATES: &[u32] = &[
            8000, 11025, 16000, 22050, 32000, 44100, 48000, 88200, 96000, 176400, 192000, 352800,
            384000,
        ];
        if !STANDARD_RATES.contains(&self.sample_rate) {
            tracing::warn!(
                sample_rate = %self.sample_rate,
                "non-standard sample rate (valid but unusual)"
            );
        }

        // Validate bit depth
        if self.bits_per_sample == 0 || self.bits_per_sample > 32 {
            return Err("bits per sample must be in 1..=32".into());
        }

        // FLAC officially supports 4-32 bits/sample
        if self.bits_per_sample < 4 {
            tracing::warn!(
                bits_per_sample = %self.bits_per_sample,
                "bits_per_sample is less than 4 (unusual for FLAC)"
            );
        }

        // Warn about common bit depths
        const COMMON_BIT_DEPTHS: &[u8] = &[8, 16, 24, 32];
        if !COMMON_BIT_DEPTHS.contains(&self.bits_per_sample) {
            tracing::warn!(
                bits_per_sample = %self.bits_per_sample,
                "non-standard bit depth (valid but unusual)"
            );
        }

        Ok(())
    }

    /// Returns the number of bytes needed to store one sample at this bit depth.
    pub fn bytes_per_sample(&self) -> usize {
        bytes_per_sample(self.bits_per_sample)
    }
}

#[derive(Debug)]
pub(crate) struct PcmChunk {
    pub data: Vec<i32>,
    pub frames: u32,
}

impl PcmChunk {
    pub fn new(data: Vec<i32>, frames: u32, channels: u8) -> Self {
        debug_assert_eq!(data.len(), frames as usize * channels as usize);
        Self { data, frames }
    }
}

pub(crate) fn bytes_per_sample(bits_per_sample: u8) -> usize {
    cmp::max(1, ((bits_per_sample as usize) + 7) / 8)
}
