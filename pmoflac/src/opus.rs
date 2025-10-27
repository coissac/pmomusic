//! # Ogg/Opus Decoder Module
//!
//! Streaming decoder for Ogg-wrapped Opus audio. This reuses the common async
//! ingestion/producer pattern established for the other decoders while staying
//! 100% streaming (no seeking or buffering entire files).

use std::{
    io::{self, Read},
    pin::Pin,
    task::{Context, Poll},
};

use opus::{Channels, Decoder as OpusDecoder, Error as OpusError};
use tokio::{
    io::{AsyncRead, ReadBuf},
    sync::{mpsc, oneshot},
};

use crate::{
    common::ChannelReader,
    decoder_common::{spawn_ingest_task, spawn_writer_task, CHANNEL_CAPACITY, DUPLEX_BUFFER_SIZE},
    pcm::StreamInfo,
    stream::ManagedAsyncReader,
};

/// Maximum number of samples per Opus frame at 48 kHz (120 ms).
const MAX_FRAME_SAMPLES: usize = 5760;

/// Errors that can occur while decoding Ogg/Opus data.
#[derive(thiserror::Error, Debug, Clone)]
pub enum OggOpusError {
    #[error("I/O error ({kind:?}): {message}")]
    Io {
        kind: io::ErrorKind,
        message: String,
    },
    #[error("Ogg/Opus decode error: {0}")]
    Decode(String),
    #[error("internal channel closed unexpectedly")]
    ChannelClosed,
}

impl From<io::Error> for OggOpusError {
    fn from(err: io::Error) -> Self {
        OggOpusError::Io {
            kind: err.kind(),
            message: err.to_string(),
        }
    }
}

impl From<OpusError> for OggOpusError {
    fn from(err: OpusError) -> Self {
        OggOpusError::Decode(err.to_string())
    }
}

impl From<String> for OggOpusError {
    fn from(value: String) -> Self {
        OggOpusError::Decode(value)
    }
}

/// An async stream that decodes Ogg/Opus audio into PCM samples.
pub struct OggOpusDecodedStream {
    info: StreamInfo,
    reader: ManagedAsyncReader<OggOpusError>,
}

impl OggOpusDecodedStream {
    /// Returns metadata about the decoded stream.
    pub fn info(&self) -> &StreamInfo {
        &self.info
    }

    /// Consumes the stream and returns its components.
    pub fn into_parts(self) -> (StreamInfo, ManagedAsyncReader<OggOpusError>) {
        (self.info, self.reader)
    }

    /// Waits for the decoding pipeline to finish.
    pub async fn wait(self) -> Result<(), OggOpusError> {
        self.reader.wait().await
    }
}

impl AsyncRead for OggOpusDecodedStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.reader).poll_read(cx, buf)
    }
}

/// Decodes an Ogg/Opus stream into PCM audio (16-bit little-endian).
pub async fn decode_ogg_opus_stream<R>(reader: R) -> Result<OggOpusDecodedStream, OggOpusError>
where
    R: AsyncRead + Unpin + Send + 'static,
{
    let (ingest_tx, ingest_rx) = mpsc::channel(CHANNEL_CAPACITY);
    spawn_ingest_task::<_, OggOpusError>(reader, ingest_tx);

    let (pcm_tx, pcm_rx) = mpsc::channel(CHANNEL_CAPACITY);
    let (pcm_reader, pcm_writer) = tokio::io::duplex(DUPLEX_BUFFER_SIZE);
    let (info_tx, info_rx) = oneshot::channel::<Result<StreamInfo, OggOpusError>>();

    let blocking_handle = tokio::task::spawn_blocking(move || -> Result<(), OggOpusError> {
        let channel_reader = ChannelReader::<OggOpusError>::new(ingest_rx);
        let mut packet_reader = StreamingPacketReader::new(channel_reader);

        let header_packet = packet_reader
            .next_packet()?
            .ok_or_else(|| OggOpusError::Decode("missing OpusHead packet".into()))?;
        let header = OpusHead::parse(&header_packet)?;

        let tags_packet = packet_reader
            .next_packet()?
            .ok_or_else(|| OggOpusError::Decode("missing OpusTags packet".into()))?;
        let _tags = OpusTags::parse(&tags_packet)?;

        let channels_enum = match header.channels {
            1 => Channels::Mono,
            2 => Channels::Stereo,
            other => {
                return Err(OggOpusError::Decode(format!(
                    "unsupported channel count: {}",
                    other
                )))
            }
        };

        let mut decoder = OpusDecoder::new(48_000, channels_enum)?;
        if header.output_gain != 0 {
            decoder.set_gain(i32::from(header.output_gain))?;
        }

        let info = StreamInfo {
            sample_rate: 48_000,
            channels: header.channels,
            bits_per_sample: 16,
            total_samples: None,
            max_block_size: MAX_FRAME_SAMPLES as u16,
            min_block_size: 0,
        };

        if info_tx.send(Ok(info.clone())).is_err() {
            return Ok(());
        }

        let channels = header.channels as usize;
        let mut pcm_buffer = vec![0i16; MAX_FRAME_SAMPLES * channels];
        let mut pcm_bytes = Vec::new();
        let mut pre_skip = header.pre_skip as usize;
        let mut produced_audio = false;

        while let Some(packet) = packet_reader.next_packet()? {
            if pcm_buffer.len() < MAX_FRAME_SAMPLES * channels {
                pcm_buffer.resize(MAX_FRAME_SAMPLES * channels, 0);
            }

            let decoded_frames = decoder.decode(
                &packet,
                &mut pcm_buffer[..MAX_FRAME_SAMPLES * channels],
                false,
            )?;
            if decoded_frames == 0 {
                continue;
            }

            let mut start_frame = 0;
            if pre_skip > 0 {
                let drop = pre_skip.min(decoded_frames);
                pre_skip -= drop;
                start_frame = drop;
                if start_frame == decoded_frames {
                    continue;
                }
            }

            let start_index = start_frame * channels;
            let end_index = decoded_frames * channels;

            pcm_bytes.clear();
            pcm_bytes.reserve((end_index - start_index) * 2);
            for sample in &pcm_buffer[start_index..end_index] {
                pcm_bytes.extend_from_slice(&sample.to_le_bytes());
            }

            if pcm_bytes.is_empty() {
                continue;
            }

            produced_audio = true;
            let chunk = pcm_bytes.clone();
            if pcm_tx.blocking_send(Ok(chunk)).is_err() {
                break;
            }
        }

        if !produced_audio {
            return Err(OggOpusError::Decode(
                "stream contained no decodable Opus packets".into(),
            ));
        }

        Ok(())
    });

    let writer_handle = spawn_writer_task(pcm_rx, pcm_writer, blocking_handle, "ogg-opus");

    let info = info_rx.await.map_err(|_| OggOpusError::ChannelClosed)??;
    let reader = ManagedAsyncReader::new("ogg-opus-writer", pcm_reader, writer_handle);

    Ok(OggOpusDecodedStream { info, reader })
}

/// Parsed OpusHead metadata.
struct OpusHead {
    channels: u8,
    pre_skip: u16,
    output_gain: i16,
}

impl OpusHead {
    fn parse(data: &[u8]) -> Result<Self, OggOpusError> {
        if data.len() < 19 {
            return Err(OggOpusError::Decode("OpusHead packet too short".into()));
        }
        if &data[0..8] != b"OpusHead" {
            return Err(OggOpusError::Decode("invalid OpusHead signature".into()));
        }
        let version = data[8];
        if version == 0 || version > 15 {
            return Err(OggOpusError::Decode(format!(
                "unsupported Opus version: {}",
                version
            )));
        }
        let channels = data[9];
        if channels == 0 {
            return Err(OggOpusError::Decode(
                "Opus channel count must be > 0".into(),
            ));
        }

        let pre_skip = u16::from_le_bytes([data[10], data[11]]);
        let _input_sample_rate = u32::from_le_bytes([data[12], data[13], data[14], data[15]]);
        let output_gain = i16::from_le_bytes([data[16], data[17]]);
        let channel_mapping = data[18];

        if channel_mapping != 0 {
            return Err(OggOpusError::Decode(
                "non-default Opus channel mapping is unsupported".into(),
            ));
        }

        Ok(Self {
            channels,
            pre_skip,
            output_gain,
        })
    }
}

/// Minimal parsing of OpusTags (metadata). We only validate the signature.
struct OpusTags;

impl OpusTags {
    fn parse(data: &[u8]) -> Result<Self, OggOpusError> {
        if data.len() < 8 || &data[0..8] != b"OpusTags" {
            return Err(OggOpusError::Decode("invalid OpusTags header".into()));
        }
        Ok(OpusTags)
    }
}

/// Streaming Ogg packet reader reused for Opus packets.
struct StreamingPacketReader<E>
where
    E: std::error::Error + std::fmt::Display,
{
    reader: ChannelReader<E>,
    current_packet: Vec<u8>,
    pending_packets: std::collections::VecDeque<Vec<u8>>,
    finished: bool,
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
            pending_packets: std::collections::VecDeque::new(),
            finished: false,
            stream_serial: None,
        }
    }

    fn next_packet(&mut self) -> Result<Option<Vec<u8>>, OggOpusError> {
        loop {
            if let Some(packet) = self.pending_packets.pop_front() {
                return Ok(Some(packet));
            }
            if self.finished {
                return Ok(None);
            }
            self.read_page()?;
        }
    }

    fn read_page(&mut self) -> Result<(), OggOpusError> {
        let mut header = [0u8; 27];
        if !read_exact_or_eof(&mut self.reader, &mut header)? {
            self.finished = true;
            return Ok(());
        }

        if &header[0..4] != b"OggS" {
            return Err(OggOpusError::Decode("invalid Ogg capture pattern".into()));
        }
        if header[4] != 0 {
            return Err(OggOpusError::Decode("unsupported Ogg version".into()));
        }

        let header_type = header[5];
        let bitstream_serial = u32::from_le_bytes([header[14], header[15], header[16], header[17]]);

        if let Some(serial) = self.stream_serial {
            if serial != bitstream_serial {
                return Err(OggOpusError::Decode(
                    "multiple logical Ogg streams are unsupported".to_string(),
                ));
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
            return Err(OggOpusError::Decode(
                "continuation flag set without existing packet".into(),
            ));
        }
        if header_type & 0x01 == 0 && !self.current_packet.is_empty() {
            return Err(OggOpusError::Decode(
                "expected continuation flag for unfinished packet".into(),
            ));
        }

        let mut offset = 0usize;
        for &seg_len in &segment_table {
            let len = seg_len as usize;
            let end = offset
                .checked_add(len)
                .ok_or_else(|| OggOpusError::Decode("segment length overflow".into()))?;
            if end > data.len() {
                return Err(OggOpusError::Decode("segment exceeds page data".into()));
            }
            self.current_packet.extend_from_slice(&data[offset..end]);
            offset = end;

            if seg_len < 255 {
                let packet = std::mem::take(&mut self.current_packet);
                self.pending_packets.push_back(packet);
            }
        }

        if offset != data.len() {
            return Err(OggOpusError::Decode("page data not fully consumed".into()));
        }

        if header_type & 0x04 != 0 && self.current_packet.is_empty() {
            self.finished = true;
        }

        Ok(())
    }
}

fn read_exact_or_eof<R: Read>(reader: &mut R, buf: &mut [u8]) -> io::Result<bool> {
    let mut read = 0;
    while read < buf.len() {
        match reader.read(&mut buf[read..])? {
            0 if read == 0 => return Ok(false),
            0 => {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "unexpected EOF while reading",
                ))
            }
            n => read += n,
        }
    }
    Ok(true)
}

fn read_exact_checked<R: Read>(reader: &mut R, buf: &mut [u8]) -> Result<(), OggOpusError> {
    if !read_exact_or_eof(reader, buf)? {
        return Err(OggOpusError::Decode("unexpected EOF in Ogg stream".into()));
    }
    Ok(())
}
