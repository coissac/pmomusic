use crate::pcm::bytes_per_sample;

pub fn interleaved_i32_to_le_bytes(samples: &[i32], bits_per_sample: u8, out: &mut Vec<u8>) {
    let bytes_per = bytes_per_sample(bits_per_sample);
    out.clear();
    out.reserve(samples.len() * bytes_per);

    for &sample in samples {
        let mut value = sample;
        if bits_per_sample < 32 {
            let shift = 32 - bits_per_sample as u32;
            value = (value << shift) >> shift;
        }
        for i in 0..bytes_per {
            out.push(((value >> (i * 8)) & 0xFF) as u8);
        }
    }
}

pub fn le_bytes_to_interleaved_i32(bytes: &[u8], bits_per_sample: u8) -> Result<Vec<i32>, String> {
    let bytes_per = bytes_per_sample(bits_per_sample);
    if bytes.len() % bytes_per != 0 {
        return Err(format!(
            "PCM byte stream length {} is not aligned to {} bytes/sample",
            bytes.len(),
            bytes_per
        ));
    }

    let mut samples = Vec::with_capacity(bytes.len() / bytes_per);
    let shift = 32 - (bits_per_sample as u32);

    let mut idx = 0;
    while idx < bytes.len() {
        let mut value = 0i32;
        for i in 0..bytes_per {
            value |= (bytes[idx + i] as i32) << (8 * i);
        }
        if bits_per_sample < 32 {
            value = (value << shift) >> shift;
        }
        samples.push(value);
        idx += bytes_per;
    }

    Ok(samples)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_roundtrip_16bit() {
        let samples = vec![0i32, 1000, -1000, i16::MAX as i32, i16::MIN as i32];
        let mut bytes = Vec::new();
        interleaved_i32_to_le_bytes(&samples, 16, &mut bytes);

        assert_eq!(bytes.len(), samples.len() * 2);

        let recovered = le_bytes_to_interleaved_i32(&bytes, 16).unwrap();
        assert_eq!(recovered, samples);
    }

    #[test]
    fn test_roundtrip_24bit() {
        // 24-bit max is 2^23 - 1 = 8388607, min is -2^23 = -8388608
        let samples = vec![0i32, 1000, -1000, 8388607, -8388608];
        let mut bytes = Vec::new();
        interleaved_i32_to_le_bytes(&samples, 24, &mut bytes);

        assert_eq!(bytes.len(), samples.len() * 3);

        let recovered = le_bytes_to_interleaved_i32(&bytes, 24).unwrap();
        assert_eq!(recovered, samples);
    }

    #[test]
    fn test_roundtrip_32bit() {
        let samples = vec![0i32, 1000, -1000, i32::MAX, i32::MIN];
        let mut bytes = Vec::new();
        interleaved_i32_to_le_bytes(&samples, 32, &mut bytes);

        assert_eq!(bytes.len(), samples.len() * 4);

        let recovered = le_bytes_to_interleaved_i32(&bytes, 32).unwrap();
        assert_eq!(recovered, samples);
    }

    #[test]
    fn test_roundtrip_8bit() {
        // 8-bit signed: -128 to 127
        let samples = vec![0i32, 100, -100, 127, -128];
        let mut bytes = Vec::new();
        interleaved_i32_to_le_bytes(&samples, 8, &mut bytes);

        assert_eq!(bytes.len(), samples.len());

        let recovered = le_bytes_to_interleaved_i32(&bytes, 8).unwrap();
        assert_eq!(recovered, samples);
    }

    #[test]
    fn test_sign_extension_16bit() {
        // Test that sign extension works correctly for 16-bit
        let sample = -1i32; // Should be 0xFFFF in 16-bit
        let mut bytes = Vec::new();
        interleaved_i32_to_le_bytes(&[sample], 16, &mut bytes);

        assert_eq!(bytes.len(), 2);
        assert_eq!(bytes[0], 0xFF);
        assert_eq!(bytes[1], 0xFF);

        let recovered = le_bytes_to_interleaved_i32(&bytes, 16).unwrap();
        assert_eq!(recovered[0], -1);
    }

    #[test]
    fn test_sign_extension_24bit() {
        // Test that sign extension works correctly for 24-bit
        let sample = -1i32;
        let mut bytes = Vec::new();
        interleaved_i32_to_le_bytes(&[sample], 24, &mut bytes);

        assert_eq!(bytes.len(), 3);
        assert_eq!(bytes[0], 0xFF);
        assert_eq!(bytes[1], 0xFF);
        assert_eq!(bytes[2], 0xFF);

        let recovered = le_bytes_to_interleaved_i32(&bytes, 24).unwrap();
        assert_eq!(recovered[0], -1);
    }

    #[test]
    fn test_misaligned_bytes_error() {
        // 16-bit samples need even number of bytes
        let bytes = vec![0, 1, 2]; // 3 bytes, not aligned to 2
        let result = le_bytes_to_interleaved_i32(&bytes, 16);

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .contains("not aligned to 2 bytes/sample"));
    }

    #[test]
    fn test_empty_samples() {
        let samples: Vec<i32> = vec![];
        let mut bytes = Vec::new();
        interleaved_i32_to_le_bytes(&samples, 16, &mut bytes);

        assert_eq!(bytes.len(), 0);

        let recovered = le_bytes_to_interleaved_i32(&bytes, 16).unwrap();
        assert_eq!(recovered.len(), 0);
    }

    #[test]
    fn test_stereo_interleaved_16bit() {
        // Simulate stereo: L, R, L, R
        let samples = vec![1000i32, 2000, 3000, 4000];
        let mut bytes = Vec::new();
        interleaved_i32_to_le_bytes(&samples, 16, &mut bytes);

        assert_eq!(bytes.len(), 8); // 4 samples * 2 bytes

        let recovered = le_bytes_to_interleaved_i32(&bytes, 16).unwrap();
        assert_eq!(recovered, samples);
    }

    #[test]
    fn test_value_truncation_overflow() {
        // Test that values outside the valid range for a bit depth
        // are properly truncated via sign extension
        let huge_value = i32::MAX; // Way beyond 16-bit range
        let mut bytes = Vec::new();
        interleaved_i32_to_le_bytes(&[huge_value], 16, &mut bytes);

        let recovered = le_bytes_to_interleaved_i32(&bytes, 16).unwrap();
        // The value should be truncated to 16-bit and sign-extended
        assert_eq!(recovered[0], -1); // 0xFFFF sign-extended
    }

    #[test]
    fn test_multiple_bit_depths() {
        for bits in [8, 16, 24, 32] {
            let samples = vec![0i32, 100, -100];
            let mut bytes = Vec::new();
            interleaved_i32_to_le_bytes(&samples, bits, &mut bytes);

            let expected_bytes = samples.len() * bytes_per_sample(bits);
            assert_eq!(bytes.len(), expected_bytes);

            let recovered = le_bytes_to_interleaved_i32(&bytes, bits).unwrap();
            assert_eq!(recovered, samples);
        }
    }
}
