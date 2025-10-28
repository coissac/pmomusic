use crate::{
    nodes::{AudioError, MultiSubscriberNode},
    AudioChunk, BitDepth,
};
use pmoflac::{decode_audio_stream, StreamInfo};
use std::{path::PathBuf, sync::Arc};
use tokio::{fs::File, io::AsyncReadExt, sync::mpsc};

/// FileSource - Lit un fichier audio et publie des `AudioChunk`
///
/// Cette source utilise `pmoflac` pour décoder le fichier (FLAC/MP3/OGG/WAV/AIFF)
/// puis transforme les échantillons PCM en `AudioChunk` stéréo.
pub struct FileSource {
    path: PathBuf,
    chunk_frames: usize,
    subscribers: MultiSubscriberNode,
}

impl FileSource {
    /// Crée une nouvelle source de fichier.
    ///
    /// * `path` - chemin du fichier audio à lire
    /// * `chunk_frames` - nombre d'échantillons par canal par chunk
    pub fn new<P: Into<PathBuf>>(path: P, chunk_frames: usize) -> Self {
        Self {
            path: path.into(),
            chunk_frames: chunk_frames.max(1),
            subscribers: MultiSubscriberNode::new(),
        }
    }

    /// Ajoute un abonné qui recevra les chunks décodés.
    pub fn add_subscriber(&mut self, tx: mpsc::Sender<Arc<AudioChunk>>) {
        self.subscribers.add_subscriber(tx);
    }

    /// Lance la lecture du fichier et diffuse les chunks.
    pub async fn run(self) -> Result<(), AudioError> {
        let file = File::open(&self.path).await.map_err(|e| {
            AudioError::ProcessingError(format!("Failed to open {:?}: {}", self.path, e))
        })?;

        let mut stream = decode_audio_stream(file)
            .await
            .map_err(|e| AudioError::ProcessingError(format!("Decode error: {}", e)))?;
        let stream_info = stream.info().clone();

        validate_stream(&stream_info)?;

        let frame_bytes = stream_info.bytes_per_sample() * stream_info.channels as usize;
        let chunk_byte_len = self.chunk_frames * frame_bytes;
        let mut pending = Vec::new();
        let mut read_buf = vec![0u8; frame_bytes * 512.max(self.chunk_frames)];
        let mut chunk_index = 0u64;

        loop {
            if pending.len() < chunk_byte_len {
                let read = stream.read(&mut read_buf).await.map_err(|e| {
                    AudioError::ProcessingError(format!("I/O error while decoding: {}", e))
                })?;
                if read == 0 {
                    break;
                }
                pending.extend_from_slice(&read_buf[..read]);
            }

            if pending.is_empty() {
                break;
            }

            let frames_in_pending = pending.len() / frame_bytes;
            let frames_to_emit = frames_in_pending.min(self.chunk_frames);
            let take_bytes = frames_to_emit * frame_bytes;
            let chunk_bytes = pending.drain(..take_bytes).collect::<Vec<u8>>();

            let chunk = bytes_to_chunk(&chunk_bytes, &stream_info, frames_to_emit, chunk_index)?;
            self.subscribers.push(chunk).await?;
            chunk_index += 1;
        }

        // Reste éventuel (moins qu'un chunk complet)
        if !pending.is_empty() {
            let frames = pending.len() / frame_bytes;
            if frames > 0 {
                let chunk = bytes_to_chunk(&pending, &stream_info, frames, chunk_index)?;
                self.subscribers.push(chunk).await?;
            }
        }

        stream
            .wait()
            .await
            .map_err(|e| AudioError::ProcessingError(format!("Decode task failed: {}", e)))?;

        Ok(())
    }
}

fn validate_stream(info: &StreamInfo) -> Result<(), AudioError> {
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

fn bytes_to_chunk(
    chunk_bytes: &[u8],
    info: &StreamInfo,
    frames: usize,
    order: u64,
) -> Result<Arc<AudioChunk>, AudioError> {
    let bytes_per_sample = info.bytes_per_sample();
    let channels = info.channels as usize;
    let frame_bytes = bytes_per_sample * channels;

    let mut left = Vec::with_capacity(frames);
    let mut right = Vec::with_capacity(frames);

    for frame_idx in 0..frames {
        let base = frame_idx * frame_bytes;
        let l = sample_to_f32(
            &chunk_bytes[base..base + bytes_per_sample],
            info.bits_per_sample,
        )?;
        let r = if channels == 1 {
            l
        } else {
            sample_to_f32(
                &chunk_bytes[base + bytes_per_sample..base + 2 * bytes_per_sample],
                info.bits_per_sample,
            )?
        };

        left.push(l);
        right.push(r);
    }

    let bit_depth = BitDepth::from_u32_strict(info.bits_per_sample as u32);
    Ok(AudioChunk::from_channels_f32(
        order,
        left,
        right,
        info.sample_rate,
        bit_depth,
    ))
}

fn sample_to_f32(sample_bytes: &[u8], bits: u8) -> Result<f32, AudioError> {
    let sample = match bits {
        8 => i8::from_le_bytes([sample_bytes[0]]) as i32,
        16 => i16::from_le_bytes(sample_bytes.try_into().unwrap()) as i32,
        24 => {
            let mut buf = [0u8; 4];
            buf[..3].copy_from_slice(sample_bytes);
            // Sign extend manually
            if sample_bytes[2] & 0x80 != 0 {
                buf[3] = 0xFF;
            }
            i32::from_le_bytes(buf)
        }
        32 => i32::from_le_bytes(sample_bytes.try_into().unwrap()),
        other => {
            return Err(AudioError::ProcessingError(format!(
                "Unsupported bit depth: {}",
                other
            )))
        }
    };

    let max = ((1i64 << (bits as i64 - 1)).saturating_sub(1)) as f32;
    Ok((sample as f32) / max)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pmoflac::{encode_flac_stream, EncoderOptions, PcmFormat};
    use std::io::Cursor;
    use tokio::io::AsyncWriteExt;
    use tokio::sync::mpsc;

    #[tokio::test]
    async fn test_file_source_decodes_flac() {
        let temp_dir = tempfile::tempdir().unwrap();
        let flac_path = temp_dir.path().join("test.flac");

        let sample_rate = 48_000;
        let frames = 256;
        let mut pcm = Vec::with_capacity(frames * 4);
        for i in 0..frames {
            let sample = ((i % 32) as f32 / 31.0 * 2.0 - 1.0) * 0.5; // simple ramp
            let sample_i16 = (sample * 32767.0) as i16;
            pcm.extend_from_slice(&sample_i16.to_le_bytes());
            pcm.extend_from_slice(&sample_i16.to_le_bytes());
        }

        let format = PcmFormat {
            sample_rate,
            channels: 2,
            bits_per_sample: 16,
        };

        let mut flac_stream =
            encode_flac_stream(Cursor::new(pcm.clone()), format, EncoderOptions::default())
                .await
                .unwrap();

        let mut file = File::create(&flac_path).await.expect("create flac file");
        tokio::io::copy(&mut flac_stream, &mut file)
            .await
            .expect("write flac");
        file.flush().await.expect("flush file");
        flac_stream.wait().await.unwrap();

        let mut source = FileSource::new(&flac_path, 64);
        let (tx, mut rx) = mpsc::channel(4);
        source.add_subscriber(tx);

        tokio::spawn(async move {
            source.run().await.unwrap();
        });

        let mut received = 0usize;
        while let Some(chunk) = rx.recv().await {
            received += chunk.len();
            assert_eq!(chunk.sample_rate(), sample_rate);
            let scale = 1.0 / chunk.bit_depth().max_value();
            if let Some(frame) = chunk.frames().first() {
                assert!(((frame[0] as f32) * scale).abs() <= 1.0); // sample range sanity
            }
        }

        assert_eq!(received, frames);
    }
}
