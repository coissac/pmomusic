//! Version simplifiée de stream_channel.rs utilisant RadioParadisePlaylistFeeder + PlaylistSource
//!
//! Cette version remplace l'architecture complexe RadioParadiseStreamSource par :
//! - RadioParadisePlaylistFeeder : télécharge les URLs gapless et alimente une playlist
//! - PlaylistSource::with_history() : lit la playlist et gère l'historique automatiquement

use std::{
    collections::HashMap,
    pin::Pin,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    task::{Context, Poll},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use crate::{
    channels::{ChannelDescriptor, ParadiseChannelKind, ALL_CHANNELS},
    client::RadioParadiseClient,
    models::{Block, EventId},
    playlist_feeder::RadioParadisePlaylistFeeder,
};
use anyhow::{anyhow, Context as AnyhowContext, Result};
use once_cell::sync::OnceCell;
use pmoaudio::{AudioError, AudioPipelineNode};
use pmoaudio_ext::{
    FlacClientStream, IcyClientStream, MetadataSnapshot, OggFlacClientStream, OggFlacStreamHandle,
    PlaylistSource, StreamHandle, StreamingFlacSink, StreamingOggFlacSink, StreamingSinkOptions,
    TrackBoundaryCoverNode,
};
use pmoaudiocache::{get_audio_cache, Cache as AudioCache};
use pmocovers::{get_cover_cache, Cache as CoverCache};
use pmoflac::EncoderOptions;
use pmoplaylist::PlaylistManager;
use thiserror::Error;
use tokio::io::{AsyncRead, ReadBuf};
use tokio::sync::{Mutex, Notify};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

/// Configuration pour un canal Radio Paradise.
#[derive(Clone, Debug)]
pub struct ParadiseStreamChannelConfig {
    /// Durée maximale (en secondes) d'avance acceptée par le broadcast.
    pub max_lead_seconds: f64,
    /// Options pour le flux FLAC pur.
    pub flac_options: StreamingSinkOptions,
    /// Options pour le flux OGG-FLAC.
    pub ogg_options: StreamingSinkOptions,
    /// URL de base du serveur (pour les métadonnées, covers...)
    pub server_base_url: Option<String>,
}

impl Default for ParadiseStreamChannelConfig {
    fn default() -> Self {
        Self {
            max_lead_seconds: 3.0, // Compromis live/fluidité : assez pour absorber les transitions
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
    pub collection: Option<String>,
    pub replay_max_lead_seconds: f64,
    pub max_history_tracks: Option<usize>,
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
            replay_max_lead_seconds: 3.0, // Aligné avec le live
        }
    }

    pub async fn build_for_channel(
        &self,
        descriptor: &ChannelDescriptor,
    ) -> Result<ParadiseHistoryOptions, pmoplaylist::Error> {
        let playlist_id = format!("{}-{}", self.playlist_prefix, descriptor.slug);

        let collection = self
            .collection_prefix
            .as_ref()
            .map(|prefix| format!("{}-{}", prefix, descriptor.slug));

        Ok(ParadiseHistoryOptions {
            audio_cache: self.audio_cache.clone(),
            cover_cache: self.cover_cache.clone(),
            playlist_id,
            collection,
            replay_max_lead_seconds: self.replay_max_lead_seconds,
            max_history_tracks: self.max_history_tracks,
        })
    }
}

impl Default for ParadiseHistoryBuilder {
    fn default() -> Self {
        let audio_cache = get_audio_cache()
            .expect("pmoaudiocache::register_audio_cache must be called before using ParadiseHistoryBuilder::default()");
        let cover_cache = get_cover_cache()
            .expect("pmocovers::register_cover_cache must be called before using ParadiseHistoryBuilder::default()");
        Self::new(audio_cache, cover_cache)
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
///
/// Version simplifiée utilisant RadioParadisePlaylistFeeder + PlaylistSource
pub struct ParadiseStreamChannel {
    descriptor: ChannelDescriptor,
    state: Arc<ChannelState>,
    pipeline_handle: JoinHandle<()>,
    feeder_handle: JoinHandle<()>,
}

impl ParadiseStreamChannel {
    /// Crée un canal avec client déjà configuré.
    pub async fn with_client(
        descriptor: ChannelDescriptor,
        client: RadioParadiseClient,
        config: ParadiseStreamChannelConfig,
        cover_cache: Option<Arc<CoverCache>>,
        history: Option<ParadiseHistoryOptions>,
    ) -> Result<Self> {
        // Propager server_base_url dans les options pour que les encoders injectent les covers du cache
        let mut config = config;
        if let Some(ref base) = config.server_base_url {
            config.flac_options = config
                .flac_options
                .clone()
                .with_server_base_url(Some(base.clone()));
            config.ogg_options = config
                .ogg_options
                .clone()
                .with_server_base_url(Some(base.clone()));
        }
        let cover_cache = cover_cache
            .or_else(|| history.as_ref().map(|opts| opts.cover_cache.clone()))
            .or_else(|| get_cover_cache());
        let manager = PlaylistManager::get();

        // 1. Créer la playlist live pour ce canal
        let live_playlist_id = format!("radio-paradise-live-{}", descriptor.slug);
        let (feeder, live_read) = if let Some(ref history_opts) = history {
            RadioParadisePlaylistFeeder::new(
                client.clone(),
                history_opts.audio_cache.clone(),
                history_opts.cover_cache.clone(),
                live_playlist_id.clone(),
                history_opts.collection.clone(),
            )
            .await?
        } else {
            // Pas d'historique, on a besoin quand même d'un cache audio basique
            return Err(anyhow!(
                "History options required for now (audio cache needed)"
            ));
        };

        let feeder = Arc::new(feeder);

        // 2. Créer/récupérer la playlist historique si activée
        let history_write = if let Some(ref history_opts) = history {
            let write = manager
                .get_persistent_write_handle(history_opts.playlist_id.clone())
                .await?;

            // Configurer la capacité
            if let Some(capacity) = history_opts.max_history_tracks {
                write.set_capacity(Some(capacity)).await?;
            }

            // Configurer le titre
            let title = format!("Radio Paradise History - {}", descriptor.display_name);
            write.set_title(title).await?;

            Some(Arc::new(write))
        } else {
            None
        };

        // 3. Créer la source playlist avec historique
        let audio_cache = history.as_ref().unwrap().audio_cache.clone();
        let mut source = if let Some(history_write) = history_write.clone() {
            PlaylistSource::with_history(live_read, audio_cache.clone(), history_write)
        } else {
            PlaylistSource::new(live_read, audio_cache.clone())
        };

        // 4. Créer les sinks de broadcast (FLAC + OGG)
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

        // 5. Optionnel : ajouter le nœud de cache de covers
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

        // 6. Lancer le pipeline audio
        let stop_token = CancellationToken::new();
        let pipeline_stop = stop_token.clone();
        let channel_display_name = descriptor.display_name;

        let state = Arc::new(ChannelState {
            descriptor,
            config,
            client,
            feeder: feeder.clone(),
            stream_handle,
            ogg_handle,
            history_playlist_id: history.map(|h| h.playlist_id),
            history_audio_cache: history_write.map(|_| audio_cache),
            active_clients: AtomicUsize::new(0),
            activity_notify: Notify::new(),
            stop_token,
            current_block: Mutex::new(None),
            prefetch_lock: Mutex::new(()),
        });

        let pipeline_state = state.clone();
        let pipeline_handle = tokio::spawn(async move {
            info!(
                "RadioParadise stream pipeline started for channel {}",
                channel_display_name
            );
            if let Err(e) = Box::new(source).run(pipeline_stop).await {
                error!("Pipeline error for channel {}: {}", channel_display_name, e);
                pipeline_state.handle_pipeline_error(&e).await;
            }
        });

        // 7. Lancer le feeder qui traite les blocs
        let feeder_runner = feeder.clone();
        tokio::spawn(async move {
            if let Err(e) = feeder_runner.run().await {
                error!("RadioParadisePlaylistFeeder error: {}", e);
            }
        });

        // 8. Lancer le scheduler qui enqueue les blocs
        let feeder_state = state.clone();
        let feeder_handle = tokio::spawn(async move {
            feeder_state.run_scheduler().await;
        });

        Ok(Self {
            descriptor,
            state,
            pipeline_handle,
            feeder_handle,
        })
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
        Self::with_client(descriptor, client, config, cover_cache, history).await
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
        let history_id = self
            .state
            .history_playlist_id
            .as_ref()
            .ok_or(HistoryStreamError::HistoryDisabled)?;

        let audio_cache = self
            .state
            .history_audio_cache
            .as_ref()
            .ok_or(HistoryStreamError::HistoryDisabled)?;

        tracing::info!(
            "Starting historical FLAC replay for channel {} (client_id={})",
            self.descriptor.display_name,
            client_id
        );

        let reader = pmoplaylist::PlaylistManager::get()
            .get_read_handle(history_id)
            .await
            .map_err(|e| HistoryStreamError::Playlist(e.to_string()))?;

        let mut source = PlaylistSource::new(reader, audio_cache.clone());
        let (flac_sink, handle) = StreamingFlacSink::with_max_broadcast_lead(
            EncoderOptions::default(),
            16,
            self.state.config.max_lead_seconds,
        );
        source.register(Box::new(flac_sink));
        let stop_token = CancellationToken::new();
        let stop_clone = stop_token.clone();
        let pipeline = tokio::spawn(async move {
            let _ = Box::new(source).run(stop_clone).await;
        });
        let stream = handle.subscribe_flac();
        Ok(HistoryFlacStream::new(stream, stop_token, pipeline))
    }

    /// Lance un pipeline dédié pour rejouer l'historique (OGG-FLAC) pour un client.
    pub async fn stream_history_ogg(
        &self,
        client_id: &str,
    ) -> Result<HistoryOggStream, HistoryStreamError> {
        let history_id = self
            .state
            .history_playlist_id
            .as_ref()
            .ok_or(HistoryStreamError::HistoryDisabled)?;

        let audio_cache = self
            .state
            .history_audio_cache
            .as_ref()
            .ok_or(HistoryStreamError::HistoryDisabled)?;

        tracing::info!(
            "Starting historical OGG replay for channel {} (client_id={})",
            self.descriptor.display_name,
            client_id
        );

        let reader = pmoplaylist::PlaylistManager::get()
            .get_read_handle(history_id)
            .await
            .map_err(|e| HistoryStreamError::Playlist(e.to_string()))?;

        let mut source = PlaylistSource::new(reader, audio_cache.clone());
        let (ogg_sink, handle) = StreamingOggFlacSink::with_max_broadcast_lead(
            EncoderOptions::default(),
            16,
            self.state.config.max_lead_seconds,
        );
        source.register(Box::new(ogg_sink));
        let stop_token = CancellationToken::new();
        let stop_clone = stop_token.clone();
        let pipeline = tokio::spawn(async move {
            let _ = Box::new(source).run(stop_clone).await;
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

const MAX_BLOCK_LEAD: Duration = Duration::from_secs(3600);
const BLOCK_LEAD_CHECK_CHUNK: Duration = Duration::from_secs(300);
const LIVE_PREFETCH_MIN_TRACKS: usize = 5;
const LIVE_PREFETCH_TIMEOUT: Duration = Duration::from_secs(10);
const LIVE_PREFETCH_POLL_INTERVAL: Duration = Duration::from_millis(200);
const LIVE_PREFETCH_MAX_BLOCKS: usize = 4;

static GLOBAL_CHANNEL_MANAGER: OnceCell<std::sync::Weak<ParadiseChannelManager>> = OnceCell::new();

struct ChannelState {
    descriptor: ChannelDescriptor,
    config: ParadiseStreamChannelConfig,
    client: RadioParadiseClient,
    feeder: Arc<RadioParadisePlaylistFeeder>,
    stream_handle: StreamHandle,
    ogg_handle: OggFlacStreamHandle,
    history_playlist_id: Option<String>,
    history_audio_cache: Option<Arc<AudioCache>>,
    active_clients: AtomicUsize,
    activity_notify: Notify,
    stop_token: CancellationToken,
    current_block: Mutex<Option<EventId>>,
    prefetch_lock: Mutex<()>,
}

impl ChannelState {
    fn current_unix_millis() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    }

    fn block_lead_delay(&self, block: &Block) -> Option<Duration> {
        let start = block.start_time_millis()?;
        let now = Self::current_unix_millis();
        let max_lead_ms = MAX_BLOCK_LEAD.as_millis() as u64;
        if start <= now + max_lead_ms {
            None
        } else {
            Some(Duration::from_millis(start - now - max_lead_ms))
        }
    }

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

    async fn wait_until_block_ready(&self, block: &Block) -> BlockReadiness {
        loop {
            if self.stop_token.is_cancelled() {
                return BlockReadiness::Stopped;
            }
            if self.active_clients.load(Ordering::SeqCst) == 0 {
                return BlockReadiness::NoClients;
            }

            if let Some(delay) = self.block_lead_delay(block) {
                let sleep_for = delay.min(BLOCK_LEAD_CHECK_CHUNK);
                let lead_secs = delay.as_secs_f64();
                info!(
                    "Block {} scheduled too far in the future ({:.1} min). Sleeping {:?} before retrying.",
                    block.event,
                    lead_secs / 60.0,
                    sleep_for
                );
                tokio::select! {
                    _ = self.stop_token.cancelled() => return BlockReadiness::Stopped,
                    _ = tokio::time::sleep(sleep_for) => {},
                }
                continue;
            }

            return BlockReadiness::Ready;
        }
    }

    fn live_playlist_id(&self) -> String {
        format!("radio-paradise-live-{}", self.descriptor.slug)
    }

    async fn prefetch_until_horizon(&self) -> Result<()> {
        let _guard = self.prefetch_lock.lock().await;
        let playlist_id = self.live_playlist_id();
        let manager = PlaylistManager::get();
        let reader = manager
            .get_read_handle(&playlist_id)
            .await
            .with_context(|| format!("Failed to get live playlist {}", playlist_id))?;
        let start = Instant::now();
        let mut next_event: Option<EventId> = None;
        let mut attempts = 0usize;

        loop {
            let available = reader
                .remaining()
                .await
                .with_context(|| format!("Failed to inspect playlist {}", playlist_id))?;
            if available >= LIVE_PREFETCH_MIN_TRACKS {
                return Ok(());
            }

            if start.elapsed() >= LIVE_PREFETCH_TIMEOUT {
                warn!(
                    "Prefetch timeout for channel {} ({} tracks available)",
                    self.descriptor.display_name, available
                );
                return Ok(());
            }

            if attempts >= LIVE_PREFETCH_MAX_BLOCKS {
                warn!(
                    "Prefetch block limit reached for channel {} ({} tracks available)",
                    self.descriptor.display_name, available
                );
                return Ok(());
            }

            match self.client.get_block(next_event).await {
                Ok(block) => {
                    attempts += 1;
                    next_event = Some(block.end_event);
                    self.feeder.push_block_id(block.event).await;
                }
                Err(e) => {
                    warn!(
                        "Failed to fetch block during prefetch for channel {}: {}",
                        self.descriptor.display_name, e
                    );
                    return Ok(());
                }
            }

            tokio::time::sleep(LIVE_PREFETCH_POLL_INTERVAL).await;
        }
    }

    async fn set_current_block(&self, event_id: EventId) {
        let mut guard = self.current_block.lock().await;
        *guard = Some(event_id);
    }

    async fn take_current_block(&self) -> Option<EventId> {
        self.current_block.lock().await.take()
    }

    async fn handle_pipeline_error(&self, err: &AudioError) {
        if let Some(event_id) = self.take_current_block().await {
            warn!(
                "Pipeline error while streaming block {} on channel {}: {}. Rescheduling block.",
                event_id, self.descriptor.display_name, err
            );
            self.feeder.retry_block(event_id).await;
        } else {
            warn!(
                "Pipeline error for channel {} but no tracked block: {}",
                self.descriptor.display_name, err
            );
        }
    }

    async fn run_scheduler(self: Arc<Self>) {
        let mut backoff = Duration::from_secs(5);
        'scheduler: loop {
            if self.stop_token.is_cancelled() {
                break;
            }

            if !self.wait_for_clients().await {
                break;
            }

            match self.client.get_block(None).await {
                Ok(block) => {
                    match self.wait_until_block_ready(&block).await {
                        BlockReadiness::Ready => {}
                        BlockReadiness::NoClients => continue,
                        BlockReadiness::Stopped => break,
                    }
                    info!(
                        "Channel {} streaming block {}",
                        self.descriptor.display_name, block.event
                    );
                    self.set_current_block(block.event).await;
                    self.feeder.push_block_id(block.event).await;
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
                                match self.wait_until_block_ready(&next_block).await {
                                    BlockReadiness::Ready => {}
                                    BlockReadiness::NoClients => break,
                                    BlockReadiness::Stopped => break 'scheduler,
                                }
                                self.set_current_block(next_block.event).await;
                                self.feeder.push_block_id(next_block.event).await;
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

enum BlockReadiness {
    Ready,
    NoClients,
    Stopped,
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
        tracing::warn!(
            "➡️ Entering with_defaults_with_cover_cache ({} channels, base_url={:?})",
            ALL_CHANNELS.len(),
            server_base_url
        );
        let mut map = HashMap::new();
        for descriptor in ALL_CHANNELS.iter().copied() {
            let mut config = ParadiseStreamChannelConfig::default();
            config.server_base_url = server_base_url.clone();

            let start = Instant::now();
            tracing::warn!(
                "⏳ Initializing Radio Paradise channel {} ({})...",
                descriptor.display_name,
                descriptor.slug
            );

            let history_opts = if let Some(builder) = &history_builder {
                tracing::warn!(
                    "  ⏳ Building history options for channel {} ({})",
                    descriptor.display_name,
                    descriptor.slug
                );
                Some(
                    builder
                        .build_for_channel(&descriptor)
                        .await
                        .map_err(|e| anyhow!("Failed to init history playlist: {}", e))?,
                )
            } else {
                None
            };
            tracing::warn!(
                "  ⏩ History options ready for channel {} ({})",
                descriptor.display_name,
                descriptor.slug
            );
            let channel = match tokio::time::timeout(
                Duration::from_secs(20),
                ParadiseStreamChannel::new(descriptor, config, cover_cache.clone(), history_opts),
            )
            .await
            {
                Ok(Ok(ch)) => {
                    tracing::warn!(
                        "✅ Channel {} ({}) initialized in {:?}",
                        descriptor.display_name,
                        descriptor.slug,
                        start.elapsed()
                    );
                    ch
                }
                Ok(Err(e)) => {
                    tracing::error!(
                        "⚠️ Failed to initialize channel {} ({}): {}",
                        descriptor.display_name,
                        descriptor.slug,
                        e
                    );
                    continue;
                }
                Err(_) => {
                    tracing::error!(
                        "⚠️ Timeout initializing channel {} ({}) after 20s, skipping",
                        descriptor.display_name,
                        descriptor.slug
                    );
                    continue;
                }
            };
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

    pub async fn prefetch_until_horizon(&self, channel_id: u8) -> Result<()> {
        let channel = self
            .get(channel_id)
            .ok_or_else(|| anyhow!("Unknown channel id {}", channel_id))?;
        channel.prefetch_until_horizon().await
    }
}

pub fn register_global_channel_manager(manager: Arc<ParadiseChannelManager>) {
    let _ = GLOBAL_CHANNEL_MANAGER.set(Arc::downgrade(&manager));
}

pub fn get_global_channel_manager() -> Option<Arc<ParadiseChannelManager>> {
    GLOBAL_CHANNEL_MANAGER.get().and_then(|weak| weak.upgrade())
}

impl ParadiseStreamChannel {
    pub async fn prefetch_until_horizon(&self) -> Result<()> {
        self.state.prefetch_until_horizon().await
    }
}
