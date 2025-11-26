use std::{
    collections::HashMap,
    pin::Pin,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    task::{Context, Poll},
    time::Duration,
};

use crate::{
    channels::{ChannelDescriptor, ParadiseChannelKind, ALL_CHANNELS},
    client::RadioParadiseClient,
    radio_paradise_stream_source::RadioParadiseStreamSource,
};
use anyhow::{anyhow, Result};
use pmoaudio::{nodes::DEFAULT_CHANNEL_SIZE, AudioPipelineNode};
use pmoaudio_ext::{
    FlacCacheSink, FlacClientStream, IcyClientStream, MetadataSnapshot, OggFlacClientStream,
    OggFlacStreamHandle, PlaylistSource, StreamHandle, StreamingFlacSink, StreamingOggFlacSink,
    TrackBoundaryCoverNode, StreamingSinkOptions,
};
use pmoaudiocache::Cache as AudioCache;
use pmocovers::Cache as CoverCache;
use pmoflac::EncoderOptions;
use pmoplaylist::WriteHandle;
use thiserror::Error;
use tokio::io::{AsyncRead, ReadBuf};
use tokio::sync::Notify;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

/// Configuration pour un canal Radio Paradise.
#[derive(Clone, Debug)]
pub struct ParadiseStreamChannelConfig {
    /// Durée maximale (en secondes) d'avance acceptée par le broadcast.
    pub max_lead_seconds: f64,
    pub flac_options: StreamingSinkOptions,
    pub ogg_options: StreamingSinkOptions,
    pub server_base_url: Option<String>,
}

impl Default for ParadiseStreamChannelConfig {
    fn default() -> Self {
        Self {
            max_lead_seconds: 1.0,
            flac_options: StreamingSinkOptions::flac_defaults(),
            ogg_options: StreamingSinkOptions::ogg_defaults(),
            server_base_url: None,
        }
    }
}

/// Options pour activer l'archivage/historique d'un canal.
pub struct ParadiseHistoryOptions {
    pub audio_cache: Arc<AudioCache>,
    pub cover_cache: Arc<CoverCache>,
    pub playlist_id: String,
    pub playlist_writer: WriteHandle,
    pub collection: Option<String>,
    pub replay_max_lead_seconds: f64,
}

/// Builder pratique pour configurer automatiquement les playlists historiques.
#[derive(Clone)]
pub struct ParadiseHistoryBuilder {
    pub audio_cache: Arc<AudioCache>,
    pub cover_cache: Arc<CoverCache>,
    pub playlist_prefix: String,
    pub playlist_title_prefix: Option<String>,
    pub max_history_tracks: Option<usize>,
    pub collection_prefix: Option<String>,
    pub replay_max_lead_seconds: f64,
}

impl ParadiseHistoryBuilder {
    pub fn new(audio_cache: Arc<AudioCache>, cover_cache: Arc<CoverCache>) -> Self {
        Self {
            audio_cache,
            cover_cache,
            playlist_prefix: "radio-paradise-history".into(),
            playlist_title_prefix: Some("Radio Paradise History".into()),
            max_history_tracks: Some(500),
            collection_prefix: Some("radio-paradise".into()),
            replay_max_lead_seconds: 1.0,
        }
    }

    pub async fn build_for_channel(
        &self,
        descriptor: &ChannelDescriptor,
    ) -> Result<ParadiseHistoryOptions, pmoplaylist::Error> {
        let playlist_id = format!("{}-{}", self.playlist_prefix, descriptor.slug);
        let manager = pmoplaylist::PlaylistManager();
        let writer = manager
            .get_persistent_write_handle(playlist_id.clone())
            .await?;

        if let Some(prefix) = &self.playlist_title_prefix {
            let title = format!("{} - {}", prefix, descriptor.display_name);
            writer.set_title(title).await?;
        }

        if let Some(capacity) = self.max_history_tracks {
            writer.set_capacity(Some(capacity)).await?;
        }

        let collection = self
            .collection_prefix
            .as_ref()
            .map(|prefix| format!("{}-{}", prefix, descriptor.slug));

        Ok(ParadiseHistoryOptions {
            audio_cache: self.audio_cache.clone(),
            cover_cache: self.cover_cache.clone(),
            playlist_id,
            playlist_writer: writer,
            collection,
            replay_max_lead_seconds: self.replay_max_lead_seconds,
        })
    }
}

struct HistoryState {
    playlist_id: String,
    audio_cache: Arc<AudioCache>,
    replay_max_lead_seconds: f64,
}

#[cfg(feature = "pmoconfig")]
impl ParadiseStreamChannelConfig {
    pub fn from_config(cfg: &pmoconfig::Config, channel: ParadiseChannelKind) -> Self {
        use serde_yaml::Value;
        let path = [
            "sources",
            "radio_paradise",
            "channels",
            channel.slug(),
            "max_lead_seconds",
        ];
        match cfg.get_value(&path) {
            Ok(Value::Number(num)) => {
                if let Some(v) = num.as_f64() {
                    Self {
                        max_lead_seconds: v.max(0.1),
                        flac_options: StreamingSinkOptions::flac_defaults(),
                        ogg_options: StreamingSinkOptions::ogg_defaults(),
                        server_base_url: None,
                    }
                } else {
                    let default = Self::default();
                    let _ =
                        cfg.set_value(&path, Value::String(default.max_lead_seconds.to_string()));
                    default
                }
            }
            Ok(Value::String(s)) => {
                if let Ok(v) = s.parse::<f64>() {
                    Self {
                        max_lead_seconds: v.max(0.1),
                        flac_options: StreamingSinkOptions::flac_defaults(),
                        ogg_options: StreamingSinkOptions::ogg_defaults(),
                        server_base_url: None,
                    }
                } else {
                    let default = Self::default();
                    let _ =
                        cfg.set_value(&path, Value::String(default.max_lead_seconds.to_string()));
                    default
                }
            }
            _ => {
                let default = Self::default();
                let _ = cfg.set_value(&path, Value::String(default.max_lead_seconds.to_string()));
                default
            }
        }
    }
}

/// Stream complet (FLAC pur + OGG-FLAC) pour un canal Radio Paradise.
pub struct ParadiseStreamChannel {
    descriptor: ChannelDescriptor,
    state: Arc<ChannelState>,
    pipeline_handle: JoinHandle<()>,
    feeder_handle: JoinHandle<()>,
    history: Option<HistoryState>,
}

impl ParadiseStreamChannel {
    /// Crée un canal avec client déjà configuré.
    pub fn with_client(
        descriptor: ChannelDescriptor,
        client: RadioParadiseClient,
        config: ParadiseStreamChannelConfig,
        cover_cache: Option<Arc<CoverCache>>,
        history: Option<ParadiseHistoryOptions>,
    ) -> Self {
        let mut source = RadioParadiseStreamSource::new(client.clone());
        let block_handle = source.block_handle();

        let (flac_sink, stream_handle) = StreamingFlacSink::with_options(
            EncoderOptions::default(),
            16,
            config.max_lead_seconds,
            config.flac_options.clone(),
        );
        let (ogg_sink, ogg_handle) = StreamingOggFlacSink::with_options(
            EncoderOptions::default(),
            16,
            config.max_lead_seconds,
            config.ogg_options.clone(),
        );

        let mut downstream_children: Vec<Box<dyn AudioPipelineNode>> = Vec::new();
        downstream_children.push(Box::new(flac_sink));
        downstream_children.push(Box::new(ogg_sink));

        let mut history_state = None;

        if let Some(history_opts) = history {
            let ParadiseHistoryOptions {
                audio_cache,
                cover_cache,
                playlist_id,
                playlist_writer,
                collection,
                replay_max_lead_seconds,
            } = history_opts;
            let mut cache_sink = FlacCacheSink::with_config(
                audio_cache.clone(),
                cover_cache,
                DEFAULT_CHANNEL_SIZE,
                EncoderOptions::default(),
                collection,
            );
            cache_sink.register_playlist(playlist_writer);
            downstream_children.push(Box::new(cache_sink));
            history_state = Some(HistoryState {
                playlist_id,
                audio_cache,
                replay_max_lead_seconds,
            });
        }

        if let Some(cache) = cover_cache {
            let mut cover_node = TrackBoundaryCoverNode::new(cache);
            for child in downstream_children {
                cover_node.register(child);
            }
            source.register(Box::new(cover_node));
        } else {
            for child in downstream_children {
                source.register(child);
            }
        }
        stream_handle.set_auto_stop(false);
        ogg_handle.set_auto_stop(false);

        let stop_token = CancellationToken::new();
        let pipeline_stop = stop_token.clone();
        let pipeline_handle = tokio::spawn(async move {
            info!(
                "RadioParadise stream pipeline started for channel {}",
                descriptor.display_name
            );
            if let Err(e) = Box::new(source).run(pipeline_stop).await {
                error!(
                    "Pipeline error for channel {}: {}",
                    descriptor.display_name, e
                );
            }
        });

        let state = Arc::new(ChannelState {
            descriptor,
            config,
            client,
            block_handle,
            stream_handle,
            ogg_handle,
            active_clients: AtomicUsize::new(0),
            activity_notify: Notify::new(),
            stop_token,
        });

        let feeder_state = state.clone();
        let feeder_handle = tokio::spawn(async move {
            feeder_state.run_scheduler().await;
        });

        Self {
            descriptor,
            state,
            pipeline_handle,
            feeder_handle,
            history: history_state,
        }
    }

    /// Crée un canal en construisant automatiquement le client pour ce descriptor.
    pub async fn new(
        descriptor: ChannelDescriptor,
        config: ParadiseStreamChannelConfig,
        cover_cache: Option<Arc<CoverCache>>,
        history: Option<ParadiseHistoryOptions>,
    ) -> Result<Self> {
        let client = RadioParadiseClient::builder()
            .channel(descriptor.id)
            .build()
            .await?;
        Ok(Self::with_client(
            descriptor,
            client,
            config,
            cover_cache,
            history,
        ))
    }

    /// S'abonne au flux FLAC pur.
    pub fn subscribe_flac(&self) -> ChannelFlacStream {
        self.state.on_client_added();
        let inner = self.state.stream_handle.subscribe_flac();
        ChannelFlacStream::new(inner, self.state.clone())
    }

    /// S'abonne au flux FLAC + ICY metadata.
    pub fn subscribe_icy(&self) -> ChannelIcyStream {
        self.state.on_client_added();
        let inner = self.state.stream_handle.subscribe_icy();
        ChannelIcyStream::new(inner, self.state.clone())
    }

    /// S'abonne au flux OGG-FLAC.
    pub fn subscribe_ogg(&self) -> ChannelOggStream {
        self.state.on_client_added();
        let inner = self.state.ogg_handle.subscribe();
        ChannelOggStream::new(inner, self.state.clone())
    }

    /// Snapshot des métadonnées actuelles.
    pub async fn metadata(&self) -> MetadataSnapshot {
        self.state.stream_handle.get_metadata().await
    }

    /// Nombre de clients actifs.
    pub fn active_clients(&self) -> usize {
        self.state.active_clients.load(Ordering::SeqCst)
    }

    pub fn descriptor(&self) -> ChannelDescriptor {
        self.descriptor
    }

    /// Lance un pipeline dédié pour rejouer l'historique (FLAC pur) pour un client.
    pub async fn stream_history_flac(
        &self,
        client_id: &str,
    ) -> Result<HistoryFlacStream, HistoryStreamError> {
        let history = self
            .history
            .as_ref()
            .ok_or(HistoryStreamError::HistoryDisabled)?;
        tracing::info!(
            "Starting historical FLAC replay for channel {} (client_id={})",
            self.descriptor.display_name,
            client_id
        );

        let reader = pmoplaylist::PlaylistManager()
            .get_read_handle(&history.playlist_id)
            .await
            .map_err(|e| HistoryStreamError::Playlist(e.to_string()))?;
        let mut source = PlaylistSource::new(reader, history.audio_cache.clone());
        let (flac_sink, handle) = StreamingFlacSink::with_options(
            EncoderOptions::default(),
            16,
            history.replay_max_lead_seconds,
            self.state.config.flac_options.clone(),
        );
        source.register(Box::new(flac_sink));
        let stop_token = CancellationToken::new();
        let mut pipeline_source = source;
        let stop_clone = stop_token.clone();
        let pipeline = tokio::spawn(async move {
            let _ = Box::new(pipeline_source).run(stop_clone).await;
        });
        let stream = handle.subscribe_flac();
        Ok(HistoryFlacStream::new(stream, stop_token, pipeline))
    }

    /// Lance un pipeline dédié pour rejouer l'historique (OGG-FLAC) pour un client.
    pub async fn stream_history_ogg(
        &self,
        client_id: &str,
    ) -> Result<HistoryOggStream, HistoryStreamError> {
        let history = self
            .history
            .as_ref()
            .ok_or(HistoryStreamError::HistoryDisabled)?;
        tracing::info!(
            "Starting historical OGG replay for channel {} (client_id={})",
            self.descriptor.display_name,
            client_id
        );

        let reader = pmoplaylist::PlaylistManager()
            .get_read_handle(&history.playlist_id)
            .await
            .map_err(|e| HistoryStreamError::Playlist(e.to_string()))?;
        let mut source = PlaylistSource::new(reader, history.audio_cache.clone());
        let (ogg_sink, handle) = StreamingOggFlacSink::with_options(
            EncoderOptions::default(),
            16,
            history.replay_max_lead_seconds,
            self.state.config.ogg_options.clone(),
        );
        source.register(Box::new(ogg_sink));
        let stop_token = CancellationToken::new();
        let mut pipeline_source = source;
        let stop_clone = stop_token.clone();
        let pipeline = tokio::spawn(async move {
            let _ = Box::new(pipeline_source).run(stop_clone).await;
        });
        let stream = handle.subscribe();
        Ok(HistoryOggStream::new(stream, stop_token, pipeline))
    }
}

impl Drop for ParadiseStreamChannel {
    fn drop(&mut self) {
        self.state.stop_token.cancel();
        self.pipeline_handle.abort();
        self.feeder_handle.abort();
    }
}

struct ChannelState {
    descriptor: ChannelDescriptor,
    config: ParadiseStreamChannelConfig,
    client: RadioParadiseClient,
    block_handle: crate::radio_paradise_stream_source::BlockQueueHandle,
    stream_handle: StreamHandle,
    ogg_handle: OggFlacStreamHandle,
    active_clients: AtomicUsize,
    activity_notify: Notify,
    stop_token: CancellationToken,
}

impl ChannelState {
    fn on_client_added(&self) {
        if self.active_clients.fetch_add(1, Ordering::SeqCst) == 0 {
            self.activity_notify.notify_one();
        }
    }

    fn on_client_removed(&self) {
        self.active_clients.fetch_sub(1, Ordering::SeqCst);
    }

    async fn wait_for_clients(&self) -> bool {
        while self.active_clients.load(Ordering::SeqCst) == 0 {
            tokio::select! {
                _ = self.stop_token.cancelled() => return false,
                _ = self.activity_notify.notified() => {},
            }
        }
        true
    }

    async fn run_scheduler(self: Arc<Self>) {
        let mut backoff = Duration::from_secs(5);
        loop {
            if self.stop_token.is_cancelled() {
                break;
            }

            if !self.wait_for_clients().await {
                break;
            }

            match self.client.get_block(None).await {
                Ok(block) => {
                    info!(
                        "Channel {} streaming block {}",
                        self.descriptor.display_name, block.event
                    );
                    self.block_handle.enqueue(block.event);
                    let mut next_event = block.end_event;

                    loop {
                        if self.stop_token.is_cancelled() {
                            return;
                        }

                        if self.active_clients.load(Ordering::SeqCst) == 0 {
                            break;
                        }

                        match self.client.get_block(Some(next_event)).await {
                            Ok(next_block) => {
                                self.block_handle.enqueue(next_block.event);
                                next_event = next_block.end_event;
                                backoff = Duration::from_secs(5);
                            }
                            Err(e) => {
                                warn!(
                                    "Failed to fetch next block for channel {}: {}",
                                    self.descriptor.display_name, e
                                );
                                tokio::select! {
                                    _ = self.stop_token.cancelled() => return,
                                    _ = tokio::time::sleep(backoff) => {},
                                }
                                backoff = (backoff * 2).min(Duration::from_secs(60));
                            }
                        }
                    }
                }
                Err(e) => {
                    warn!(
                        "Failed to fetch current block for channel {}: {}",
                        self.descriptor.display_name, e
                    );
                    tokio::select! {
                        _ = self.stop_token.cancelled() => break,
                        _ = tokio::time::sleep(backoff) => {},
                    }
                    backoff = (backoff * 2).min(Duration::from_secs(60));
                }
            }
        }
    }
}

macro_rules! wrap_stream {
    ($name:ident, $inner:ty) => {
        pub struct $name {
            inner: $inner,
            state: Arc<ChannelState>,
        }

        impl $name {
            fn new(inner: $inner, state: Arc<ChannelState>) -> Self {
                Self { inner, state }
            }
        }

        impl AsyncRead for $name {
            fn poll_read(
                mut self: Pin<&mut Self>,
                cx: &mut Context<'_>,
                buf: &mut ReadBuf<'_>,
            ) -> Poll<std::io::Result<()>> {
                Pin::new(&mut self.inner).poll_read(cx, buf)
            }
        }

        impl Drop for $name {
            fn drop(&mut self) {
                self.state.on_client_removed();
            }
        }
    };
}

wrap_stream!(ChannelFlacStream, FlacClientStream);
wrap_stream!(ChannelIcyStream, IcyClientStream);
wrap_stream!(ChannelOggStream, OggFlacClientStream);

#[derive(Debug, Error)]
pub enum HistoryStreamError {
    #[error("history replay not enabled for this channel")]
    HistoryDisabled,
    #[error("playlist error: {0}")]
    Playlist(String),
}

pub struct HistoryFlacStream {
    inner: FlacClientStream,
    stop_token: CancellationToken,
    pipeline: Option<JoinHandle<()>>,
}

impl HistoryFlacStream {
    fn new(
        inner: FlacClientStream,
        stop_token: CancellationToken,
        pipeline: JoinHandle<()>,
    ) -> Self {
        Self {
            inner,
            stop_token,
            pipeline: Some(pipeline),
        }
    }
}

impl AsyncRead for HistoryFlacStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

impl Unpin for HistoryFlacStream {}

impl Drop for HistoryFlacStream {
    fn drop(&mut self) {
        self.stop_token.cancel();
        if let Some(handle) = self.pipeline.take() {
            handle.abort();
        }
    }
}

pub struct HistoryOggStream {
    inner: OggFlacClientStream,
    stop_token: CancellationToken,
    pipeline: Option<JoinHandle<()>>,
}

impl HistoryOggStream {
    fn new(
        inner: OggFlacClientStream,
        stop_token: CancellationToken,
        pipeline: JoinHandle<()>,
    ) -> Self {
        Self {
            inner,
            stop_token,
            pipeline: Some(pipeline),
        }
    }
}

impl AsyncRead for HistoryOggStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

impl Unpin for HistoryOggStream {}

impl Drop for HistoryOggStream {
    fn drop(&mut self) {
        self.stop_token.cancel();
        if let Some(handle) = self.pipeline.take() {
            handle.abort();
        }
    }
}

/// Gestionnaire multi-canaux.
pub struct ParadiseChannelManager {
    channels: HashMap<u8, Arc<ParadiseStreamChannel>>,
}

impl ParadiseChannelManager {
    pub fn new(channels: HashMap<u8, Arc<ParadiseStreamChannel>>) -> Self {
        Self { channels }
    }

    pub async fn with_defaults_with_cover_cache(
        cover_cache: Option<Arc<CoverCache>>,
        history_builder: Option<ParadiseHistoryBuilder>,
        server_base_url: Option<String>,
    ) -> Result<Self> {
        let mut map = HashMap::new();
        for descriptor in ALL_CHANNELS.iter().copied() {
            let mut config = ParadiseStreamChannelConfig::default();
            config.server_base_url = server_base_url.clone();

            let history_opts = if let Some(builder) = &history_builder {
                Some(
                    builder
                        .build_for_channel(&descriptor)
                        .await
                        .map_err(|e| anyhow!("Failed to init history playlist: {}", e))?,
                )
            } else {
                None
            };
            let channel = ParadiseStreamChannel::new(
                descriptor,
                config,
                cover_cache.clone(),
                history_opts,
            )
            .await?;
            map.insert(descriptor.id, Arc::new(channel));
        }
        Ok(Self { channels: map })
    }

    pub async fn with_defaults() -> Result<Self> {
        Self::with_defaults_with_cover_cache(None, None, None).await
    }

    pub fn get(&self, id: u8) -> Option<Arc<ParadiseStreamChannel>> {
        self.channels.get(&id).cloned()
    }

    pub fn iter(&self) -> impl Iterator<Item = &Arc<ParadiseStreamChannel>> {
        self.channels.values()
    }
}
