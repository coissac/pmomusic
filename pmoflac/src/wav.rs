//! # WAV (RIFF) Decoder Module
//!
//! Streaming WAV → PCM conversion with zero seeking. The decoder reads the RIFF
//! header incrementally, validates the format, and then streams `data` chunk
//! payload as little-endian PCM frames through the common async pipeline.

use std::{
    fmt,
    io::{self, Read},
    pin::Pin,
    task::{Context, Poll},
};

use tokio::{
    io::AsyncRead,
    sync::{mpsc, oneshot},
};

use crate::{
    common::ChannelReader,
    decoder_common::{
        spawn_ingest_task, spawn_writer_task, CHANNEL_CAPACITY, DUPLEX_BUFFER_SIZE,
    },
    pcm::StreamInfo,
    stream::ManagedAsyncReader,
};

/// Errors that can occur while decoding WAV data.
#[derive(thiserror::Error, Debug, Clone)]
pub enum WavError {
    #[error("I/O error ({kind:?}): {message}")]
    Io {
        kind: io::ErrorKind,
        message: String,
    },
    #[error("WAV decode error: {0}")]
    Decode(String),
    #[error("internal channel closed unexpectedly")]
    ChannelClosed,
}

impl From<io::Error> for WavError {
    fn from(err: io::Error) -> Self {
        WavError::Io {
            kind: err.kind(),
            message: err.to_string(),
        }
    }
}

impl From<String> for WavError {
    fn from(value: String) -> Self {
        WavError::Decode(value)
    }
}

/// Streaming WAV reader state.
struct StreamingWavReader<E>
where
    E: fmt::Display + std::error::Error,
{
    reader: ChannelReader<E>,
    buffer: Vec<u8>,
    position: usize,
    finished: bool,
}

impl<E> StreamingWavReader<E>
where
    E: fmt::Display + std::error::Error,
{
    fn new(reader: ChannelReader<E>) -> Self {
        Self {
            reader,
            buffer: Vec::new(),
            position: 0,
            finished: false,
        }
    }

    fn read_exact(&mut self, len: usize) -> Result<&[u8], WavError> {
        while self.buffer.len() - self.position < len {
            if self.finished {
                return Err(WavError::Decode("unexpected EOF in WAV header".into()));
            }
            let mut chunk = [0u8; 4096];
            let read = self.reader.read(&mut chunk)?;
            if read == 0 {
                self.finished = true;
            } else {
                self.buffer.extend_from_slice(&chunk[..read]);
            }
        }
        let start = self.position;
        let end = start + len;
        self.position = end;
        Ok(&self.buffer[start..end])
    }

    fn skip(&mut self, mut len: usize) -> Result<(), WavError> {
        while len > 0 {
            let available = self.buffer.len() - self.position;
            if available >= len {
                self.position += len;
                return Ok(());
            } else {
                self.position += available;
                len -= available;
                let mut chunk = [0u8; 4096];
                let read = self.reader.read(&mut chunk)?;
                if read == 0 {
                    return Err(WavError::Decode("unexpected EOF while skipping chunk".into()));
                }
                self.buffer.clear();
                self.buffer.extend_from_slice(&chunk[..read]);
                self.position = 0;
            }
        }
        Ok(())
    }

}

/// PCM format metadata extracted from the WAV `fmt ` chunk.
#[derive(Clone, Debug)]
struct FmtChunk {
    audio_format: u16,
    channels: u16,
    sample_rate: u32,
    bits_per_sample: u16,
}

impl FmtChunk {
    fn validate(&self) -> Result<(), WavError> {
        if !(self.audio_format == 0x0001 || self.audio_format == 0x0003) {
            return Err(WavError::Decode(format!(
                "unsupported WAV audio format: {}",
                self.audio_format
            )));
        }
        if self.channels == 0 {
            return Err(WavError::Decode("WAV channel count must be > 0".into()));
        }
        if self.sample_rate == 0 {
            return Err(WavError::Decode("WAV sample rate must be > 0".into()));
        }
        if self.bits_per_sample == 0 || self.bits_per_sample > 32 {
            return Err(WavError::Decode(format!(
                "unsupported bits per sample: {}",
                self.bits_per_sample
            )));
        }
        if self.audio_format == 0x0001 {
            match self.bits_per_sample {
                8 | 16 | 24 | 32 => Ok(()),
                _ => Err(WavError::Decode(format!(
                    "unsupported PCM bit depth: {}",
                    self.bits_per_sample
                ))),
            }
        } else {
            Err(WavError::Decode(
                "IEEE float WAV decoding is not yet supported".into(),
            ))
        }
    }

    fn bytes_per_sample(&self) -> usize {
        ((self.bits_per_sample as usize) + 7) / 8
    }
}

/// An async stream that yields PCM decoded from WAV data.
pub struct WavDecodedStream {
    info: StreamInfo,
    reader: ManagedAsyncReader<WavError>,
}

impl WavDecodedStream {
    pub fn info(&self) -> &StreamInfo {
        &self.info
    }

    pub fn into_parts(self) -> (StreamInfo, ManagedAsyncReader<WavError>) {
        (self.info, self.reader)
    }

    pub async fn wait(self) -> Result<(), WavError> {
        self.reader.wait().await
    }
}

impl AsyncRead for WavDecodedStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.reader).poll_read(cx, buf)
    }
}

/// Decode a WAV stream into PCM audio.
pub async fn decode_wav_stream<R>(reader: R) -> Result<WavDecodedStream, WavError>
where
    R: AsyncRead + Unpin + Send + 'static,
{
    let (ingest_tx, ingest_rx) = mpsc::channel(CHANNEL_CAPACITY);
    spawn_ingest_task::<_, WavError>(reader, ingest_tx);

    let (pcm_tx, pcm_rx) = mpsc::channel(CHANNEL_CAPACITY);
    let (pcm_reader, pcm_writer) = tokio::io::duplex(DUPLEX_BUFFER_SIZE);
    let (info_tx, info_rx) = oneshot::channel::<Result<StreamInfo, WavError>>();

    let blocking_handle = tokio::task::spawn_blocking(move || -> Result<(), WavError> {
        let channel_reader = ChannelReader::<WavError>::new(ingest_rx);
        let mut wav_reader = StreamingWavReader::new(channel_reader);

        let riff = wav_reader.read_exact(12)?;
        if &riff[0..4] != b"RIFF" {
            return Err(WavError::Decode("missing RIFF header".into()));
        }
        if &riff[8..12] != b"WAVE" {
            return Err(WavError::Decode("missing WAVE signature".into()));
        }

        let mut fmt_chunk: Option<FmtChunk> = None;
        let mut data_found = false;

        loop {
            let mut chunk_header = [0u8; 8];
            match wav_reader.read_exact(8) {
                Ok(bytes) => chunk_header.copy_from_slice(bytes),
                Err(WavError::Decode(msg)) if msg.contains("unexpected EOF") => break,
                Err(err) => return Err(err),
            }
            let chunk_id = &chunk_header[..4];
            let chunk_size = u32::from_le_bytes([
                chunk_header[4],
                chunk_header[5],
                chunk_header[6],
                chunk_header[7],
            ]) as usize;

            let padded_size = (chunk_size + 1) & !1; // align to even bytes

            match chunk_id {
                b"fmt " => {
                    let bytes = wav_reader.read_exact(chunk_size)?;
                    if chunk_size < 16 {
                        return Err(WavError::Decode("fmt chunk too small".into()));
                    }
                    let audio_format = u16::from_le_bytes([bytes[0], bytes[1]]);
                    let channels = u16::from_le_bytes([bytes[2], bytes[3]]);
                    let sample_rate =
                        u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
                    let bits_per_sample = u16::from_le_bytes([bytes[14], bytes[15]]);
                    let fmt = FmtChunk {
                        audio_format,
                        channels,
                        sample_rate,
                        bits_per_sample,
                    };
                    fmt.validate()?;
                    fmt_chunk = Some(fmt);
                    if padded_size > chunk_size {
                        wav_reader.skip(padded_size - chunk_size)?;
                    }
                }
                b"data" => {
                    let fmt = fmt_chunk
                        .as_ref()
                        .ok_or_else(|| WavError::Decode("data chunk before fmt chunk".into()))?;

                    let info = StreamInfo {
                        sample_rate: fmt.sample_rate,
                        channels: fmt.channels as u8,
                        bits_per_sample: fmt.bits_per_sample as u8,
                        total_samples: None,
                        max_block_size: 0,
                        min_block_size: 0,
                    };

                    if info_tx.send(Ok(info.clone())).is_err() {
                        return Ok(());
                    }

                    let mut remaining = chunk_size;
                    let bytes_per_frame = fmt.bytes_per_sample() * fmt.channels as usize;
                    let mut buffer = vec![0u8; 4096];

                    while remaining > 0 {
                        let to_read = remaining.min(buffer.len());
                        let read = wav_reader.reader.read(&mut buffer[..to_read])?;
                        if read == 0 {
                            break;
                        }
                        remaining -= read;
                        let aligned = read - (read % bytes_per_frame);
                        if aligned > 0 {
                            if pcm_tx
                                .blocking_send(Ok(buffer[..aligned].to_vec()))
                                .is_err()
                            {
                                return Ok(());
                            }
                        }
                        if aligned < read {
                            return Err(WavError::Decode(
                                "incomplete frame at end of chunk".into(),
                            ));
                        }
                    }

                    if padded_size > chunk_size {
                        let mut pad = [0u8; 1];
                        wav_reader.reader.read_exact(&mut pad)?;
                    }

                    data_found = true;
                    break;
                }
                _ => {
                    wav_reader.skip(chunk_size)?;
                    if padded_size > chunk_size {
                        wav_reader.skip(padded_size - chunk_size)?;
                    }
                }
            }
        }

        if !data_found {
            return Err(WavError::Decode("no data chunk found in WAV stream".into()));
        }

        Ok(())
    });

    let writer_handle = spawn_writer_task(pcm_rx, pcm_writer, blocking_handle, "wav-decode");

    let info = info_rx.await.map_err(|_| WavError::ChannelClosed)??;
    let reader = ManagedAsyncReader::new("wav-decode-writer", pcm_reader, writer_handle);

    Ok(WavDecodedStream { info, reader })
}
