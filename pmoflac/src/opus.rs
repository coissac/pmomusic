//! # Ogg/Opus Decoder Module
//!
//! Streaming decoder for Ogg-wrapped Opus audio. This reuses the common async
//! ingestion/producer pattern established for the other decoders while staying
//! 100% streaming (no seeking or buffering entire files).

use std::{
    io,
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
    ogg_common::{OggContainerError, OggPacketReader, OggReaderOptions},
    pcm::StreamInfo,
    stream::ManagedAsyncReader,
};

/// Maximum number of samples per Opus frame at 48 kHz (120 ms).
const MAX_FRAME_SAMPLES: usize = 5760;

/// Shared error alias for the Opus decoder.
pub type OggOpusError = OggContainerError;

impl From<OpusError> for OggContainerError {
    fn from(err: OpusError) -> Self {
        OggContainerError::Decode(err.to_string())
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
        let channel_reader = ChannelReader::<OggContainerError>::new(ingest_rx);
        let mut packet_reader = OggPacketReader::new(
            channel_reader,
            OggReaderOptions {
                validate_crc: false,
                find_sync: false,
                ..OggReaderOptions::default()
            },
        );

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
