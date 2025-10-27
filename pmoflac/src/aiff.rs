//! # AIFF Decoder Module
//!
//! Streaming AIFF (Audio Interchange File Format) to PCM conversion without any
//! seeking. The decoder parses the FORM/COMM/SSND chunks incrementally and emits
//! little-endian interleaved PCM frames compatible with the rest of the
//! pipeline.

use std::{
    collections::VecDeque,
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
    decoder_common::{spawn_ingest_task, spawn_writer_task, CHANNEL_CAPACITY, DUPLEX_BUFFER_SIZE},
    pcm::StreamInfo,
    stream::ManagedAsyncReader,
};

/// Errors that can occur while decoding AIFF data.
#[derive(thiserror::Error, Debug, Clone)]
pub enum AiffError {
    #[error("I/O error ({kind:?}): {message}")]
    Io {
        kind: io::ErrorKind,
        message: String,
    },
    #[error("AIFF decode error: {0}")]
    Decode(String),
    #[error("internal channel closed unexpectedly")]
    ChannelClosed,
}

impl From<io::Error> for AiffError {
    fn from(err: io::Error) -> Self {
        AiffError::Io {
            kind: err.kind(),
            message: err.to_string(),
        }
    }
}

impl From<String> for AiffError {
    fn from(value: String) -> Self {
        AiffError::Decode(value)
    }
}

/// Streaming reader that buffers bytes as they arrive and exposes convenience helpers.
struct StreamingAiffReader<E>
where
    E: fmt::Display + std::error::Error,
{
    reader: ChannelReader<E>,
    buffer: VecDeque<u8>,
    finished: bool,
}

impl<E> StreamingAiffReader<E>
where
    E: fmt::Display + std::error::Error,
{
    fn new(reader: ChannelReader<E>) -> Self {
        Self {
            reader,
            buffer: VecDeque::new(),
            finished: false,
        }
    }

    fn fill_buffer(&mut self, len: usize) -> Result<(), AiffError> {
        while self.buffer.len() < len {
            if self.finished {
                break;
            }
            let mut chunk = [0u8; 4096];
            let read = self.reader.read(&mut chunk)?;
            if read == 0 {
                self.finished = true;
            } else {
                self.buffer.extend(&chunk[..read]);
            }
        }
        Ok(())
    }

    fn read_exact_vec(&mut self, len: usize) -> Result<Vec<u8>, AiffError> {
        self.fill_buffer(len)?;
        if self.buffer.len() < len {
            return Err(AiffError::Decode("unexpected EOF in AIFF stream".into()));
        }
        let mut out = Vec::with_capacity(len);
        for _ in 0..len {
            out.push(self.buffer.pop_front().unwrap());
        }
        Ok(out)
    }

    fn skip(&mut self, mut len: usize) -> Result<(), AiffError> {
        while len > 0 {
            if !self.buffer.is_empty() {
                let take = len.min(self.buffer.len());
                for _ in 0..take {
                    self.buffer.pop_front();
                }
                len -= take;
                continue;
            }
            let mut chunk = [0u8; 4096];
            let read = self.reader.read(&mut chunk)?;
            if read == 0 {
                return Err(AiffError::Decode(
                    "unexpected EOF while skipping chunk".into(),
                ));
            }
            self.buffer.extend(&chunk[..read]);
        }
        Ok(())
    }
}

/// Compression / endianness mode for AIFF data.
#[derive(Clone, Copy, Debug)]
enum Compression {
    BigEndianPcm,
    LittleEndianPcm,
}

/// Parsed COMM chunk data.
#[derive(Clone, Debug)]
struct CommChunk {
    channels: u16,
    num_frames: u32,
    bits_per_sample: u16,
    sample_rate: u32,
    compression: Compression,
}

impl CommChunk {
    fn bytes_per_sample(&self) -> usize {
        ((self.bits_per_sample as usize) + 7) / 8
    }

    fn validate(&self) -> Result<(), AiffError> {
        if self.channels == 0 {
            return Err(AiffError::Decode("AIFF channel count must be > 0".into()));
        }
        if self.sample_rate == 0 {
            return Err(AiffError::Decode("AIFF sample rate must be > 0".into()));
        }
        match self.bytes_per_sample() {
            1 | 2 | 3 | 4 => Ok(()),
            other => Err(AiffError::Decode(format!(
                "unsupported AIFF bytes per sample: {}",
                other
            ))),
        }
    }
}

/// Asynchronous decoded stream wrapper for AIFF data.
pub struct AiffDecodedStream {
    info: StreamInfo,
    reader: ManagedAsyncReader<AiffError>,
}

impl AiffDecodedStream {
    pub fn info(&self) -> &StreamInfo {
        &self.info
    }

    pub fn into_parts(self) -> (StreamInfo, ManagedAsyncReader<AiffError>) {
        (self.info, self.reader)
    }

    pub async fn wait(self) -> Result<(), AiffError> {
        self.reader.wait().await
    }
}

impl AsyncRead for AiffDecodedStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.reader).poll_read(cx, buf)
    }
}

/// Decode an AIFF stream into PCM audio (little-endian interleaved).
pub async fn decode_aiff_stream<R>(reader: R) -> Result<AiffDecodedStream, AiffError>
where
    R: AsyncRead + Unpin + Send + 'static,
{
    let (ingest_tx, ingest_rx) = mpsc::channel(CHANNEL_CAPACITY);
    spawn_ingest_task::<_, AiffError>(reader, ingest_tx);

    let (pcm_tx, pcm_rx) = mpsc::channel(CHANNEL_CAPACITY);
    let (pcm_reader, pcm_writer) = tokio::io::duplex(DUPLEX_BUFFER_SIZE);
    let (info_tx, info_rx) = oneshot::channel::<Result<StreamInfo, AiffError>>();

    let blocking_handle = tokio::task::spawn_blocking(move || -> Result<(), AiffError> {
        let channel_reader = ChannelReader::<AiffError>::new(ingest_rx);
        let mut aiff_reader = StreamingAiffReader::new(channel_reader);

        // Parse FORM header
        let form_header = aiff_reader.read_exact_vec(12)?;
        if &form_header[0..4] != b"FORM" {
            return Err(AiffError::Decode("missing FORM header".into()));
        }
        let form_type = <[u8; 4]>::try_from(&form_header[8..12]).unwrap();
        if form_type != *b"AIFF" && form_type != *b"AIFC" {
            return Err(AiffError::Decode(
                "unsupported FORM type (expected AIFF/AIFC)".into(),
            ));
        }

        let mut comm_chunk: Option<CommChunk> = None;
        let mut stream_info_sent = false;

        loop {
            let header = match aiff_reader.read_exact_vec(8) {
                Ok(bytes) => bytes,
                Err(AiffError::Decode(msg)) if msg.contains("unexpected EOF") => break,
                Err(err) => return Err(err),
            };
            let chunk_id = <[u8; 4]>::try_from(&header[..4]).unwrap();
            let chunk_size =
                u32::from_be_bytes([header[4], header[5], header[6], header[7]]) as usize;

            let padded_size = if chunk_size % 2 == 0 {
                chunk_size
            } else {
                chunk_size + 1
            };

            match &chunk_id {
                b"COMM" => {
                    let data = aiff_reader.read_exact_vec(chunk_size)?;
                    if form_type == *b"AIFF" && data.len() < 18 {
                        return Err(AiffError::Decode("COMM chunk too small".into()));
                    }
                    if data.len() < 18 {
                        return Err(AiffError::Decode("COMM chunk too small for AIFC".into()));
                    }
                    let channels = u16::from_be_bytes([data[0], data[1]]);
                    let num_frames = u32::from_be_bytes([data[2], data[3], data[4], data[5]]);
                    let bits_per_sample = u16::from_be_bytes([data[6], data[7]]);
                    let sample_rate = parse_extended_f80(&data[8..18])?;

                    let compression = if form_type == *b"AIFC" {
                        if data.len() < 22 {
                            return Err(AiffError::Decode(
                                "AIFC COMM chunk missing compression type".into(),
                            ));
                        }
                        match &data[18..22] {
                            b"NONE" => Compression::BigEndianPcm,
                            b"sowt" => Compression::LittleEndianPcm,
                            code => {
                                return Err(AiffError::Decode(format!(
                                    "unsupported AIFC compression type: {}",
                                    String::from_utf8_lossy(code)
                                )))
                            }
                        }
                    } else {
                        Compression::BigEndianPcm
                    };

                    let comm = CommChunk {
                        channels,
                        num_frames,
                        bits_per_sample,
                        sample_rate,
                        compression,
                    };
                    comm.validate()?;

                    comm_chunk = Some(comm);

                    if padded_size > chunk_size {
                        aiff_reader.skip(padded_size - chunk_size)?;
                    }
                }
                b"SSND" => {
                    let comm = comm_chunk.as_ref().ok_or_else(|| {
                        AiffError::Decode("SSND chunk encountered before COMM".into())
                    })?;

                    let header = aiff_reader.read_exact_vec(8)?;
                    let offset =
                        u32::from_be_bytes([header[0], header[1], header[2], header[3]]) as usize;
                    let _block_size =
                        u32::from_be_bytes([header[4], header[5], header[6], header[7]]) as usize;

                    if offset > 0 {
                        aiff_reader.skip(offset)?;
                    }

                    let data_bytes = chunk_size
                        .checked_sub(8)
                        .ok_or_else(|| AiffError::Decode("invalid SSND chunk size".into()))?;
                    let bytes_per_sample = comm.bytes_per_sample();

                    let info = StreamInfo {
                        sample_rate: comm.sample_rate,
                        channels: comm.channels as u8,
                        bits_per_sample: comm.bits_per_sample as u8,
                        total_samples: Some(comm.num_frames as u64),
                        max_block_size: 0,
                        min_block_size: 0,
                    };

                    if !stream_info_sent {
                        if info_tx.send(Ok(info.clone())).is_err() {
                            return Ok(());
                        }
                        stream_info_sent = true;
                    }

                    let mut remaining = data_bytes;
                    while remaining > 0 {
                        let mut to_read = remaining.min(8192);
                        let residue = to_read % bytes_per_sample;
                        if residue != 0 {
                            to_read -= residue;
                        }
                        if to_read == 0 {
                            to_read = bytes_per_sample;
                        }
                        let mut chunk = aiff_reader.read_exact_vec(to_read)?;
                        match comm.compression {
                            Compression::BigEndianPcm => {
                                chunk = convert_be_pcm(chunk, comm.bits_per_sample)?;
                            }
                            Compression::LittleEndianPcm => {
                                // data already little-endian; no conversion
                            }
                        }
                        if !chunk.is_empty() {
                            if pcm_tx.blocking_send(Ok(chunk)).is_err() {
                                return Ok(());
                            }
                        }
                        remaining = remaining
                            .checked_sub(to_read)
                            .ok_or_else(|| AiffError::Decode("SSND chunk underflow".into()))?;
                    }

                    if padded_size > chunk_size {
                        aiff_reader.skip(1)?;
                    }

                    break;
                }
                _ => {
                    aiff_reader.skip(chunk_size)?;
                    if padded_size > chunk_size {
                        aiff_reader.skip(padded_size - chunk_size)?;
                    }
                }
            }
        }

        if !stream_info_sent {
            return Err(AiffError::Decode(
                "no SSND chunk found in AIFF stream".into(),
            ));
        }

        Ok(())
    });

    let writer_handle = spawn_writer_task(pcm_rx, pcm_writer, blocking_handle, "aiff-decode");

    let info = info_rx.await.map_err(|_| AiffError::ChannelClosed)??;
    let reader = ManagedAsyncReader::new("aiff-decode-writer", pcm_reader, writer_handle);

    Ok(AiffDecodedStream { info, reader })
}

fn parse_extended_f80(bytes: &[u8]) -> Result<u32, AiffError> {
    if bytes.len() != 10 {
        return Err(AiffError::Decode("invalid 80-bit float length".into()));
    }
    let sign = if bytes[0] & 0x80 != 0 { -1.0 } else { 1.0 };
    let exponent = (((bytes[0] & 0x7F) as i32) << 8 | bytes[1] as i32) - 16383;
    let mut mantissa: u64 = 0;
    for b in &bytes[2..10] {
        mantissa = (mantissa << 8) | (*b as u64);
    }

    if exponent == -16383 && mantissa == 0 {
        return Ok(0);
    }

    let magnitude = mantissa as f64 / (1u64 << 63) as f64;
    let value = sign * magnitude * 2f64.powi(exponent);
    if value <= 0.0 {
        return Err(AiffError::Decode("invalid or negative sample rate".into()));
    }
    Ok(value.round() as u32)
}

fn convert_be_pcm(mut chunk: Vec<u8>, bits_per_sample: u16) -> Result<Vec<u8>, AiffError> {
    let bytes_per_sample = ((bits_per_sample as usize) + 7) / 8;
    if chunk.len() % bytes_per_sample != 0 {
        return Err(AiffError::Decode(
            "AIFF PCM data not aligned to whole samples".into(),
        ));
    }

    match bytes_per_sample {
        1 => Ok(chunk),
        2 => {
            for sample in chunk.chunks_mut(2) {
                sample.swap(0, 1);
            }
            Ok(chunk)
        }
        3 => {
            let mut out = Vec::with_capacity(chunk.len());
            for sample in chunk.chunks(3) {
                let value =
                    ((sample[0] as i32) << 16) | ((sample[1] as i32) << 8) | (sample[2] as i32);
                let value = if value & 0x0080_0000 != 0 {
                    value | !0x00FF_FFFF
                } else {
                    value
                };
                let le = value.to_le_bytes();
                out.extend_from_slice(&le[..3]);
            }
            Ok(out)
        }
        4 => {
            for sample in chunk.chunks_mut(4) {
                sample.swap(0, 3);
                sample.swap(1, 2);
            }
            Ok(chunk)
        }
        other => Err(AiffError::Decode(format!(
            "unsupported bytes per sample: {}",
            other
        ))),
    }
}
