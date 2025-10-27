//! Common utilities shared between decoders.
//!
//! This module contains structures and adapters used by both FLAC and MP3 decoders
//! to bridge async channels with synchronous I/O requirements.

use std::io::{self, Read};

use bytes::Bytes;
use tokio::sync::mpsc;

/// Internal adapter that bridges async channel reading to sync `std::io::Read`.
///
/// Many decoder libraries (like minimp3, claxon) require a synchronous `Read`
/// implementation, but our data arrives via an async channel. This adapter uses
/// `blocking_recv` to bridge the gap, buffering chunks as they arrive.
///
/// This is generic over the error type to support both FLAC and MP3 decoders.
pub(crate) struct ChannelReader<E>
where
    E: std::error::Error + std::fmt::Display,
{
    rx: mpsc::Receiver<Result<Bytes, E>>,
    current: Bytes,
    offset: usize,
    finished: bool,
}

impl<E> ChannelReader<E>
where
    E: std::error::Error + std::fmt::Display,
{
    pub fn new(rx: mpsc::Receiver<Result<Bytes, E>>) -> Self {
        Self {
            rx,
            current: Bytes::new(),
            offset: 0,
            finished: false,
        }
    }
}

impl<E> Read for ChannelReader<E>
where
    E: std::error::Error + std::fmt::Display,
{
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        loop {
            // If we have data in the current buffer, copy it out
            if self.offset < self.current.len() {
                let n = buf.len().min(self.current.len() - self.offset);
                buf[..n].copy_from_slice(&self.current[self.offset..self.offset + n]);
                self.offset += n;
                return Ok(n);
            }

            // If we're finished, return EOF
            if self.finished {
                return Ok(0);
            }

            // Try to receive the next chunk from the channel
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
