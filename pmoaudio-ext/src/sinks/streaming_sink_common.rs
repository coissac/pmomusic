use std::collections::VecDeque;
use std::future::Future;
use std::io;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use bytes::Bytes;
use pmoaudio::AudioError;
use pmoflac::{encode_flac_stream, EncoderOptions, FlacEncodedStream, PcmFormat};
use pmometadata::TrackMetadata;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, ReadBuf};
use tokio::sync::{mpsc, RwLock};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, trace, warn};

use crate::byte_stream_reader::{ByteStreamReader, PcmChunk};
use crate::sinks::timed_broadcast::{self, TryRecvError};

/// Snapshot of track metadata shared across streaming sinks.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MetadataSnapshot {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration: Option<Duration>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cover_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cover_pk: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track_number: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub album_artist: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub genre: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub year: Option<u32>,
    pub audio_timestamp_sec: f64,
    pub version: u64,
}

/// Configuration options shared by streaming sinks.
#[derive(Clone, Debug)]
pub struct StreamingSinkOptions {
    pub restart_encoder_on_track_boundary: bool,
    pub enable_total_samples: bool,
    pub default_title: Option<String>,
    pub default_artist: Option<String>,
    pub use_only_default_metadata: bool,
    pub server_base_url: Option<String>,
}

impl StreamingSinkOptions {
    pub fn flac_defaults() -> Self {
        Self {
            restart_encoder_on_track_boundary: false,
            enable_total_samples: false,
            default_title: None,
            default_artist: None,
            use_only_default_metadata: false,
            server_base_url: None,
        }
    }

    pub fn ogg_defaults() -> Self {
        Self {
            restart_encoder_on_track_boundary: true,
            enable_total_samples: true,
            default_title: None,
            default_artist: None,
            use_only_default_metadata: false,
            server_base_url: None,
        }
    }

    pub fn with_restart(mut self, restart: bool) -> Self {
        self.restart_encoder_on_track_boundary = restart;
        self
    }

    pub fn with_total_samples(mut self, enable: bool) -> Self {
        self.enable_total_samples = enable;
        self
    }

    pub fn with_default_title(mut self, title: impl Into<Option<String>>) -> Self {
        self.default_title = title.into();
        self
    }

    pub fn with_default_artist(mut self, artist: impl Into<Option<String>>) -> Self {
        self.default_artist = artist.into();
        self
    }

    pub fn with_only_default_metadata(mut self, only_default: bool) -> Self {
        self.use_only_default_metadata = only_default;
        self
    }

    pub fn with_server_base_url(mut self, url: impl Into<Option<String>>) -> Self {
        self.server_base_url = url.into();
        self
    }
}

/// Shared handle state for streaming sinks.
pub struct SharedStreamHandleInner {
    pub broadcast: timed_broadcast::Sender<Bytes>,
    pub metadata: Arc<RwLock<MetadataSnapshot>>,
    pub active_clients: Arc<AtomicUsize>,
    pub stop_token: CancellationToken,
    pub header: Arc<RwLock<Option<Bytes>>>,
    pub auto_stop: Arc<AtomicBool>,
}

impl SharedStreamHandleInner {
    pub fn new(
        broadcast: timed_broadcast::Sender<Bytes>,
        metadata: Arc<RwLock<MetadataSnapshot>>,
        stop_token: CancellationToken,
        header: Arc<RwLock<Option<Bytes>>>,
        auto_stop: Arc<AtomicBool>,
    ) -> Self {
        Self {
            broadcast,
            metadata,
            active_clients: Arc::new(AtomicUsize::new(0)),
            stop_token,
            header,
            auto_stop,
        }
    }

    pub fn register_client(&self) -> timed_broadcast::Receiver<Bytes> {
        self.broadcast.subscribe()
    }

    pub fn client_connected(&self) -> usize {
        self.active_clients.fetch_add(1, Ordering::SeqCst) + 1
    }

    pub fn client_disconnected(&self) -> usize {
        let prev = self.active_clients.fetch_sub(1, Ordering::SeqCst);
        let remaining = prev.saturating_sub(1);
        if prev == 1 && self.auto_stop.load(Ordering::SeqCst) {
            trace!("Last client disconnected, signaling pipeline stop (shared handle)");
            self.stop_token.cancel();
        }
        remaining
    }
}

enum StreamState {
    SendingHeader,
    Streaming,
}

pub struct SharedClientStream {
    rx: timed_broadcast::Receiver<Bytes>,
    buffer: VecDeque<u8>,
    finished: bool,
    handle: Arc<SharedStreamHandleInner>,
    state: StreamState,
    current_epoch: u64,
}

impl SharedClientStream {
    pub fn new(rx: timed_broadcast::Receiver<Bytes>, handle: Arc<SharedStreamHandleInner>) -> Self {
        Self {
            rx,
            buffer: VecDeque::new(),
            finished: false,
            handle,
            state: StreamState::SendingHeader,
            current_epoch: 0,
        }
    }

    pub fn current_epoch(&self) -> u64 {
        self.current_epoch
    }

    pub fn handle(&self) -> &Arc<SharedStreamHandleInner> {
        &self.handle
    }
}

impl AsyncRead for SharedClientStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        loop {
            if matches!(self.state, StreamState::SendingHeader) {
                let header_opt = if let Ok(guard) = self.handle.header.try_read() {
                    guard.clone()
                } else {
                    None
                };

                if let Some(header) = header_opt {
                    self.buffer.extend(header.iter());
                    trace!(
                        "Sending cached header to new client ({} bytes)",
                        header.len()
                    );
                    self.state = StreamState::Streaming;
                    continue;
                } else {
                    self.state = StreamState::Streaming;
                }
            }

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

            match self.rx.try_recv() {
                Ok(packet) => {
                    self.current_epoch = packet.epoch;
                    self.buffer.extend(packet.payload.iter());
                }
                Err(TryRecvError::Empty) => {
                    let waker = cx.waker().clone();
                    tokio::spawn(async move {
                        tokio::time::sleep(Duration::from_millis(10)).await;
                        waker.wake();
                    });
                    return Poll::Pending;
                }
                Err(TryRecvError::Lagged(skipped)) => {
                    warn!("Client lagged, skipped {} messages", skipped);
                }
                Err(TryRecvError::Closed) => {
                    self.finished = true;
                    return Poll::Ready(Ok(()));
                }
            }
        }
    }
}

pub struct EncoderState {
    pub broadcaster_task: JoinHandle<()>,
}

pub struct SharedSinkContext {
    pub encoder_options: EncoderOptions,
    pub bits_per_sample: u8,
    /// Whether to propagate total_samples into STREAMINFO.
    /// For unbounded live streams (raw FLAC), this must stay false to avoid
    /// players stopping after they reach the advertised length.
    pub enable_total_samples: bool,
    pub restart_encoder_on_track_boundary: bool,
    pub default_title: Option<String>,
    pub default_artist: Option<String>,
    pub use_only_default_metadata: bool,
    pub pcm_tx: Option<mpsc::Sender<PcmChunk>>,
    pub pcm_rx: Option<mpsc::Receiver<PcmChunk>>,
    pub metadata: Arc<RwLock<MetadataSnapshot>>,
    pub broadcast: timed_broadcast::Sender<Bytes>,
    pub header: Arc<RwLock<Option<Bytes>>>,
    pub encoder_state: Option<EncoderState>,
    pub sample_rate: Option<u32>,
    pub broadcast_max_lead_time: f64,
    pub first_chunk_timestamp_checked: bool,
    pub timestamp_offset_sec: f64,
    pub current_timestamp: Arc<RwLock<f64>>,
    pub pending_track_duration: Option<Duration>,
    pub pending_total_samples: Option<u64>,
}

impl SharedSinkContext {
    pub async fn initialize_encoder<Fut, F>(
        &mut self,
        sample_rate: u32,
        timestamp_offset_sec: f64,
        broadcaster: F,
    ) -> Result<(), AudioError>
    where
        F: FnOnce(
                FlacEncodedStream,
                timed_broadcast::Sender<Bytes>,
                Arc<RwLock<Option<Bytes>>>,
                Arc<RwLock<f64>>,
                Arc<RwLock<f64>>,
                f64,
                u32,
                f64,
            ) -> Fut
            + Send
            + 'static,
        Fut: Future<Output = Result<(), AudioError>> + Send + 'static,
    {
        if self.encoder_state.is_some() {
            return Ok(());
        }

        debug!(
            "Initializing FLAC encoder with sample rate: {} Hz",
            sample_rate
        );

        let pcm_rx = self
            .pcm_rx
            .take()
            .ok_or_else(|| AudioError::ProcessingError("PCM receiver already consumed".into()))?;

        let current_timestamp = self.current_timestamp.clone();
        let current_duration = Arc::new(RwLock::new(0.0f64));

        let pcm_reader =
            ByteStreamReader::new(pcm_rx, current_timestamp.clone(), current_duration.clone());

        let pcm_format = PcmFormat {
            sample_rate,
            channels: 2,
            bits_per_sample: self.bits_per_sample,
        };

        let flac_stream = encode_flac_stream(pcm_reader, pcm_format, self.encoder_options.clone())
            .await
            .map_err(|e| {
                AudioError::ProcessingError(format!("Failed to start FLAC encoder: {}", e))
            })?;

        debug!("FLAC encoder initialized successfully");

        let broadcast = self.broadcast.clone();
        let header = self.header.clone();
        let max_lead = self.broadcast_max_lead_time;
        let current_timestamp_clone = current_timestamp.clone();
        let current_duration_clone = current_duration.clone();

        let broadcaster_task = tokio::spawn(async move {
            if let Err(e) = broadcaster(
                flac_stream,
                broadcast,
                header,
                current_timestamp_clone,
                current_duration_clone,
                max_lead,
                sample_rate,
                timestamp_offset_sec,
            )
            .await
            {
                error!("Broadcaster task error: {}", e);
            }
        });

        self.encoder_state = Some(EncoderState { broadcaster_task });

        Ok(())
    }

    /// Prepare encoder options for a new track so the next FLAC header embeds up-to-date metadata
    /// and duration (total_samples) when available.
    pub async fn prepare_encoder_options_for_track(
        &mut self,
        metadata_lock: &Arc<RwLock<dyn TrackMetadata>>,
    ) -> Result<(), AudioError> {
        debug!("Encoder metadata: preparing metadata");
        // Always pass the metadata handle to the encoder so Vorbis comments are emitted.
        self.encoder_options.metadata = Some(metadata_lock.clone());

        // Capture duration (if any) to set total_samples.
        let duration_opt = {
            let metadata = metadata_lock.read().await;
            metadata.get_duration().await.ok().flatten()
        };
        self.pending_track_duration = duration_opt;
        self.pending_total_samples = {
            let metadata = metadata_lock.read().await;
            metadata.get_total_samples().await.ok().flatten()
        };

        debug!(
            "Encoder metadata: from TrackBoundary - duration={:?}s, total_samples={:?}",
            self.pending_track_duration
                .as_ref()
                .map(|d| d.as_secs_f64()),
            self.pending_total_samples
        );

        // In raw FLAC live streaming we must NOT advertise a total_samples value,
        // otherwise players think the stream ends after the first track.
        if !self.enable_total_samples {
            self.encoder_options.total_samples = None;
            debug!("Encoder metadata: total_samples disabled for this sink (enable_total_samples=false)");
            return Ok(());
        }

        // Compute total_samples only when we know the sample rate.
        self.refresh_total_samples_with_sample_rate();

        Ok(())
    }

    /// Refresh total_samples when the sample rate is learned after metadata was already set.
    pub fn refresh_total_samples_with_sample_rate(&mut self) {
        info!(
            "Encoder metadata: refresh_total_samples (pending_total_samples={:?}, pending_duration={:?}, sample_rate={:?})",
            self.pending_total_samples,
            self.pending_track_duration
                .as_ref()
                .map(Duration::as_secs_f64),
            self.sample_rate
        );

        if !self.enable_total_samples {
            self.encoder_options.total_samples = None;
            info!("Encoder metadata: total_samples disabled for live streaming");
            return;
        }

        if let Some(total) = self.pending_total_samples {
            self.encoder_options.total_samples = Some(total);
            info!(
                "Encoder metadata: using provided total_samples={} from TrackBoundary",
                total
            );
            return;
        }

        if let (Some(duration), Some(sr)) = (self.pending_track_duration, self.sample_rate) {
            let samples = (duration.as_secs_f64() * sr as f64).round() as u64;
            self.encoder_options.total_samples = Some(samples);
            info!(
                "Encoder metadata: computed total_samples={} (duration {:.3}s @ {} Hz)",
                samples,
                duration.as_secs_f64(),
                sr
            );
        } else {
            // Avoid leaking the previous track's length.
            self.encoder_options.total_samples = None;
            info!("Encoder metadata: no duration/total_samples available; clearing total_samples in encoder options");
        }
    }

    pub async fn restart_encoder_for_new_track<Fut, F>(
        &mut self,
        broadcaster: F,
    ) -> Result<(), AudioError>
    where
        F: FnOnce(
                FlacEncodedStream,
                timed_broadcast::Sender<Bytes>,
                Arc<RwLock<Option<Bytes>>>,
                Arc<RwLock<f64>>,
                Arc<RwLock<f64>>,
                f64,
                u32,
                f64,
            ) -> Fut
            + Send
            + 'static,
        Fut: Future<Output = Result<(), AudioError>> + Send + 'static,
    {
        let sample_rate = self
            .sample_rate
            .ok_or_else(|| AudioError::ProcessingError("Sample rate not initialized".into()))?;

        debug!("Restarting FLAC encoder for new track");

        let last_timestamp = *self.current_timestamp.read().await;
        debug!("Last timestamp before restart: {:.3}s", last_timestamp);

        if let Some(tx) = self.pcm_tx.take() {
            drop(tx);
            trace!("Dropped PCM sender to signal encoder finish");
        }

        if let Some(state) = self.encoder_state.take() {
            trace!("Waiting for broadcaster task to finish...");
            match state.broadcaster_task.await {
                Ok(_) => trace!("Broadcaster task finished successfully"),
                Err(e) => warn!("Broadcaster task error during restart: {:?}", e),
            }
        }

        self.timestamp_offset_sec += last_timestamp;
        debug!("New timestamp offset: {:.3}s", self.timestamp_offset_sec);

        let (pcm_tx, pcm_rx) = mpsc::channel::<PcmChunk>(16);
        self.pcm_tx = Some(pcm_tx);
        self.pcm_rx = Some(pcm_rx);

        // self.initialize_encoder(sample_rate, self.timestamp_offset_sec, broadcaster)
        //    .await?;

        self.initialize_encoder(sample_rate, 0.0, broadcaster)
            .await?;

        debug!("FLAC encoder restarted successfully for new track");
        Ok(())
    }

    pub async fn update_metadata(
        &mut self,
        metadata_lock: &Arc<RwLock<dyn TrackMetadata>>,
        timestamp_sec: f64,
    ) -> Result<(), AudioError> {
        let metadata = metadata_lock.read().await;
        let mut snapshot = self.metadata.write().await;

        // Title / artist with default fallback or forced default.
        if self.use_only_default_metadata {
            snapshot.title = self.default_title.clone();
            snapshot.artist = self.default_artist.clone();
        } else {
            snapshot.title = metadata
                .get_title()
                .await
                .ok()
                .flatten()
                .or_else(|| self.default_title.clone());
            snapshot.artist = metadata
                .get_artist()
                .await
                .ok()
                .flatten()
                .or_else(|| self.default_artist.clone());
        }
        snapshot.album = metadata.get_album().await.ok().flatten();
        snapshot.duration = metadata.get_duration().await.ok().flatten();
        snapshot.cover_url = metadata.get_cover_url().await.ok().flatten();
        snapshot.cover_pk = metadata.get_cover_pk().await.ok().flatten();
        snapshot.year = metadata.get_year().await.ok().flatten();

        if let Ok(Some(extra)) = metadata.get_extra().await {
            snapshot.genre = extra.get("genre").cloned();
            snapshot.track_number = extra
                .get("track_number")
                .and_then(|s| s.parse::<u32>().ok());
        } else {
            snapshot.genre = None;
            snapshot.track_number = None;
        }

        snapshot.audio_timestamp_sec = timestamp_sec;
        snapshot.version += 1;

        debug!(
            "Metadata updated: v{} @ {:.2}s - {} - {} (cover_pk: {:?})",
            snapshot.version,
            timestamp_sec,
            snapshot.artist.as_deref().unwrap_or("?"),
            snapshot.title.as_deref().unwrap_or("?"),
            snapshot.cover_pk
        );

        Ok(())
    }
}
