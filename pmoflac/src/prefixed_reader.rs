//! Utility for reading from a prefix buffer followed by an underlying reader.
//!
//! This module provides `PrefixedReader`, which allows reading from an in-memory
//! buffer first, then seamlessly continuing with an underlying async reader.
//!
//! This is particularly useful for format detection where we need to peek at the
//! beginning of a stream before processing it.

use std::{
    cmp, io,
    pin::Pin,
    task::{Context, Poll},
};

use tokio::io::{AsyncRead, ReadBuf};

/// An async reader that reads from a prefix buffer before delegating to an underlying reader.
///
/// This is useful when you've already consumed some bytes from a stream (e.g., for format
/// detection) and need to replay them before continuing with the rest of the stream.
pub(crate) struct PrefixedReader<R> {
    prefix: Vec<u8>,
    position: usize,
    reader: R,
}

impl<R> PrefixedReader<R> {
    /// Creates a new `PrefixedReader` with the given prefix and underlying reader.
    ///
    /// # Arguments
    ///
    /// * `prefix` - Bytes to read first before delegating to the reader
    /// * `reader` - The underlying async reader to use after the prefix is exhausted
    pub fn new(prefix: Vec<u8>, reader: R) -> Self {
        Self {
            prefix,
            position: 0,
            reader,
        }
    }
}

impl<R: AsyncRead + Unpin> AsyncRead for PrefixedReader<R> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        // If we still have bytes in the prefix, read from there first
        if self.position < self.prefix.len() && buf.remaining() > 0 {
            let remaining = self.prefix.len() - self.position;
            let to_copy = cmp::min(remaining, buf.remaining());
            buf.put_slice(&self.prefix[self.position..self.position + to_copy]);
            self.position += to_copy;
            return Poll::Ready(Ok(()));
        }

        // Prefix exhausted, delegate to underlying reader
        Pin::new(&mut self.reader).poll_read(cx, buf)
    }
}

impl<R: Unpin> Unpin for PrefixedReader<R> {}
unsafe impl<R: Send> Send for PrefixedReader<R> {}
