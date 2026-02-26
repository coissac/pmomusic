//! Helpers partagés pour le décodage PCM → AudioSegment
//!
//! Utilisés par `PlaylistSource` et `UriSource`.
//!
//! Les fonctions `bytes_to_segment` et `validate_stream` ont été extraites
//! de `playlist_source.rs` sans modification pour éviter toute duplication.

use std::sync::Arc;

use pmoaudio::{AudioChunk, AudioChunkData, AudioSegment, I24, _AudioSegment, nodes::AudioError};
use pmoflac::StreamInfo;

pub(crate) fn validate_stream(info: &StreamInfo) -> Result<(), AudioError> {
    if !(1..=2).contains(&info.channels) {
        return Err(AudioError::ProcessingError(format!(
            "Unsupported channel count: {}",
            info.channels
        )));
    }
    match info.bits_per_sample {
        8 | 16 | 24 | 32 => Ok(()),
        other => Err(AudioError::ProcessingError(format!(
            "Unsupported bit depth: {}",
            other
        ))),
    }
}

/// Convertit des bytes PCM en AudioSegment avec le type approprié
pub(crate) fn bytes_to_segment(
    chunk_bytes: &[u8],
    info: &StreamInfo,
    frames: usize,
    order: u64,
    timestamp_sec: f64,
) -> Result<Arc<AudioSegment>, AudioError> {
    let bytes_per_sample = info.bytes_per_sample();
    let channels = info.channels as usize;
    let frame_bytes = bytes_per_sample * channels;

    // Créer le chunk du bon type selon la profondeur de bit
    let chunk = match info.bits_per_sample {
        16 => {
            // Type I16
            let mut stereo = Vec::with_capacity(frames);
            for frame_idx in 0..frames {
                let base = frame_idx * frame_bytes;
                let l = i16::from_le_bytes(
                    chunk_bytes[base..base + bytes_per_sample]
                        .try_into()
                        .unwrap(),
                );
                let r = if channels == 1 {
                    l
                } else {
                    i16::from_le_bytes(
                        chunk_bytes[base + bytes_per_sample..base + 2 * bytes_per_sample]
                            .try_into()
                            .unwrap(),
                    )
                };
                stereo.push([l, r]);
            }
            let chunk_data = AudioChunkData::new(stereo, info.sample_rate, 0.0);
            AudioChunk::I16(chunk_data)
        }
        24 => {
            // Type I24
            let mut stereo = Vec::with_capacity(frames);
            for frame_idx in 0..frames {
                let base = frame_idx * frame_bytes;
                let l_i32 = {
                    let mut buf = [0u8; 4];
                    buf[..3].copy_from_slice(&chunk_bytes[base..base + 3]);
                    // Sign extend
                    if chunk_bytes[base + 2] & 0x80 != 0 {
                        buf[3] = 0xFF;
                    }
                    i32::from_le_bytes(buf)
                };
                let l = I24::new(l_i32).ok_or_else(|| {
                    AudioError::ProcessingError(format!("Invalid I24 value: {}", l_i32))
                })?;

                let r = if channels == 1 {
                    l
                } else {
                    let r_i32 = {
                        let mut buf = [0u8; 4];
                        buf[..3].copy_from_slice(
                            &chunk_bytes[base + bytes_per_sample..base + bytes_per_sample + 3],
                        );
                        // Sign extend
                        if chunk_bytes[base + bytes_per_sample + 2] & 0x80 != 0 {
                            buf[3] = 0xFF;
                        }
                        i32::from_le_bytes(buf)
                    };
                    I24::new(r_i32).ok_or_else(|| {
                        AudioError::ProcessingError(format!("Invalid I24 value: {}", r_i32))
                    })?
                };
                stereo.push([l, r]);
            }
            let chunk_data = AudioChunkData::new(stereo, info.sample_rate, 0.0);
            AudioChunk::I24(chunk_data)
        }
        32 => {
            // Type I32
            let mut stereo = Vec::with_capacity(frames);
            for frame_idx in 0..frames {
                let base = frame_idx * frame_bytes;
                let l = i32::from_le_bytes(
                    chunk_bytes[base..base + bytes_per_sample]
                        .try_into()
                        .unwrap(),
                );
                let r = if channels == 1 {
                    l
                } else {
                    i32::from_le_bytes(
                        chunk_bytes[base + bytes_per_sample..base + 2 * bytes_per_sample]
                            .try_into()
                            .unwrap(),
                    )
                };
                stereo.push([l, r]);
            }
            let chunk_data = AudioChunkData::new(stereo, info.sample_rate, 0.0);
            AudioChunk::I32(chunk_data)
        }
        _ => {
            return Err(AudioError::ProcessingError(format!(
                "Unsupported bit depth: {}",
                info.bits_per_sample
            )))
        }
    };

    Ok(Arc::new(AudioSegment {
        order,
        timestamp_sec,
        segment: _AudioSegment::Chunk(Arc::new(chunk)),
    }))
}
