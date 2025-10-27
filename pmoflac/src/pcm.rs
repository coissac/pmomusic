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

/// Basic PCM format used when encoding to FLAC.
#[derive(Debug, Clone, Copy)]
pub struct PcmFormat {
    pub sample_rate: u32,
    pub channels: u8,
    pub bits_per_sample: u8,
}

impl PcmFormat {
    pub fn validate(&self) -> Result<(), String> {
        if self.channels == 0 {
            return Err("channel count must be greater than 0".into());
        }
        if self.channels > 8 {
            return Err("channel count greater than 8 is unsupported".into());
        }
        if self.sample_rate == 0 {
            return Err("sample rate must be greater than 0".into());
        }
        if self.bits_per_sample == 0 || self.bits_per_sample > 32 {
            return Err("bits per sample must be in 1..=32".into());
        }
        Ok(())
    }

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
