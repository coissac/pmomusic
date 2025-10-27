//! Common utilities for audio decoders.
//!
//! This module provides shared functionality for the FLAC, MP3, and Ogg/Vorbis decoders,
//! reducing code duplication and ensuring consistent behavior across all decoders.

use std::{
    io,
    pin::Pin,
    task::{Context, Poll},
};

use bytes::Bytes;
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWriteExt, DuplexStream, ReadBuf},
    sync::mpsc,
    task::JoinHandle,
};

use crate::{pcm::StreamInfo, stream::ManagedAsyncReader};

/// Generic error type shared by the streaming decoders.
#[derive(thiserror::Error, Debug, Clone)]
pub enum DecoderError {
    #[error("I/O error ({kind:?}): {message}")]
    Io {
        kind: io::ErrorKind,
        message: String,
    },
    #[error("{0}")]
    Decode(String),
    #[error("internal channel closed unexpectedly")]
    ChannelClosed,
}

impl From<io::Error> for DecoderError {
    fn from(err: io::Error) -> Self {
        DecoderError::Io {
            kind: err.kind(),
            message: err.to_string(),
        }
    }
}

impl From<String> for DecoderError {
    fn from(value: String) -> Self {
        DecoderError::Decode(value)
    }
}

impl From<&str> for DecoderError {
    fn from(value: &str) -> Self {
        DecoderError::Decode(value.to_owned())
    }
}

/// Generic decoded stream wrapper shared by all decoders.
pub struct DecodedStream<E>
where
    E: std::error::Error,
{
    info: StreamInfo,
    reader: ManagedAsyncReader<E>,
}

impl<E> DecodedStream<E>
where
    E: std::error::Error,
{
    /// Creates a new decoded stream from metadata and the underlying reader.
    pub fn new(info: StreamInfo, reader: ManagedAsyncReader<E>) -> Self {
        Self { info, reader }
    }

    /// Returns metadata about the decoded audio stream.
    pub fn info(&self) -> &StreamInfo {
        &self.info
    }

    /// Consumes the stream and returns its components.
    pub fn into_parts(self) -> (StreamInfo, ManagedAsyncReader<E>) {
        (self.info, self.reader)
    }

    /// Waits for the decoding pipeline to finish.
    pub async fn wait(self) -> Result<(), E>
    where
        E: From<String>,
    {
        self.reader.wait().await
    }
}

impl<E> AsyncRead for DecodedStream<E>
where
    E: std::error::Error,
{
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.reader).poll_read(cx, buf)
    }
}

/// Size of chunks when reading input data.
///
/// This size balances between efficient I/O operations and memory usage.
/// Larger chunks reduce system call overhead, while smaller chunks reduce latency.
pub(crate) const INGEST_CHUNK_SIZE: usize = 16 * 1024;

/// Channel capacity for async message passing between tasks.
///
/// This bounded capacity provides backpressure: if the decoder can't keep up,
/// the ingest task will wait before reading more data.
pub(crate) const CHANNEL_CAPACITY: usize = 8;

/// Size of the duplex stream buffer for PCM output (256 KB).
pub(crate) const DUPLEX_BUFFER_SIZE: usize = 256 * 1024;

/// Spawns an async task that ingests data from a reader and sends it through a channel.
///
/// This task reads chunks of data from the input reader and forwards them via an mpsc channel
/// to the decoder. It handles EOF and errors gracefully.
///
/// # Arguments
///
/// * `reader` - The async reader to ingest data from
/// * `ingest_tx` - Channel sender to forward data chunks
///
/// # Type Parameters
///
/// * `R` - The async reader type
/// * `E` - The error type (must be convertible from `io::Error`)
pub(crate) fn spawn_ingest_task<R, E>(
    reader: R,
    ingest_tx: mpsc::Sender<Result<Bytes, E>>,
) -> JoinHandle<()>
where
    R: AsyncRead + Unpin + Send + 'static,
    E: From<io::Error> + Send + 'static,
{
    tokio::spawn(async move {
        let mut reader = tokio::io::BufReader::new(reader);
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
                    let _ = ingest_tx.send(Err(E::from(err))).await;
                    break;
                }
            }
        }
    })
}

/// Spawns an async task that writes PCM data from a channel to a duplex stream.
///
/// This task receives PCM chunks from a channel and writes them to a duplex stream,
/// which can be read by the consumer. It waits for the blocking decoder task to complete
/// and propagates any errors.
///
/// # Arguments
///
/// * `pcm_rx` - Channel receiver for PCM data chunks
/// * `pcm_writer` - Duplex stream writer for PCM output
/// * `blocking_handle` - Join handle for the blocking decoder task
/// * `role` - Name of the decoder role (for error messages)
///
/// # Type Parameters
///
/// * `E` - The error type
///
/// # Returns
///
/// A join handle for the writer task that returns `Result<(), E>`
pub(crate) fn spawn_writer_task<E>(
    mut pcm_rx: mpsc::Receiver<Result<Vec<u8>, E>>,
    mut pcm_writer: DuplexStream,
    blocking_handle: JoinHandle<Result<(), E>>,
    role: &'static str,
) -> JoinHandle<Result<(), E>>
where
    E: From<io::Error> + From<String> + Send + 'static,
{
    tokio::spawn(async move {
        while let Some(chunk_result) = pcm_rx.recv().await {
            let chunk = chunk_result?;
            if chunk.is_empty() {
                continue;
            }
            pcm_writer.write_all(&chunk).await.map_err(E::from)?;
        }
        pcm_writer.shutdown().await.map_err(E::from)?;
        match blocking_handle.await {
            Ok(res) => res,
            Err(err) => Err(E::from(format!("{} task failed: {}", role, err))),
        }
    })
}
