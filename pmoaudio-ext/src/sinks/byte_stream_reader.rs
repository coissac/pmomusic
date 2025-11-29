use std::io;
use std::{
    collections::VecDeque,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

use tokio::{
    io::{AsyncRead, ReadBuf},
    sync::{mpsc, RwLock},
};

/// PCM chunk with audio data and timestamp for precise pacing.
#[derive(Debug)]
pub struct PcmChunk {
    /// Raw PCM audio bytes
    pub bytes: Vec<u8>,
    /// Timestamp in seconds (from AudioSegment)
    pub timestamp_sec: f64,
    /// Duration in seconds of this PCM chunk (samples / sample_rate)
    pub duration_sec: f64,
}

/// AsyncRead adapter for mpsc::Receiver<PcmChunk>.
/// Extracts bytes from PcmChunk and provides them to the FLAC encoder.
pub struct ByteStreamReader {
    rx: mpsc::Receiver<PcmChunk>,
    buffer: VecDeque<u8>,
    finished: bool,
    /// Shared timestamp for broadcaster pacing
    current_timestamp: Arc<RwLock<f64>>,
    /// Shared duration for broadcaster pacing
    current_duration: Arc<RwLock<f64>>,
}

impl ByteStreamReader {
    pub fn new(
        rx: mpsc::Receiver<PcmChunk>,
        current_timestamp: Arc<RwLock<f64>>,
        current_duration: Arc<RwLock<f64>>,
    ) -> Self {
        Self {
            rx,
            buffer: VecDeque::new(),
            finished: false,
            current_timestamp,
            current_duration,
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

                let slice = self.buffer.make_contiguous();
                buf.put_slice(&slice[..to_copy]);
                self.buffer.drain(..to_copy);
                return Poll::Ready(Ok(()));
            }

            if self.finished {
                return Poll::Ready(Ok(()));
            }

            match Pin::new(&mut self.rx).poll_recv(cx) {
                Poll::Ready(Some(chunk)) => {
                    if chunk.bytes.is_empty() {
                        continue;
                    }
                    // Update shared timestamp and duration for broadcaster pacing
                    if let Ok(mut ts) = self.current_timestamp.try_write() {
                        *ts = chunk.timestamp_sec;
                    }
                    if let Ok(mut dur) = self.current_duration.try_write() {
                        *dur = chunk.duration_sec;
                    }
                    self.buffer.extend(chunk.bytes);
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
