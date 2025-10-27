//! # Ogg/Vorbis Streaming Decoder
//!
//! This module implements a fully streaming Ogg/Vorbis decoder without relying
//! on random access. It parses Ogg pages sequentially, assembles Vorbis packets,
//! and decodes them with Lewton's low-level audio API, yielding 16-bit LE PCM.

use std::{
    collections::VecDeque,
    io::{self, Read},
    pin::Pin,
    task::{Context, Poll},
};

use bytes::Bytes;
use lewton::{
    audio::{self, read_audio_packet_generic, PreviousWindowRight},
    header::{self, read_header_comment, read_header_ident, read_header_setup, CommentHeader},
    samples::InterleavedSamples,
};
use tokio::{
    io::{
        self as tokio_io, AsyncRead, AsyncReadExt, AsyncWriteExt, ReadBuf,
    },
    sync::{mpsc, oneshot},
};

use crate::{common::ChannelReader, pcm::StreamInfo, stream::ManagedAsyncReader};

/// Size of chunks when reading Ogg/Vorbis input data (16 KB).
const INGEST_CHUNK_SIZE: usize = 16 * 1024;

/// Channel capacity for async message passing between tasks.
const CHANNEL_CAPACITY: usize = 8;

/// Errors that can occur while decoding Ogg/Vorbis data.
#[derive(thiserror::Error, Debug, Clone)]
pub enum OggError {
    #[error("I/O error ({kind:?}): {message}")]
    Io {
        kind: io::ErrorKind,
        message: String,
    },
    #[error("Ogg/Vorbis decode error: {0}")]
    Decode(String),
    #[error("internal channel closed unexpectedly")]
    ChannelClosed,
    #[error("{role} task failed: {details}")]
    TaskJoin { role: &'static str, details: String },
}

impl From<io::Error> for OggError {
    fn from(err: io::Error) -> Self {
        OggError::Io {
            kind: err.kind(),
            message: err.to_string(),
        }
    }
}

impl From<header::HeaderReadError> for OggError {
    fn from(err: header::HeaderReadError) -> Self {
        OggError::Decode(err.to_string())
    }
}

impl From<audio::AudioReadError> for OggError {
    fn from(err: audio::AudioReadError) -> Self {
        OggError::Decode(err.to_string())
    }
}

impl From<String> for OggError {
    fn from(value: String) -> Self {
        OggError::Decode(value)
    }
}

/// An async stream that decodes Ogg/Vorbis audio into PCM samples.
pub struct OggDecodedStream {
    info: StreamInfo,
    reader: ManagedAsyncReader<OggError>,
}

impl OggDecodedStream {
    /// Returns metadata about the decoded stream (sample rate, channels, etc.).
    pub fn info(&self) -> &StreamInfo {
        &self.info
    }

    /// Consumes the stream and returns its components.
    pub fn into_parts(self) -> (StreamInfo, ManagedAsyncReader<OggError>) {
        (self.info, self.reader)
    }

    /// Waits for the background decoding task to complete.
    pub async fn wait(self) -> Result<(), OggError> {
        self.reader.wait().await
    }
}

impl AsyncRead for OggDecodedStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.reader).poll_read(cx, buf)
    }
}

/// Decodes an Ogg/Vorbis stream into PCM audio (16-bit little-endian, interleaved).
pub async fn decode_ogg_vorbis_stream<R>(reader: R) -> Result<OggDecodedStream, OggError>
where
    R: AsyncRead + Unpin + Send + 'static,
{
    let (ingest_tx, ingest_rx) = mpsc::channel::<Result<Bytes, OggError>>(CHANNEL_CAPACITY);

    tokio::spawn(async move {
        let mut reader = tokio_io::BufReader::new(reader);
        let mut buf = vec![0u8; INGEST_CHUNK_SIZE];

        loop {
            match reader.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    let chunk = Bytes::copy_from_slice(&buf[..n]);
                    if ingest_tx.send(Ok(chunk)).await.is_err() {
                        break;
                    }
                }
                Err(err) => {
                    let _ = ingest_tx.send(Err(OggError::from(err))).await;
                    break;
                }
            }
        }
    });

    let (pcm_tx, mut pcm_rx) = mpsc::channel::<Result<Vec<u8>, OggError>>(CHANNEL_CAPACITY);
    let (pcm_reader, mut pcm_writer) = tokio_io::duplex(256 * 1024);
    let (info_tx, info_rx) = oneshot::channel::<Result<StreamInfo, OggError>>();

    let blocking_handle = tokio::task::spawn_blocking(move || -> Result<(), OggError> {
        let channel_reader = ChannelReader::<OggError>::new(ingest_rx);
        let mut packet_reader = StreamingPacketReader::new(channel_reader);

        // Read Vorbis headers
        let ident_packet = packet_reader
            .next_packet()?
            .ok_or_else(|| OggError::Decode("missing Vorbis identification header".into()))?;
        let ident_hdr = read_header_ident(&ident_packet)?;

        let comment_packet = packet_reader
            .next_packet()?
            .ok_or_else(|| OggError::Decode("missing Vorbis comment header".into()))?;
        let _comment_hdr: CommentHeader = read_header_comment(&comment_packet)?;

        let setup_packet = packet_reader
            .next_packet()?
            .ok_or_else(|| OggError::Decode("missing Vorbis setup header".into()))?;
        let setup_hdr = read_header_setup(&setup_packet, ident_hdr.audio_channels, (ident_hdr.blocksize_0, ident_hdr.blocksize_1))?;

        let info = StreamInfo {
            sample_rate: ident_hdr.audio_sample_rate,
            channels: ident_hdr.audio_channels,
            bits_per_sample: 16,
            total_samples: None,
            max_block_size: 1 << ident_hdr.blocksize_1,
            min_block_size: 1 << ident_hdr.blocksize_0,
        };

        if info_tx.send(Ok(info.clone())).is_err() {
            return Ok(());
        }

        let mut pcm_bytes = Vec::new();
        let mut produced_audio = false;
        let mut pwr = PreviousWindowRight::new();

        while let Some(packet) = packet_reader.next_packet()? {
            let decoded: InterleavedSamples<i16> =
                read_audio_packet_generic(&ident_hdr, &setup_hdr, &packet, &mut pwr)?;

            if decoded.samples.is_empty() {
                continue;
            }

            produced_audio = true;
            pcm_bytes.clear();
            pcm_bytes.reserve(decoded.samples.len() * 2);
            for sample in decoded.samples {
                pcm_bytes.extend_from_slice(&sample.to_le_bytes());
            }
            let chunk = std::mem::take(&mut pcm_bytes);
            if pcm_tx.blocking_send(Ok(chunk)).is_err() {
                break;
            }
            pcm_bytes = Vec::new();
        }

        if !produced_audio {
            let err = OggError::Decode("stream contained no decodable Vorbis packets".into());
            let _ = pcm_tx.blocking_send(Err(err.clone()));
            return Err(err);
        }

        Ok(())
    });

    let writer_handle = tokio::spawn(async move {
        while let Some(chunk_result) = pcm_rx.recv().await {
            let chunk = chunk_result?;
            if chunk.is_empty() {
                continue;
            }
            pcm_writer.write_all(&chunk).await.map_err(OggError::from)?;
        }
        pcm_writer.shutdown().await.map_err(OggError::from)?;
        match blocking_handle.await {
            Ok(res) => res,
            Err(err) => Err(OggError::TaskJoin {
                role: "ogg-decode",
                details: err.to_string(),
            }),
        }
    });

    let info = info_rx.await.map_err(|_| OggError::ChannelClosed)??;
    let reader = ManagedAsyncReader::new("ogg-decode-writer", pcm_reader, writer_handle);

    Ok(OggDecodedStream { info, reader })
}

/// Streaming packet reader that assembles Vorbis packets without seeking.
struct StreamingPacketReader<E>
where
    E: std::error::Error + std::fmt::Display,
{
    reader: ChannelReader<E>,
    current_packet: Vec<u8>,
    queue: VecDeque<Vec<u8>>,
    finished: bool,
    eos_seen: bool,
    stream_serial: Option<u32>,
}

impl<E> StreamingPacketReader<E>
where
    E: std::error::Error + std::fmt::Display,
{
    fn new(reader: ChannelReader<E>) -> Self {
        Self {
            reader,
            current_packet: Vec::new(),
            queue: VecDeque::new(),
            finished: false,
            eos_seen: false,
            stream_serial: None,
        }
    }

    fn next_packet(&mut self) -> Result<Option<Vec<u8>>, OggError> {
        loop {
            if let Some(packet) = self.queue.pop_front() {
                return Ok(Some(packet));
            }
            if self.finished {
                return Ok(None);
            }
            self.read_page()?;
        }
    }

    fn read_page(&mut self) -> Result<(), OggError> {
        let mut header = [0u8; 27];
        if !read_exact_or_eof(&mut self.reader, &mut header)? {
            self.finished = true;
            return Ok(());
        }

        if &header[0..4] != b"OggS" {
            return Err(OggError::Decode("invalid Ogg capture pattern".into()));
        }
        if header[4] != 0 {
            return Err(OggError::Decode("unsupported Ogg version".into()));
        }

        let header_type = header[5];
        let bitstream_serial = u32::from_le_bytes([
            header[14], header[15], header[16], header[17],
        ]);

        if let Some(serial) = self.stream_serial {
            if serial != bitstream_serial {
                return Err(OggError::Decode("multiple logical streams are not supported".into()));
            }
        } else {
            self.stream_serial = Some(bitstream_serial);
        }

        let page_segments = header[26] as usize;
        let mut segment_table = vec![0u8; page_segments];
        read_exact_checked(&mut self.reader, &mut segment_table)?;

        let data_len: usize = segment_table.iter().map(|&v| v as usize).sum();
        let mut data = vec![0u8; data_len];
        read_exact_checked(&mut self.reader, &mut data)?;

        if header_type & 0x01 != 0 && self.current_packet.is_empty() {
            return Err(OggError::Decode("unexpected continuation flag without existing packet".into()));
        }
        if header_type & 0x01 == 0 && !self.current_packet.is_empty() {
            return Err(OggError::Decode("dangling packet without continuation flag".into()));
        }

        let mut offset: usize = 0;
        for &seg_len in &segment_table {
            let len = seg_len as usize;
            let end = offset
                .checked_add(len)
                .ok_or_else(|| OggError::Decode("segment length overflow".into()))?;
            if end > data.len() {
                return Err(OggError::Decode("segment exceeds page data".into()));
            }
            self.current_packet.extend_from_slice(&data[offset..end]);
            offset = end;

            if seg_len < 255 {
                let packet = std::mem::take(&mut self.current_packet);
                self.queue.push_back(packet);
            }
        }

        if offset != data.len() {
            return Err(OggError::Decode("page data not fully consumed".into()));
        }

        if header_type & 0x04 != 0 {
            self.eos_seen = true;
            if self.current_packet.is_empty() {
                self.finished = true;
            }
        }

        Ok(())
    }
}

fn read_exact_or_eof<R: Read>(reader: &mut R, buf: &mut [u8]) -> io::Result<bool> {
    let mut read = 0;
    while read < buf.len() {
        match reader.read(&mut buf[read..])? {
            0 if read == 0 => return Ok(false),
            0 => return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "unexpected EOF while reading",
            )),
            n => read += n,
        }
    }
    Ok(true)
}

fn read_exact_checked<R: Read>(reader: &mut R, buf: &mut [u8]) -> Result<(), OggError> {
    if !read_exact_or_eof(reader, buf)? {
        return Err(OggError::Decode("unexpected EOF in Ogg stream".into()));
    }
    Ok(())
}
