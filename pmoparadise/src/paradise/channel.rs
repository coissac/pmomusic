//! Channel orchestration primitives.
//!
//! This module wires together configuration, playlists, workers and client
//! tracking for a single Radio Paradise channel.  The implementation is still
//! a scaffolding of the final behaviour; commands sent to the worker are
//! logged but not yet executing the full download/buffering pipeline.

use super::history::HistoryBackend;
use super::playlist::{PlaylistEntry, SharedPlaylist};
use super::worker::{ParadiseWorker, WorkerCommand};
use crate::client::RadioParadiseClient;
use anyhow::{Context, Result};
use async_stream::try_stream;
use bytes::Bytes;
use futures::{stream::BoxStream, StreamExt};
use pmosource::SourceCacheManager;
use std::fmt;
use std::str::FromStr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::fs::File;
use tokio::sync::{mpsc, Mutex};
use tokio_util::io::ReaderStream;
use tracing::warn;

/// Logical identifier for a Radio Paradise channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ParadiseChannelKind {
    Main,
    Mellow,
    Rock,
    Eclectic,
}

impl ParadiseChannelKind {
    pub const fn id(self) -> u8 {
        match self {
            Self::Main => 0,
            Self::Mellow => 1,
            Self::Rock => 2,
            Self::Eclectic => 3,
        }
    }

    pub const fn slug(self) -> &'static str {
        match self {
            Self::Main => "main",
            Self::Mellow => "mellow",
            Self::Rock => "rock",
            Self::Eclectic => "eclectic",
        }
    }

    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Main => "Main Mix",
            Self::Mellow => "Mellow Mix",
            Self::Rock => "Rock Mix",
            Self::Eclectic => "Eclectic Mix",
        }
    }

    pub const fn description(self) -> &'static str {
        match self {
            Self::Main => "Eclectic mix of rock, world, electronica, and more",
            Self::Mellow => "Mellower, less aggressive music",
            Self::Rock => "Heavier, more guitar-driven music",
            Self::Eclectic => "Curated worldwide selection",
        }
    }
}

impl FromStr for ParadiseChannelKind {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "main" | "0" => Ok(Self::Main),
            "mellow" | "1" => Ok(Self::Mellow),
            "rock" | "2" => Ok(Self::Rock),
            "eclectic" | "3" => Ok(Self::Eclectic),
            other => Err(anyhow::anyhow!("Unknown Radio Paradise channel: {}", other)),
        }
    }
}

/// Metadata descriptor for a channel.
#[derive(Debug, Clone, Copy)]
pub struct ChannelDescriptor {
    pub kind: ParadiseChannelKind,
    pub id: u8,
    pub slug: &'static str,
    pub display_name: &'static str,
    pub description: &'static str,
}

impl ChannelDescriptor {
    pub const fn new(kind: ParadiseChannelKind) -> Self {
        Self {
            id: kind.id(),
            slug: kind.slug(),
            display_name: kind.display_name(),
            description: kind.description(),
            kind,
        }
    }
}

pub const ALL_CHANNELS: [ChannelDescriptor; 4] = [
    ChannelDescriptor::new(ParadiseChannelKind::Main),
    ChannelDescriptor::new(ParadiseChannelKind::Mellow),
    ChannelDescriptor::new(ParadiseChannelKind::Rock),
    ChannelDescriptor::new(ParadiseChannelKind::Eclectic),
];

/// Returns the maximum valid channel ID
pub const fn max_channel_id() -> u8 {
    (ALL_CHANNELS.len() - 1) as u8
}

/// Public handle to interact with a channel.
#[derive(Clone)]
pub struct ParadiseChannel {
    inner: Arc<ParadiseChannelInner>,
}

struct ParadiseChannelInner {
    descriptor: ChannelDescriptor,
    client: RadioParadiseClient,
    history_max_tracks: usize,
    playlist: SharedPlaylist,
    history: Arc<dyn HistoryBackend>,
    cache_manager: Arc<SourceCacheManager>,
    active_clients: AtomicUsize,
    worker_tx: mpsc::Sender<WorkerCommand>,
    worker: Mutex<Option<ParadiseWorker>>,
}

impl fmt::Debug for ParadiseChannel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ParadiseChannel")
            .field("slug", &self.inner.descriptor.slug)
            .field(
                "active_clients",
                &self.inner.active_clients.load(Ordering::SeqCst),
            )
            .finish()
    }
}

impl ParadiseChannel {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        descriptor: ChannelDescriptor,
        base_client: RadioParadiseClient,
        history_max_tracks: usize,
        history: Arc<dyn HistoryBackend>,
        cache_manager: Arc<SourceCacheManager>,
    ) -> Result<Self> {
        let client = base_client.clone_with_channel(descriptor.id);
        let playlist = SharedPlaylist::new(history_max_tracks);
        let (worker, worker_tx) = ParadiseWorker::spawn(
            descriptor,
            client.clone(),
            history_max_tracks,
            playlist.clone(),
            history.clone(),
            cache_manager.clone(),
        );

        Ok(Self {
            inner: Arc::new(ParadiseChannelInner {
                descriptor,
                client,
                history_max_tracks,
                playlist,
                history,
                cache_manager,
                active_clients: AtomicUsize::new(0),
                worker_tx,
                worker: Mutex::new(Some(worker)),
            }),
        })
    }

    pub fn descriptor(&self) -> ChannelDescriptor {
        self.inner.descriptor
    }

    pub fn playlist(&self) -> &SharedPlaylist {
        &self.inner.playlist
    }

    pub fn history_max_tracks(&self) -> usize {
        self.inner.history_max_tracks
    }

    pub fn history_backend(&self) -> &Arc<dyn HistoryBackend> {
        &self.inner.history
    }

    pub fn cache_manager(&self) -> Arc<SourceCacheManager> {
        self.inner.cache_manager.clone()
    }

    pub fn client(&self) -> &RadioParadiseClient {
        &self.inner.client
    }

    pub fn active_client_count(&self) -> usize {
        self.inner.active_clients.load(Ordering::SeqCst)
    }

    pub async fn connect_client(
        &self,
        client_id: impl Into<String>,
    ) -> Result<ParadiseClientStream> {
        let client_id = client_id.into();
        self.inner.active_clients.fetch_add(1, Ordering::SeqCst);

        if let Err(err) = self
            .inner
            .worker_tx
            .send(WorkerCommand::ClientConnected {
                client_id: client_id.clone(),
            })
            .await
        {
            self.inner.active_clients.fetch_sub(1, Ordering::SeqCst);
            return Err(anyhow::anyhow!("worker unavailable: {}", err));
        }

        self.inner.playlist.increment_all_pending().await;
        self.ensure_started().await?;

        Ok(ParadiseClientStream::new(self.clone(), client_id))
    }

    pub async fn disconnect_client(&self, client_id: impl Into<String>) -> Result<()> {
        let client_id = client_id.into();
        self.inner.active_clients.fetch_sub(1, Ordering::SeqCst);
        self.inner
            .worker_tx
            .send(WorkerCommand::ClientDisconnected { client_id })
            .await
            .context("failed to notify worker of client disconnection")?;
        Ok(())
    }

    pub async fn ensure_started(&self) -> Result<()> {
        self.inner
            .worker_tx
            .send(WorkerCommand::EnsureReady)
            .await
            .context("failed to schedule worker warmup")
    }

    pub async fn shutdown(&self) -> Result<()> {
        self.inner
            .worker_tx
            .send(WorkerCommand::Shutdown)
            .await
            .ok();

        let mut guard = self.inner.worker.lock().await;
        if let Some(worker) = guard.take() {
            worker
                .wait()
                .await
                .context("failed to join worker task")
                .map(|_| ())
        } else {
            Ok(())
        }
    }

    pub async fn mark_track_completed(&self, track: &Arc<PlaylistEntry>) {
        let remaining = track.decrement_clients();
        if remaining > 0 {
            return;
        }

        if let Some(removed) = self
            .inner
            .playlist
            .pop_front_matching(&track.track_id)
            .await
        {
            if let Err(err) = self.inner.history.append(removed.as_history_entry()).await {
                warn!(
                    channel = self.inner.descriptor.slug,
                    "Failed to persist history entry: {err:?}"
                );
            }

            if let Err(err) = self
                .inner
                .history
                .truncate(self.inner.history_max_tracks)
                .await
            {
                warn!(
                    channel = self.inner.descriptor.slug,
                    "Failed to truncate history: {err:?}"
                );
            }

            let history_entry = removed.as_history_entry();
            self.inner.playlist.push_history_entry(history_entry).await;
        }
    }
}

/// Placeholder stream handle for per-client playback.
#[derive(Debug, Clone)]
pub struct ParadiseClientStream {
    channel: ParadiseChannel,
    client_id: String,
}

impl ParadiseClientStream {
    fn new(channel: ParadiseChannel, client_id: String) -> Self {
        Self { channel, client_id }
    }

    pub fn client_id(&self) -> &str {
        &self.client_id
    }

    pub fn channel(&self) -> ParadiseChannel {
        self.channel.clone()
    }

    pub fn into_byte_stream(self) -> BoxStream<'static, Result<Bytes, anyhow::Error>> {
        let channel = self.channel.clone();
        let stream = try_stream! {
            channel.ensure_started().await?;
            let mut index = 0usize;
            loop {
                let entries = channel.playlist().active_snapshot().await;

                if index >= entries.len() {
                    channel.ensure_started().await?;
                    channel.playlist().wait_for_track_count(index).await;
                    continue;
                }

                let entry = entries[index].clone();
                index += 1;

                let audio_pk = entry
                    .audio_pk
                    .clone()
                    .ok_or_else(|| anyhow::anyhow!("Audio not cached yet"))?;

                channel
                    .cache_manager()
                    .wait_audio_ready(&audio_pk)
                    .await
                    .map_err(|e| anyhow::anyhow!(e.to_string()))?;

                let file_path = if let Some(path) = entry.file_path.clone() {
                    path
                } else {
                    channel
                        .cache_manager()
                        .audio_file_path(&audio_pk)
                        .await
                        .ok_or_else(|| anyhow::anyhow!("Audio file path unavailable"))?
                };

                let file = File::open(&file_path).await?;
                let mut reader = ReaderStream::new(file);

                while let Some(chunk) = reader.next().await {
                    let bytes = chunk?;
                    yield bytes;
                }

                channel.mark_track_completed(&entry).await;
            }
        };

        stream.boxed()
    }
}

impl Drop for ParadiseClientStream {
    fn drop(&mut self) {
        let channel = self.channel.clone();
        let client_id = self.client_id.clone();
        let slug = channel.descriptor().slug;
        tokio::spawn(async move {
            if let Err(err) = channel.disconnect_client(client_id).await {
                warn!(channel = slug, "Failed to disconnect client: {err:?}");
            }
        });
    }
}
