use std::{
    io::{self, Read},
    pin::Pin,
    task::{Context, Poll},
};

use bytes::Bytes;
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWriteExt},
    sync::{mpsc, oneshot},
};

use crate::{
    error::FlacError, pcm::StreamInfo, stream::ManagedAsyncReader,
    util::interleaved_i32_to_le_bytes,
};

const INGEST_CHUNK_SIZE: usize = 32 * 1024;
const CHANNEL_CAPACITY: usize = 8;

pub struct FlacDecodedStream {
    info: StreamInfo,
    reader: ManagedAsyncReader,
}

impl FlacDecodedStream {
    pub fn info(&self) -> &StreamInfo {
        &self.info
    }

    pub fn into_parts(self) -> (StreamInfo, ManagedAsyncReader) {
        (self.info, self.reader)
    }

    pub async fn wait(self) -> Result<(), FlacError> {
        self.reader.wait().await
    }
}

impl AsyncRead for FlacDecodedStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.reader).poll_read(cx, buf)
    }
}

pub async fn decode_flac_stream<R>(reader: R) -> Result<FlacDecodedStream, FlacError>
where
    R: AsyncRead + Unpin + Send + 'static,
{
    let (ingest_tx, ingest_rx) = mpsc::channel::<Result<Bytes, FlacError>>(CHANNEL_CAPACITY);

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
                    let _ = ingest_tx.send(Err(FlacError::Io(err))).await;
                    break;
                }
            }
        }
    });

    let (pcm_tx, mut pcm_rx) = mpsc::channel::<Result<Vec<u8>, FlacError>>(CHANNEL_CAPACITY);
    let (pcm_reader, mut pcm_writer) = tokio::io::duplex(256 * 1024);
    let (info_tx, info_rx) = oneshot::channel::<Result<StreamInfo, FlacError>>();

    let blocking_handle = tokio::task::spawn_blocking(move || -> Result<(), FlacError> {
        let mut channel_reader = ChannelReader::new(ingest_rx);
        let mut flac_reader = match claxon::FlacReader::new(&mut channel_reader) {
            Ok(reader) => reader,
            Err(err) => {
                let msg = err.to_string();
                let _ = info_tx.send(Err(FlacError::Decode(msg.clone())));
                return Err(FlacError::Decode(msg));
            }
        };

        let flac_info = flac_reader.streaminfo();
        let info = StreamInfo {
            sample_rate: flac_info.sample_rate,
            channels: flac_info.channels as u8,
            bits_per_sample: flac_info.bits_per_sample as u8,
            total_samples: flac_info.samples,
            max_block_size: flac_info.max_block_size,
            min_block_size: flac_info.min_block_size,
        };
        if info_tx.send(Ok(info.clone())).is_err() {
            return Ok(());
        }

        let mut blocks = flac_reader.blocks();
        let mut buffer = Vec::new();
        let mut interleaved = Vec::new();
        let mut pcm_bytes = Vec::new();
        loop {
            match blocks.read_next_or_eof(buffer) {
                Ok(Some(block)) => {
                    let frames = block.duration() as usize;
                    let channels = block.channels() as usize;

                    interleaved.clear();
                    interleaved.reserve(frames * channels);
                    for frame_idx in 0..frames {
                        for channel_idx in 0..channels {
                            interleaved.push(block.sample(channel_idx as u32, frame_idx as u32));
                        }
                    }

                    pcm_bytes.clear();
                    pcm_bytes.reserve(frames * channels * info.bytes_per_sample());
                    interleaved_i32_to_le_bytes(&interleaved, info.bits_per_sample, &mut pcm_bytes);
                    let chunk = std::mem::take(&mut pcm_bytes);
                    if pcm_tx.blocking_send(Ok(chunk)).is_err() {
                        break;
                    }

                    pcm_bytes = Vec::with_capacity(frames * channels * info.bytes_per_sample());
                    buffer = block.into_buffer();
                }
                Ok(None) => break,
                Err(err) => {
                    let msg = err.to_string();
                    let _ = pcm_tx.blocking_send(Err(FlacError::Decode(msg.clone())));
                    return Err(FlacError::Decode(msg));
                }
            }
        }

        Ok(())
    });

    let writer_handle = tokio::spawn(async move {
        while let Some(chunk_result) = pcm_rx.recv().await {
            let chunk = chunk_result?;
            if chunk.is_empty() {
                continue;
            }
            pcm_writer.write_all(&chunk).await?;
        }
        pcm_writer.shutdown().await?;
        match blocking_handle.await {
            Ok(res) => res,
            Err(err) => Err(FlacError::TaskJoin {
                role: "flac-decode",
                details: err.to_string(),
            }),
        }
    });

    let info = info_rx.await.map_err(|_| FlacError::ChannelClosed)??;
    let reader = ManagedAsyncReader::new("flac-decode-writer", pcm_reader, writer_handle);

    Ok(FlacDecodedStream { info, reader })
}

struct ChannelReader {
    rx: mpsc::Receiver<Result<Bytes, FlacError>>,
    current: Bytes,
    offset: usize,
    finished: bool,
}

impl ChannelReader {
    fn new(rx: mpsc::Receiver<Result<Bytes, FlacError>>) -> Self {
        Self {
            rx,
            current: Bytes::new(),
            offset: 0,
            finished: false,
        }
    }
}

impl Read for ChannelReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        loop {
            if self.offset < self.current.len() {
                let n = std::cmp::min(buf.len(), self.current.len() - self.offset);
                buf[..n].copy_from_slice(&self.current[self.offset..self.offset + n]);
                self.offset += n;
                return Ok(n);
            }
            if self.finished {
                return Ok(0);
            }

            match self.rx.blocking_recv() {
                Some(Ok(bytes)) => {
                    if bytes.is_empty() {
                        continue;
                    }
                    self.current = bytes;
                    self.offset = 0;
                }
                Some(Err(err)) => {
                    self.finished = true;
                    return Err(io::Error::new(io::ErrorKind::Other, err.to_string()));
                }
                None => {
                    self.finished = true;
                    return Ok(0);
                }
            }
        }
    }
}
