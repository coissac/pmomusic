//! # MP3 Decoder Module
//!
//! This module provides asynchronous streaming MP3 decoding capabilities.
//! It decodes MP3 audio streams into PCM data (16-bit little-endian interleaved),
//! which can then be fed directly into the FLAC encoder for transcoding.
//!
//! ## Architecture
//!
//! The decoder uses a multi-task pipeline for efficient streaming:
//!
//! ```text
//! MP3 Input → [Ingest Task] → [Decode Task] → [Writer Task] → PCM Output (AsyncRead)
//!                  ↓              ↓                ↓
//!              mpsc channel   blocking I/O    duplex stream
//! ```
//!
//! - **Ingest Task**: Reads MP3 data in chunks and sends it through a channel
//! - **Decode Task**: Runs in a blocking thread, decodes MP3 frames using minimp3
//! - **Writer Task**: Writes decoded PCM data to a duplex stream
//!
//! This architecture ensures:
//! - True streaming with minimal memory footprint
//! - Non-blocking async I/O for the consumer
//! - Proper backpressure through bounded channels
//!
//! ## Example: Basic MP3 Decoding
//!
//! ```no_run
//! use pmoflac::decode_mp3_stream;
//! use tokio::fs::File;
//! use tokio::io::AsyncReadExt;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let file = File::open("audio.mp3").await?;
//!     let mut stream = decode_mp3_stream(file).await?;
//!
//!     // Get stream information
//!     let info = stream.info();
//!     println!("Sample rate: {} Hz", info.sample_rate);
//!     println!("Channels: {}", info.channels);
//!     println!("Bits per sample: {}", info.bits_per_sample);
//!
//!     // Read PCM data
//!     let mut pcm_buffer = Vec::new();
//!     stream.read_to_end(&mut pcm_buffer).await?;
//!
//!     // Wait for decoding to complete
//!     stream.wait().await?;
//!
//!     Ok(())
//! }
//! ```
//!
//! ## Example: MP3 to FLAC Transcoding
//!
//! ```no_run
//! use pmoflac::{decode_mp3_stream, encode_flac_stream, PcmFormat, EncoderOptions};
//! use tokio::fs::File;
//! use tokio::io::AsyncReadExt;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Decode MP3
//!     let mp3_file = File::open("input.mp3").await?;
//!     let stream = decode_mp3_stream(mp3_file).await?;
//!     let (info, pcm_reader) = stream.into_parts();
//!
//!     // Encode to FLAC
//!     let format = PcmFormat {
//!         sample_rate: info.sample_rate,
//!         channels: info.channels,
//!         bits_per_sample: info.bits_per_sample,
//!     };
//!     let mut flac_stream = encode_flac_stream(
//!         pcm_reader,
//!         format,
//!         EncoderOptions::default()
//!     ).await?;
//!
//!     // Write FLAC output
//!     let mut output = File::create("output.flac").await?;
//!     tokio::io::copy(&mut flac_stream, &mut output).await?;
//!     flac_stream.wait().await?;
//!
//!     Ok(())
//! }
//! ```

use std::{
    io,
    pin::Pin,
    task::{Context, Poll},
};

use minimp3::{Decoder as MiniMp3Decoder, Error as MiniMp3Error};
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

/// Errors that can occur while decoding MP3 data.
#[derive(thiserror::Error, Debug, Clone)]
pub enum Mp3Error {
    #[error("I/O error ({kind:?}): {message}")]
    Io {
        kind: io::ErrorKind,
        message: String,
    },
    #[error("MP3 decode error: {0}")]
    Decode(String),
    #[error("internal channel closed unexpectedly")]
    ChannelClosed,
    #[error("{role} task failed: {details}")]
    TaskJoin { role: &'static str, details: String },
}

impl From<io::Error> for Mp3Error {
    fn from(err: io::Error) -> Self {
        Mp3Error::Io {
            kind: err.kind(),
            message: err.to_string(),
        }
    }
}

impl From<String> for Mp3Error {
    fn from(msg: String) -> Self {
        Mp3Error::Decode(msg)
    }
}

/// An async stream that decodes MP3 audio into PCM samples.
///
/// This struct implements `AsyncRead`, allowing you to read decoded PCM data
/// as it becomes available. The decoding happens in a background task.
pub struct Mp3DecodedStream {
    info: StreamInfo,
    reader: ManagedAsyncReader<Mp3Error>,
}

impl Mp3DecodedStream {
    /// Returns metadata about the decoded MP3 stream.
    pub fn info(&self) -> &StreamInfo {
        &self.info
    }

    /// Consumes the stream and returns its components.
    pub fn into_parts(self) -> (StreamInfo, ManagedAsyncReader<Mp3Error>) {
        (self.info, self.reader)
    }

    /// Waits for the background decoding task to complete.
    ///
    /// This should be called after reading all data to ensure proper cleanup
    /// and to catch any errors that occurred during decoding.
    pub async fn wait(self) -> Result<(), Mp3Error> {
        self.reader.wait().await
    }
}

impl AsyncRead for Mp3DecodedStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.reader).poll_read(cx, buf)
    }
}

/// Decodes an MP3 stream into PCM audio data (16-bit little-endian interleaved).
///
/// This function spawns background tasks to perform the decoding asynchronously.
/// The returned `Mp3DecodedStream` implements `AsyncRead` for streaming the PCM output.
pub async fn decode_mp3_stream<R>(reader: R) -> Result<Mp3DecodedStream, Mp3Error>
where
    R: AsyncRead + Unpin + Send + 'static,
{
    let (ingest_tx, ingest_rx) = mpsc::channel(CHANNEL_CAPACITY);
    spawn_ingest_task(reader, ingest_tx);

    let (pcm_tx, pcm_rx) = mpsc::channel(CHANNEL_CAPACITY);
    let (pcm_reader, pcm_writer) = tokio::io::duplex(DUPLEX_BUFFER_SIZE);
    let (info_tx, info_rx) = oneshot::channel::<Result<StreamInfo, Mp3Error>>();

    let blocking_handle = tokio::task::spawn_blocking(move || -> Result<(), Mp3Error> {
        let channel_reader = ChannelReader::<Mp3Error>::new(ingest_rx);
        let mut decoder = MiniMp3Decoder::new(channel_reader);
        let mut info_tx = Some(info_tx);
        let mut pcm_bytes = Vec::new();

        loop {
            match decoder.next_frame() {
                Ok(frame) => {
                    if frame.channels == 0 {
                        let err = Mp3Error::Decode("MP3 frame reported zero channels".into());
                        if let Some(tx) = info_tx.take() {
                            let _ = tx.send(Err(err.clone()));
                        }
                        return Err(err);
                    }

                    if let Some(tx) = info_tx.take() {
                        let info = StreamInfo {
                            sample_rate: frame.sample_rate as u32,
                            channels: frame.channels as u8,
                            bits_per_sample: 16,
                            total_samples: None,
                            max_block_size: 0,
                            min_block_size: 0,
                        };

                        if tx.send(Ok(info.clone())).is_err() {
                            // Consumer dropped; we can stop decoding early.
                            return Ok(());
                        }
                    }

                    pcm_bytes.clear();
                    pcm_bytes.reserve(frame.data.len() * 2);
                    for sample in &frame.data {
                        pcm_bytes.extend_from_slice(&sample.to_le_bytes());
                    }

                    let chunk = std::mem::take(&mut pcm_bytes);
                    if pcm_tx.blocking_send(Ok(chunk)).is_err() {
                        break;
                    }
                    pcm_bytes = Vec::with_capacity(frame.data.len() * 2);
                }
                Err(MiniMp3Error::Eof) => break,
                Err(MiniMp3Error::InsufficientData) | Err(MiniMp3Error::SkippedData) => {
                    // Decoder needs more data; continue ingesting.
                    continue;
                }
                Err(MiniMp3Error::Io(err)) => {
                    let err = Mp3Error::from(err);
                    if let Some(tx) = info_tx.take() {
                        let _ = tx.send(Err(err.clone()));
                    }
                    return Err(err);
                }
            }
        }

        if let Some(tx) = info_tx.take() {
            let err = Mp3Error::Decode("stream contained no decodable MP3 frames".into());
            let _ = tx.send(Err(err.clone()));
            return Err(err);
        }

        Ok(())
    });

    let writer_handle = spawn_writer_task(pcm_rx, pcm_writer, blocking_handle, "mp3-decode");

    let info = info_rx
        .await
        .map_err(|_| Mp3Error::ChannelClosed)??;
    let reader = ManagedAsyncReader::new("mp3-decode-writer", pcm_reader, writer_handle);

    Ok(Mp3DecodedStream { info, reader })
}
