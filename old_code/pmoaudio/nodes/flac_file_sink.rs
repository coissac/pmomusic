use crate::{nodes::AudioError, AudioChunk};
use pmoflac::{encode_flac_stream, EncoderOptions, PcmFormat};
use std::{
    collections::VecDeque,
    path::PathBuf,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};
use tokio::{
    fs::File,
    io::{self, AsyncRead, AsyncWriteExt, ReadBuf},
    sync::mpsc,
};

/// Sink qui encode les `AudioChunk` reçus au format FLAC.
pub struct FlacFileSink {
    rx: mpsc::Receiver<Arc<AudioChunk>>,
    path: PathBuf,
    encoder_options: EncoderOptions,
    pcm_buffer_capacity: usize,
}

impl FlacFileSink {
    /// Crée un sink FLAC avec les options par défaut (compression 5).
    pub fn new<P: Into<PathBuf>>(
        path: P,
        channel_size: usize,
    ) -> (Self, mpsc::Sender<Arc<AudioChunk>>) {
        Self::with_options(path, channel_size, EncoderOptions::default())
    }

    /// Crée un sink FLAC avec des options explicites.
    pub fn with_options<P: Into<PathBuf>>(
        path: P,
        channel_size: usize,
        encoder_options: EncoderOptions,
    ) -> (Self, mpsc::Sender<Arc<AudioChunk>>) {
        let (tx, rx) = mpsc::channel(channel_size);
        let sink = Self {
            rx,
            path: path.into(),
            encoder_options,
            pcm_buffer_capacity: 8,
        };
        (sink, tx)
    }

    /// Lance l'encodage vers le fichier cible.
    pub async fn run(self) -> Result<FlacFileSinkStats, AudioError> {
        let FlacFileSink {
            mut rx,
            path,
            encoder_options,
            pcm_buffer_capacity,
        } = self;

        let first_chunk = rx.recv().await.ok_or_else(|| {
            AudioError::ProcessingError("FlacFileSink: no audio data received".into())
        })?;

        if first_chunk.len() == 0 {
            return Err(AudioError::ProcessingError(
                "FlacFileSink: received empty chunk".into(),
            ));
        }

        let format = PcmFormat {
            sample_rate: first_chunk.sample_rate(),
            channels: 2,
            bits_per_sample: 16,
        };
        if let Err(err) = format.validate() {
            return Err(AudioError::ProcessingError(format!(
                "Invalid PCM format: {}",
                err
            )));
        }

        let (pcm_tx, pcm_rx) = mpsc::channel::<Vec<u8>>(pcm_buffer_capacity);
        let pump_handle = tokio::spawn(pump_chunks(first_chunk, rx, pcm_tx));

        let reader = ByteStreamReader::new(pcm_rx);
        let mut flac_stream = encode_flac_stream(reader, format, encoder_options)
            .await
            .map_err(|e| AudioError::ProcessingError(format!("FLAC encode init failed: {}", e)))?;

        let mut output = File::create(&path).await.map_err(|e| {
            AudioError::ProcessingError(format!("Failed to create {:?}: {}", path, e))
        })?;

        tokio::io::copy(&mut flac_stream, &mut output)
            .await
            .map_err(|e| AudioError::ProcessingError(format!("FLAC write failed: {}", e)))?;
        output.flush().await.map_err(|e| {
            AudioError::ProcessingError(format!("Failed to flush {:?}: {}", path, e))
        })?;

        flac_stream
            .wait()
            .await
            .map_err(|e| AudioError::ProcessingError(format!("FLAC encoder task failed: {}", e)))?;

        let pump_stats = pump_handle
            .await
            .map_err(|e| AudioError::ProcessingError(format!("Pump task panicked: {}", e)))??;

        Ok(FlacFileSinkStats {
            path,
            chunks_received: pump_stats.chunks,
            total_samples: pump_stats.samples,
            total_duration_sec: pump_stats.duration_sec,
        })
    }
}

struct PumpStats {
    chunks: u64,
    samples: u64,
    duration_sec: f64,
}

async fn pump_chunks(
    first_chunk: Arc<AudioChunk>,
    mut rx: mpsc::Receiver<Arc<AudioChunk>>,
    pcm_tx: mpsc::Sender<Vec<u8>>,
) -> Result<PumpStats, AudioError> {
    let mut chunks = 0u64;
    let mut samples = 0u64;
    let mut duration_sec = 0.0f64;
    let expected_rate = first_chunk.sample_rate();

    let mut current = Some(first_chunk);

    loop {
        let chunk_opt = if let Some(ch) = current.take() {
            Some(ch)
        } else {
            rx.recv().await
        };

        let chunk = match chunk_opt {
            Some(ch) => ch,
            None => break,
        };

        if chunk.sample_rate() != expected_rate {
            return Err(AudioError::ProcessingError(format!(
                "FlacFileSink: inconsistent sample rate ({} vs {})",
                chunk.sample_rate(),
                expected_rate
            )));
        }

        let pcm_bytes = chunk_to_pcm_bytes(&chunk);
        if pcm_bytes.is_empty() {
            continue;
        }

        pcm_tx
            .send(pcm_bytes)
            .await
            .map_err(|_| AudioError::SendError)?;

        chunks += 1;
        samples += chunk.len() as u64;
        duration_sec += chunk.len() as f64 / expected_rate as f64;
    }

    Ok(PumpStats {
        chunks,
        samples,
        duration_sec,
    })
}

fn chunk_to_pcm_bytes(chunk: &AudioChunk) -> Vec<u8> {
    let len = chunk.len();
    let mut bytes = Vec::with_capacity(len * 4);
    let gain = chunk.gain_linear() as f32;
    let scale = 1.0f32 / chunk.bit_depth().max_value();
    for frame in chunk.frames() {
        let left = (frame[0] as f32 * scale * gain).clamp(-1.0, 1.0);
        let right = (frame[1] as f32 * scale * gain).clamp(-1.0, 1.0);
        let left_i16 = (left * 32767.0) as i16;
        let right_i16 = (right * 32767.0) as i16;
        bytes.extend_from_slice(&left_i16.to_le_bytes());
        bytes.extend_from_slice(&right_i16.to_le_bytes());
    }
    bytes
}

struct ByteStreamReader {
    rx: mpsc::Receiver<Vec<u8>>,
    buffer: VecDeque<u8>,
    finished: bool,
}

impl ByteStreamReader {
    fn new(rx: mpsc::Receiver<Vec<u8>>) -> Self {
        Self {
            rx,
            buffer: VecDeque::new(),
            finished: false,
        }
    }
}

impl AsyncRead for ByteStreamReader {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        loop {
            if !self.buffer.is_empty() {
                let to_copy = self.buffer.len().min(buf.remaining());
                if to_copy == 0 {
                    return Poll::Ready(Ok(()));
                }

                // VecDeque::make_contiguous pour copier efficacement
                let slice = self.buffer.make_contiguous();
                buf.put_slice(&slice[..to_copy]);
                self.buffer.drain(..to_copy);
                return Poll::Ready(Ok(()));
            }

            if self.finished {
                return Poll::Ready(Ok(()));
            }

            match Pin::new(&mut self.rx).poll_recv(cx) {
                Poll::Ready(Some(bytes)) => {
                    if bytes.is_empty() {
                        continue;
                    }
                    self.buffer.extend(bytes);
                }
                Poll::Ready(None) => {
                    self.finished = true;
                    return Poll::Ready(Ok(()));
                }
                Poll::Pending => return Poll::Pending,
            }
        }
    }
}

/// Statistiques produites par le `FlacFileSink`.
#[derive(Debug, Clone)]
pub struct FlacFileSinkStats {
    pub path: PathBuf,
    pub chunks_received: u64,
    pub total_samples: u64,
    pub total_duration_sec: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::BitDepth;
    use pmoflac::decode_flac_stream;
    use tokio::io::AsyncReadExt;

    #[tokio::test]
    async fn test_flac_file_sink_writes_audio() {
        let temp_dir = tempfile::tempdir().unwrap();
        let output_path = temp_dir.path().join("output.flac");

        let (sink, tx) = FlacFileSink::new(&output_path, 8);
        let handle = tokio::spawn(async move { sink.run().await.unwrap() });

        let chunk = AudioChunk::from_channels_f32(
            0,
            vec![0.25; 256],
            vec![0.5; 256],
            44_100,
            BitDepth::B24,
        );
        tx.send(chunk).await.unwrap();

        drop(tx);

        let stats = handle.await.unwrap();
        assert_eq!(stats.chunks_received, 1);
        assert_eq!(stats.total_samples, 256);

        let file = File::open(&output_path).await.unwrap();
        let mut stream = decode_flac_stream(file).await.unwrap();
        let info = stream.info().clone();
        assert_eq!(info.channels, 2);
        assert_eq!(info.sample_rate, 44_100);

        let mut decoded = Vec::new();
        stream.read_to_end(&mut decoded).await.unwrap();
        stream.wait().await.unwrap();
        assert_eq!(decoded.len(), 256 * 4); // 256 frames * 2 channels * 2 bytes
    }
}
