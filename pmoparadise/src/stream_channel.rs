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
use anyhow::Result;
use pmoaudio::AudioPipelineNode;
use pmoaudio_ext::{
    FlacClientStream, IcyClientStream, MetadataSnapshot, OggFlacClientStream, OggFlacStreamHandle,
    StreamHandle, StreamingFlacSink, StreamingOggFlacSink,
};
use pmoflac::EncoderOptions;
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
}

impl Default for ParadiseStreamChannelConfig {
    fn default() -> Self {
        Self {
            max_lead_seconds: 1.0,
        }
    }
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
}

impl ParadiseStreamChannel {
    /// Crée un canal avec client déjà configuré.
    pub fn with_client(
        descriptor: ChannelDescriptor,
        client: RadioParadiseClient,
        config: ParadiseStreamChannelConfig,
    ) -> Self {
        let mut source = RadioParadiseStreamSource::new(client.clone());
        let block_handle = source.block_handle();

        let (flac_sink, stream_handle) = StreamingFlacSink::with_max_broadcast_lead(
            EncoderOptions::default(),
            16,
            config.max_lead_seconds,
        );
        let (ogg_sink, ogg_handle) = StreamingOggFlacSink::with_max_broadcast_lead(
            EncoderOptions::default(),
            16,
            config.max_lead_seconds,
        );

        source.register(Box::new(flac_sink));
        source.register(Box::new(ogg_sink));
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
        }
    }

    /// Crée un canal en construisant automatiquement le client pour ce descriptor.
    pub async fn new(
        descriptor: ChannelDescriptor,
        config: ParadiseStreamChannelConfig,
    ) -> Result<Self> {
        let client = RadioParadiseClient::builder()
            .channel(descriptor.id)
            .build()
            .await?;
        Ok(Self::with_client(descriptor, client, config))
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

/// Gestionnaire multi-canaux.
pub struct ParadiseChannelManager {
    channels: HashMap<u8, Arc<ParadiseStreamChannel>>,
}

impl ParadiseChannelManager {
    pub fn new(channels: HashMap<u8, Arc<ParadiseStreamChannel>>) -> Self {
        Self { channels }
    }

    pub async fn with_defaults() -> Result<Self> {
        let mut map = HashMap::new();
        for descriptor in ALL_CHANNELS.iter().copied() {
            let channel =
                ParadiseStreamChannel::new(descriptor, ParadiseStreamChannelConfig::default())
                    .await?;
            map.insert(descriptor.id, Arc::new(channel));
        }
        Ok(Self { channels: map })
    }

    pub fn get(&self, id: u8) -> Option<Arc<ParadiseStreamChannel>> {
        self.channels.get(&id).cloned()
    }

    pub fn iter(&self) -> impl Iterator<Item = &Arc<ParadiseStreamChannel>> {
        self.channels.values()
    }
}
