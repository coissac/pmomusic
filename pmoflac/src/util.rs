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
