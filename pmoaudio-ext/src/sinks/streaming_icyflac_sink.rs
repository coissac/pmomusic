use std::{collections::VecDeque, pin::Pin, sync::Arc, task::{Context, Poll}};

use tokio::{io::{AsyncRead, ReadBuf}, sync::RwLock};

use crate::{MetadataSnapshot, sinks::{flac_frame_utils::FlacStreamState, streaming_sink_common::SharedStreamHandleInner, timed_broadcast::{self, TryRecvError}}};
use bytes::Bytes;
use std::io;

use tracing::{debug, warn};

/// ICY-wrapped FLAC client stream (implements AsyncRead).
///
/// This stream injects ICY metadata blocks at regular intervals,
/// allowing clients to display "Now Playing" information.
/// As with [`FlacClientStream`], hitting [`TryRecvError::Lagged`]
/// simply indicates that the timed broadcast discarded a stale chunk;
/// the client resumes with fresh data to avoid wedging the HTTP response.
pub struct IcyClientStream {
    rx: timed_broadcast::Receiver<Bytes>,
    metadata: Arc<RwLock<MetadataSnapshot>>,
    metaint: usize,
    byte_count: usize,
    buffer: VecDeque<u8>,
    current_metadata_version: u64,
    cached_icy_metadata: Bytes,
    finished: bool,
    handle: Arc<SharedStreamHandleInner>,
    state: FlacStreamState,
    current_epoch: u64,
}

impl IcyClientStream {
    pub(crate) fn new(
        rx: timed_broadcast::Receiver<Bytes>,
        handle: Arc<SharedStreamHandleInner>,
        metaint: usize,
    ) -> Self {
        Self {
            rx,
            metadata: handle.metadata.clone(),
            metaint,
            byte_count: 0,
            buffer: VecDeque::new(),
            current_metadata_version: 0,
            cached_icy_metadata: Bytes::new(),
            finished: false,
            handle,
            state: FlacStreamState::SendingHeader,
            current_epoch: 0,
        }
    }

    pub fn current_epoch(&self) -> u64 {
        self.current_epoch
    }
}

impl IcyClientStream {
    /// Format metadata as ICY metadata block.
    ///
    /// ICY format: StreamTitle='Artist - Title';StreamUrl='url';
    /// Padded to multiple of 16 bytes, prefixed with length byte.
    ///
    /// If cover_pk is available, constructs a URL for the cover image:
    /// - If pmoserver is initialized: http://server/covers/image/{pk}/256
    /// - Otherwise: relative URL /covers/image/{pk}/256
    fn format_icy_metadata(meta: &MetadataSnapshot) -> Bytes {
        let title = meta.title.as_deref().unwrap_or("Unknown");
        let artist = meta.artist.as_deref().unwrap_or("Unknown Artist");

        // Build ICY metadata string with cover URL if available
        let mut metadata_str = format!("StreamTitle='{} - {}';", artist, title);

        // Add cover URL if we have a cover_pk
        if let Some(pk) = &meta.cover_pk {
            // Use relative URL /covers/image/{pk}/256
            // This works when streaming from the same server that serves covers
            // VLC and other players will resolve relative URLs correctly
            metadata_str.push_str(&format!("StreamUrl='/covers/image/{}/256';", pk));
        } else if let Some(url) = &meta.cover_url {
            // Fallback to external cover URL if no local pk
            metadata_str.push_str(&format!("StreamUrl='{}';", url));
        }

        // ICY metadata is padded to multiple of 16 bytes
        let metadata_bytes = metadata_str.as_bytes();
        let length = metadata_bytes.len();
        let padded_length = ((length + 15) / 16) * 16;
        let length_byte = (padded_length / 16) as u8;

        let mut result = Vec::with_capacity(1 + padded_length);
        result.push(length_byte);
        result.extend_from_slice(metadata_bytes);
        result.resize(1 + padded_length, 0); // Pad with zeros

        Bytes::from(result)
    }

    /// Get metadata block if it needs to be inserted.
    #[allow(dead_code)]
    async fn get_metadata_if_changed(&mut self) -> Option<Bytes> {
        let meta = self.metadata.read().await;
        if meta.version > self.current_metadata_version {
            self.current_metadata_version = meta.version;
            let icy_meta = Self::format_icy_metadata(&meta);
            self.cached_icy_metadata = icy_meta.clone();
            Some(icy_meta)
        } else if self.byte_count == 0 {
            // Always send metadata at the start
            Some(self.cached_icy_metadata.clone())
        } else {
            // No change, send empty metadata block
            Some(Bytes::from(vec![0u8]))
        }
    }
}

impl AsyncRead for IcyClientStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        loop {
            // If in header state, send the header first
            if matches!(self.state, FlacStreamState::SendingHeader) {
                let header_opt = if let Ok(guard) = self.handle.header.try_read() {
                    guard.clone()
                } else {
                    None
                };

                if let Some(header) = header_opt {
                    self.buffer.extend(header.iter());
                    debug!(
                        "Sending cached FLAC header to new ICY client ({} bytes)",
                        header.len()
                    );
                    self.state = FlacStreamState::Streaming;
                    continue; // Now copy header to output buffer
                } else {
                    // Header not yet captured - client will receive it via broadcast
                    // Skip directly to streaming to avoid blocking
                    debug!(
                        "FLAC header not yet available, ICY client will receive it via broadcast"
                    );
                    self.state = FlacStreamState::Streaming;
                }
            }

            // If we have buffered data, copy it
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

            // Check if we need to insert metadata
            if self.byte_count % self.metaint == 0 && self.byte_count > 0 {
                // Time to insert ICY metadata
                // Use try_read to avoid blocking in poll context
                let update = {
                    if let Ok(meta) = self.metadata.try_read() {
                        if meta.version > self.current_metadata_version {
                            Some((meta.version, Self::format_icy_metadata(&meta)))
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                };

                if let Some((new_version, new_metadata)) = update {
                    self.current_metadata_version = new_version;
                    self.cached_icy_metadata = new_metadata;
                }

                let icy_data = self.cached_icy_metadata.clone();
                self.buffer.extend(icy_data.iter());
                self.byte_count = 0; // Reset counter after metadata
                continue;
            }

            // Try to receive audio data
            match self.rx.try_recv() {
                Ok(packet) => {
                    self.current_epoch = packet.epoch;
                    // Calculate how many bytes until next metadata block
                    let until_metadata = self.metaint - (self.byte_count % self.metaint);
                    let to_buffer = packet.payload.len().min(until_metadata);

                    self.buffer.extend(packet.payload[..to_buffer].iter());
                    self.byte_count += to_buffer;

                    // If we have more data, we'll process it in the next iteration
                    if to_buffer < packet.payload.len() {
                        // Save remaining for next iteration
                        // For now, we'll just drop it and get it again
                        // TODO: Improve this
                    }
                }
                Err(TryRecvError::Empty) => {
                    // No data available right now.
                    // Schedule a wakeup after a small delay to avoid busy-loop polling.
                    let waker = cx.waker().clone();
                    tokio::spawn(async move {
                        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
                        waker.wake();
                    });
                    return Poll::Pending;
                }
                Err(TryRecvError::Lagged(skipped)) => {
                    warn!("ICY client lagged, skipped {} messages", skipped);
                }
                Err(TryRecvError::Closed) => {
                    self.finished = true;
                    return Poll::Ready(Ok(()));
                }
            }
        }
    }
}

impl Drop for IcyClientStream {
    fn drop(&mut self) {
        let remaining = self.handle.client_disconnected();
        debug!("ICY client disconnected (remaining: {})", remaining);
    }
}
