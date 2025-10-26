use anyhow::Result;
use bytes::Bytes;
use futures::stream::Stream;
use std::io::{self, Read};
use std::pin::Pin;
use std::sync::mpsc::{sync_channel, Receiver, RecvError, SyncSender};
use std::time::{Duration, Instant};

const CHANNEL_BUFFER_SIZE: usize = 16;
pub const CHUNK_SIZE_FRAMES: usize = 4096;

pub struct ChannelReader {
    receiver: Receiver<Result<Bytes, String>>,
    current_chunk: Option<Bytes>,
    position: usize,
}

impl ChannelReader {
    pub fn new(
        stream: Pin<Box<dyn Stream<Item = Result<Bytes, crate::error::Error>> + Send>>,
    ) -> Self {
        let (tx, rx) = sync_channel(CHANNEL_BUFFER_SIZE);
        tokio::spawn(Self::stream_feeder(stream, tx));
        Self {
            receiver: rx,
            current_chunk: None,
            position: 0,
        }
    }

    async fn stream_feeder(
        mut stream: Pin<Box<dyn Stream<Item = Result<Bytes, crate::error::Error>> + Send>>,
        tx: SyncSender<Result<Bytes, String>>,
    ) {
        use futures::StreamExt;
        while let Some(result) = stream.next().await {
            let start = Instant::now();

            let to_send = result.map_err(|e| e.to_string());
            match tx.try_send(to_send) {
                Ok(_) => { /* message envoyé sans attente */ }
                Err(std::sync::mpsc::TrySendError::Full(value)) => {
                    tracing::warn!("stream_feeder: buffer plein");
                    // Revenir à l’envoi bloquant pour ne pas perdre le message
                    if tx.send(value).is_err() {
                        break;
                    }
                }
                Err(std::sync::mpsc::TrySendError::Disconnected(_)) => break,
            }
            let waited = start.elapsed();
            tracing::trace!("stream_feeder send {:?}", waited);
            if waited > Duration::from_millis(200) {
                tracing::warn!("stream_feeder wait {:?}", waited);
            }
        }
    }
}

impl Read for ChannelReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let start = Instant::now();
        loop {
            if let Some(chunk) = &self.current_chunk {
                if self.position < chunk.len() {
                    let available = chunk.len() - self.position;
                    let to_copy = available.min(buf.len());
                    buf[..to_copy].copy_from_slice(&chunk[self.position..self.position + to_copy]);
                    self.position += to_copy;
                     tracing::trace!(
                         "ChannelReader copied {} bytes (elapsed {:?})",
                         to_copy,
                         start.elapsed()
                     );
                    return Ok(to_copy);
                }
            }

            match self.receiver.recv() {
                Ok(Ok(bytes)) => {
                    tracing::trace!(
                        "ChannelReader received chunk of {} bytes after {:?}",
                        bytes.len(),
                        start.elapsed()
                    );
                    self.current_chunk = Some(bytes);
                    self.position = 0;
                }
                Ok(Err(e)) => {
                    tracing::warn!("ChannelReader received error chunk: {}", e);
                    return Err(io::Error::new(io::ErrorKind::Other, e));
                }
                Err(RecvError) => {
                    tracing::trace!(
                        "ChannelReader stream closed after {:?}",
                        start.elapsed()
                    );
                    return Ok(0);
                }
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct PCMChunk {
    pub samples: Vec<i32>,
    pub position_ms: u64,
    pub sample_rate: u32,
    pub channels: u32,
}

pub struct StreamingPCMDecoder<R: Read> {
    reader: claxon::FlacReader<std::io::BufReader<R>>,
    sample_rate: u32,
    channels: u32,
    bits_per_sample: u32,
    total_samples_decoded: u64,
    done: bool,
}

impl StreamingPCMDecoder<ChannelReader> {
    /// Create a new decoder from an HTTP stream with default chunk size
    pub fn new(http_stream: crate::stream::BlockStream) -> anyhow::Result<Self> {
        Self::with_chunk_size(http_stream, CHUNK_SIZE_FRAMES)
    }

    pub fn with_chunk_size(
        http_stream: crate::stream::BlockStream,
        _chunk_size: usize,
    ) -> anyhow::Result<Self> {
        let channel_reader = ChannelReader::new(http_stream.into_inner());
        let buffered = std::io::BufReader::new(channel_reader);
        let reader = claxon::FlacReader::new(buffered)
            .map_err(|e| anyhow::anyhow!("FLAC reader error: {}", e))?;
        let info = reader.streaminfo();

        Ok(Self {
            reader,
            sample_rate: info.sample_rate,
            channels: info.channels,
            bits_per_sample: info.bits_per_sample,
            total_samples_decoded: 0,
            done: false,
        })
    }

    /// Get the sample rate (e.g., 44100 Hz)
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Get the number of channels (e.g., 2 for stereo)
    pub fn channels(&self) -> u32 {
        self.channels
    }

    /// Get bits per sample (e.g., 16)
    pub fn bits_per_sample(&self) -> u32 {
        self.bits_per_sample
    }

    pub fn decode_chunk(&mut self) -> anyhow::Result<Option<PCMChunk>> {
        if self.done {
            return Ok(None);
        }

        // Crée le FrameReader à la volée (emprunt de self.reader)
        let mut frames = self.reader.blocks();

        // API claxon 0.6.x : il FAUT fournir un Vec<i32> par valeur
        let buf: Vec<i32> = Vec::new();
        let frame = match frames.read_next_or_eof(buf) {
            Ok(None) => {
                self.done = true;
                return Ok(None);
            }
            Ok(Some(f)) => f,
            Err(e) => return Err(anyhow::anyhow!("FLAC decode error: {}", e)),
        };

        let samples: Vec<i32> = frame.into_buffer();
        if samples.is_empty() {
            self.done = true;
            return Ok(None);
        }

        let position_ms = {
            let frames = self.total_samples_decoded / self.channels as u64;
            (frames * 1000) / self.sample_rate as u64
        };
        self.total_samples_decoded += samples.len() as u64;

        Ok(Some(PCMChunk {
            samples,
            position_ms,
            sample_rate: self.sample_rate,
            channels: self.channels,
        }))
    }
}

pub fn ms_to_frames(ms: u64, sample_rate: u32) -> usize {
    ((ms as u128 * sample_rate as u128) / 1000) as usize
}

pub fn frames_to_ms(frames: usize, sample_rate: u32) -> u64 {
    ((frames as u128 * 1000) / sample_rate as u128) as u64
}
