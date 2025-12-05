use std::collections::HashMap;
use std::io;
use std::sync::{Arc, Mutex, RwLock};
use std::thread;
use std::time::Duration;

use anyhow::anyhow;
use crossbeam_channel::Receiver;
use pmoupnp::ssdp::SsdpClient;
use tracing::{debug, error, info, warn};

use crate::MusicRenderer;
use crate::capabilities::{
    PlaybackPosition, PlaybackPositionInfo, PlaybackState, PlaybackStatus, TransportControl,
    VolumeControl,
};
use crate::discovery::DiscoveryManager;
use crate::events::{MediaServerEventBus, RendererEventBus};
use crate::media_server::{
    MediaBrowser, MediaEntry, MediaResource, MediaServerInfo, MusicServer, ServerId,
};
use crate::media_server_events::spawn_media_server_event_runtime;
use crate::model::TrackMetadata;
use crate::model::{MediaServerEvent, RendererEvent, RendererId, RendererProtocol};
use crate::music_renderer::op_not_supported;
use crate::playback_queue::{PlaybackItem, PlaybackQueue};
use crate::provider::HttpXmlDescriptionProvider;
use crate::registry::{DeviceRegistry, DeviceRegistryRead, DeviceUpdate};
use crate::upnp_renderer::UpnpRenderer;

/// Optional attachment between a renderer playback queue and a server-side
/// DIDL-Lite playlist container.
///
/// When a queue is bound to a playlist, the control point will automatically
/// refresh it whenever the server notifies us of changes to that container.
/// User-driven mutations (clear, enqueue, etc.) break the binding automatically.
#[derive(Clone, Debug)]
pub struct PlaylistBinding {
    /// MediaServer that owns the playlist container.
    pub server_id: ServerId,
    /// DIDL-Lite object id of the playlist container.
    pub container_id: String,
    /// True once at least one ContainerUpdateIDs notification has been seen.
    pub(crate) has_seen_update: bool,
    /// Flag used internally to signal that the queue should be refreshed
    /// from the server container.
    pub(crate) pending_refresh: bool,
    /// Whether the next refresh should auto-start playback if the renderer is idle.
    pub(crate) auto_play_on_refresh: bool,
}

/// Control point minimal :
/// - lance un SsdpClient dans un thread,
/// - passe les SsdpEvent au DiscoveryManager,
/// - applique les DeviceUpdate dans le DeviceRegistry.
pub struct ControlPoint {
    registry: Arc<RwLock<DeviceRegistry>>,
    event_bus: RendererEventBus,
    media_event_bus: MediaServerEventBus,
    runtime: Arc<RuntimeState>,
    /// Optional attachment between a renderer playback queue and a
    /// server-side DIDL-Lite playlist container.
    ///
    /// Key   : RendererId
    /// Value : PlaylistBinding
    playlist_bindings: Arc<Mutex<HashMap<RendererId, PlaylistBinding>>>,
}

impl ControlPoint {
    /// Crée un ControlPoint et lance le thread de découverte SSDP.
    ///
    /// `timeout_secs` : timeout HTTP pour la récupération des descriptions UPnP.
    pub fn spawn(timeout_secs: u64) -> io::Result<Self> {
        let registry = Arc::new(RwLock::new(DeviceRegistry::new()));
        let event_bus = RendererEventBus::new();
        let media_event_bus = MediaServerEventBus::new();
        let runtime = Arc::new(RuntimeState::new());
        let playlist_bindings = Arc::new(Mutex::new(HashMap::new()));

        // SsdpClient
        let client = SsdpClient::new()?; // pmoupnp::ssdp::SsdpClient

        // Arc utilisé dans le thread
        let registry_for_thread = Arc::clone(&registry);

        // Thread de découverte
        thread::spawn(move || {
            // Provider HTTP+XML et DiscoveryManager VIVENT dans le thread
            let provider = HttpXmlDescriptionProvider::new(timeout_secs);
            let mut discovery = DiscoveryManager::new(provider);

            // ACTIVE DISCOVERY : envoyer quelques M-SEARCH au démarrage
            // pour forcer les devices à répondre rapidement.
            let search_targets = [
                "ssdp:all",
                "urn:schemas-upnp-org:device:MediaRenderer:1",
                "urn:av-openhome-org:device:MediaRenderer:1",
                "urn:schemas-upnp-org:device:MediaServer:1",
                "urn:schemas-wiimu-com:service:PlayQueue:1", // <-- AJOUTER
            ];

            for st in &search_targets {
                if let Err(e) = client.send_msearch(st, 3) {
                    eprintln!("Failed to send M-SEARCH for {}: {}", st, e);
                }
                std::thread::sleep(Duration::from_millis(200));
            }

            // La closure passée à run_event_loop capture discovery par mutable borrow
            // => FnMut, ce que SsdpClient::run_event_loop accepte.
            client.run_event_loop(move |event| {
                let updates: Vec<DeviceUpdate> = discovery.handle_ssdp_event(event);

                if updates.is_empty() {
                    return;
                }

                if let Ok(mut reg) = registry_for_thread.write() {
                    for update in updates {
                        reg.apply_update(update);
                    }
                }
            });
        });

        let runtime_cp = ControlPoint {
            registry: Arc::clone(&registry),
            event_bus: event_bus.clone(),
            media_event_bus: media_event_bus.clone(),
            runtime: Arc::clone(&runtime),
            playlist_bindings: Arc::clone(&playlist_bindings),
        };

        thread::spawn(move || {
            let mut tick: u32 = 0;
            loop {
                let infos = {
                    let reg = runtime_cp.registry.read().unwrap();
                    reg.list_renderers()
                };
                let renderers = infos
                    .into_iter()
                    .filter_map(|info| {
                        MusicRenderer::from_registry_info(info, &runtime_cp.registry)
                    })
                    .collect::<Vec<_>>();

                for renderer in renderers {
                    let info = renderer.info();

                    if !info.online {
                        continue;
                    }

                    match info.protocol {
                        RendererProtocol::UpnpAvOnly | RendererProtocol::Hybrid => {}
                        RendererProtocol::OpenHomeOnly => continue,
                    }

                    let renderer_id = info.id.clone();
                    let prev_snapshot = runtime_cp.runtime.snapshot_for(&renderer_id);
                    let mut new_snapshot = prev_snapshot.clone();
                    let prev_position = prev_snapshot.position.clone();

                    // Poll position every tick (1s) for smooth UI progress
                    if let Ok(position) = renderer.playback_position() {
                        let has_changed = match prev_snapshot.position.as_ref() {
                            Some(prev) => !playback_position_equal(prev, &position),
                            None => true,
                        };

                        if has_changed {
                            runtime_cp.emit_renderer_event(RendererEvent::PositionChanged {
                                id: renderer_id.clone(),
                                position: position.clone(),
                            });
                        }

                        // Extract and emit metadata changes
                        match extract_track_metadata(&position) {
                            Some(metadata) => {
                                let metadata_changed = match prev_snapshot.last_metadata.as_ref() {
                                    Some(prev) => prev != &metadata,
                                    None => true,
                                };

                                if metadata_changed {
                                    debug!(
                                        renderer = renderer_id.0.as_str(),
                                        title = metadata.title.as_deref(),
                                        artist = metadata.artist.as_deref(),
                                        "Emitting metadata changed event"
                                    );
                                    runtime_cp.emit_renderer_event(RendererEvent::MetadataChanged {
                                        id: renderer_id.clone(),
                                        metadata: metadata.clone(),
                                    });
                                    new_snapshot.last_metadata = Some(metadata);
                                }
                            }
                            None => {
                                debug!(
                                    renderer = renderer_id.0.as_str(),
                                    has_track_metadata = position.track_metadata.is_some(),
                                    "No metadata extracted from position info"
                                );
                            }
                        }

                        new_snapshot.position = Some(position);
                    }

                    // Poll state every tick to ensure responsive playback control
                    if let Ok(raw_state) = renderer.playback_state() {
                        let logical_state = compute_logical_playback_state(
                            &raw_state,
                            prev_position.as_ref(),
                            new_snapshot.position.as_ref(),
                        );

                        let has_changed = match prev_snapshot.state.as_ref() {
                            Some(prev) => !playback_state_equal(prev, &logical_state),
                            None => true,
                        };

                        // Emit event only for non-transient states to reduce noise
                        // and avoid overwhelming the renderer during track changes
                        if has_changed && !matches!(logical_state, PlaybackState::Transitioning) {
                            runtime_cp.emit_renderer_event(RendererEvent::StateChanged {
                                id: renderer_id.clone(),
                                state: logical_state.clone(),
                            });
                        }

                        new_snapshot.state = Some(logical_state);
                    }

                    // Poll volume and mute less frequently (every 3 seconds)
                    // to reduce SOAP overhead without impacting UI responsiveness
                    if tick % 3 == 0 {
                        if let Ok(volume) = renderer.volume() {
                            if prev_snapshot.last_volume != Some(volume) {
                                runtime_cp.emit_renderer_event(RendererEvent::VolumeChanged {
                                    id: renderer_id.clone(),
                                    volume,
                                });
                            }

                            new_snapshot.last_volume = Some(volume);
                        }

                        if let Ok(mute) = renderer.mute() {
                            if prev_snapshot.last_mute != Some(mute) {
                                runtime_cp.emit_renderer_event(RendererEvent::MuteChanged {
                                    id: renderer_id.clone(),
                                    mute,
                                });
                            }

                            new_snapshot.last_mute = Some(mute);
                        }
                    }

                    runtime_cp
                        .runtime
                        .update_snapshot(&renderer_id, new_snapshot);
                }

                tick = tick.wrapping_add(1);
                // Keep 1 second polling for smooth position updates
                thread::sleep(Duration::from_secs(1));
            }
        });

        spawn_media_server_event_runtime(
            Arc::clone(&registry),
            media_event_bus.clone(),
            timeout_secs,
        )?;

        // Worker thread to process MediaServerEvent and trigger queue refreshes
        // for renderers bound to updated playlist containers
        let registry_for_media_worker = Arc::clone(&registry);
        let runtime_for_media_worker = Arc::clone(&runtime);
        let bindings_for_media_worker = Arc::clone(&playlist_bindings);
        let event_bus_for_media_worker = event_bus.clone();
        let media_rx = media_event_bus.subscribe();

        thread::Builder::new()
            .name("cp-media-server-event-worker".into())
            .spawn(move || {
                loop {
                    let event = match media_rx.recv() {
                        Ok(e) => e,
                        Err(_) => {
                            warn!("MediaServerEvent channel closed, worker exiting");
                            break;
                        }
                    };

                    match event {
                        MediaServerEvent::GlobalUpdated {
                            server_id,
                            system_update_id,
                        } => {
                            info!(
                                server = server_id.0.as_str(),
                                system_update_id = system_update_id,
                                "MediaServer global update"
                            );
                        }
                        MediaServerEvent::ContainersUpdated {
                            server_id,
                            container_ids,
                        } => {
                            let renderers_to_refresh: Vec<RendererId> = {
                                let mut bindings = bindings_for_media_worker.lock().unwrap();
                                let mut to_refresh = Vec::new();

                                for (renderer_id, binding) in bindings.iter_mut() {
                                    if binding.server_id == server_id
                                        && container_ids.contains(&binding.container_id)
                                    {
                                        binding.pending_refresh = true;
                                        binding.has_seen_update = true;
                                        to_refresh.push(renderer_id.clone());
                                    }
                                }

                                to_refresh
                            };

                            for renderer_id in renderers_to_refresh {
                                debug!(
                                    renderer = renderer_id.0.as_str(),
                                    server = server_id.0.as_str(),
                                    "Triggering queue refresh for bound playlist"
                                );

                                if let Err(err) = refresh_attached_queue_for(
                                    &registry_for_media_worker,
                                    &runtime_for_media_worker,
                                    &bindings_for_media_worker,
                                    &renderer_id,
                                    &event_bus_for_media_worker,
                                    None,
                                ) {
                                    warn!(
                                        renderer = renderer_id.0.as_str(),
                                        server = server_id.0.as_str(),
                                        error = %err,
                                        "Failed to refresh queue from playlist container"
                                    );
                                }
                            }
                        }
                    }
                }
            })?;

        // Periodic refresh worker for bound playlists
        // Every 60 seconds, trigger a refresh for all renderers with active bindings
        let registry_for_periodic = Arc::clone(&registry);
        let runtime_for_periodic = Arc::clone(&runtime);
        let bindings_for_periodic = Arc::clone(&playlist_bindings);
        let event_bus_for_periodic = event_bus.clone();

        thread::Builder::new()
            .name("cp-playlist-periodic-refresh".into())
            .spawn(move || {
                loop {
                    // Sleep for 60 seconds between refresh cycles
                    thread::sleep(Duration::from_secs(60));

                    // Collect all renderers with active bindings and mark them for refresh
                    let renderers_to_refresh: Vec<RendererId> = {
                        let mut bindings = bindings_for_periodic.lock().unwrap();
                        let mut to_refresh = Vec::new();

                        for (renderer_id, binding) in bindings.iter_mut() {
                            binding.pending_refresh = true;
                            to_refresh.push(renderer_id.clone());
                        }

                        to_refresh
                    };

                    // Trigger refresh for each bound renderer (outside of lock)
                    for renderer_id in renderers_to_refresh {
                        debug!(
                            renderer = renderer_id.0.as_str(),
                            "Periodic refresh triggered for bound playlist"
                        );

                        if let Err(err) = refresh_attached_queue_for(
                            &registry_for_periodic,
                            &runtime_for_periodic,
                            &bindings_for_periodic,
                            &renderer_id,
                            &event_bus_for_periodic,
                            None,
                        ) {
                            warn!(
                                renderer = renderer_id.0.as_str(),
                                error = %err,
                                "Periodic refresh failed for bound playlist"
                            );
                        }
                    }
                }
            })?;

        Ok(Self {
            registry,
            event_bus,
            media_event_bus,
            runtime,
            playlist_bindings,
        })
    }

    /// Accès au DeviceRegistry partagé.
    pub fn registry(&self) -> Arc<RwLock<DeviceRegistry>> {
        Arc::clone(&self.registry)
    }

    /// Snapshot list of renderers currently known by the registry.
    pub fn list_upnp_renderers(&self) -> Vec<UpnpRenderer> {
        let infos = {
            let reg = self.registry.read().unwrap();
            reg.list_renderers()
        };

        infos
            .into_iter()
            .map(|info| UpnpRenderer::from_registry(info, &self.registry))
            .collect()
    }

    /// Return the first renderer in the registry, if any.
    pub fn default_upnp_renderer(&self) -> Option<UpnpRenderer> {
        let info = {
            let reg = self.registry.read().unwrap();
            reg.list_renderers().into_iter().next()
        }?;

        Some(UpnpRenderer::from_registry(info, &self.registry))
    }

    /// Lookup a renderer by id.
    pub fn upnp_renderer_by_id(&self, id: &RendererId) -> Option<UpnpRenderer> {
        let info = {
            let reg = self.registry.read().unwrap();
            reg.get_renderer(id)
        }?;

        Some(UpnpRenderer::from_registry(info, &self.registry))
    }

    /// Snapshot list of music renderers (protocol-agnostic view).
    ///
    /// For now, only UPnP AV / hybrid renderers are wrapped as
    /// [`MusicRenderer::Upnp`]. OpenHome-only devices will be
    /// ignored until an OpenHome backend is implemented.
    pub fn list_music_renderers(&self) -> Vec<MusicRenderer> {
        let infos = {
            let reg = self.registry.read().unwrap();
            reg.list_renderers()
        };

        infos
            .into_iter()
            .filter_map(|info| MusicRenderer::from_registry_info(info, &self.registry))
            .collect()
    }

    /// Return the first music renderer in the registry, if any.
    pub fn default_music_renderer(&self) -> Option<MusicRenderer> {
        let infos = {
            let reg = self.registry.read().unwrap();
            reg.list_renderers()
        };

        infos
            .into_iter()
            .find_map(|info| MusicRenderer::from_registry_info(info, &self.registry))
    }

    /// Lookup a music renderer by id.
    pub fn music_renderer_by_id(&self, id: &RendererId) -> Option<MusicRenderer> {
        let info = {
            let reg = self.registry.read().unwrap();
            reg.get_renderer(id)
        }?;

        MusicRenderer::from_registry_info(info, &self.registry)
    }

    /// Snapshot list of media servers currently known by the registry.
    pub fn list_media_servers(&self) -> Vec<MediaServerInfo> {
        let reg = self.registry.read().unwrap();
        reg.list_servers()
    }

    /// Lookup a media server by id.
    pub fn media_server(&self, id: &ServerId) -> Option<MediaServerInfo> {
        let reg = self.registry.read().unwrap();
        reg.get_server(id)
    }

    pub fn clear_queue(&self, renderer_id: &RendererId) -> anyhow::Result<()> {
        if !self.runtime.has_entry(renderer_id) {
            let err = Self::runtime_entry_missing(renderer_id);
            warn!(
                renderer = renderer_id.0.as_str(),
                "Cannot clear queue: renderer not registered in runtime"
            );
            return Err(err);
        }

        // User-driven mutation: detach any playlist binding
        self.detach_binding_on_user_mutation(renderer_id, "clear_queue");

        let removed = self
            .runtime
            .with_queue_mut(renderer_id, |queue| {
                let removed = queue.upcoming_len();
                queue.clear();
                removed
            })
            .ok_or_else(|| Self::runtime_entry_missing(renderer_id))?;

        debug!(
            renderer = renderer_id.0.as_str(),
            items_removed = removed,
            queue_len = 0,
            "Cleared playback queue"
        );

        // Emit QueueUpdated event
        self.emit_renderer_event(RendererEvent::QueueUpdated {
            id: renderer_id.clone(),
            queue_length: 0,
        });

        Ok(())
    }

    pub fn enqueue_items(
        &self,
        renderer_id: &RendererId,
        items: Vec<PlaybackItem>,
    ) -> anyhow::Result<()> {
        if !self.runtime.has_entry(renderer_id) {
            let err = Self::runtime_entry_missing(renderer_id);
            warn!(
                renderer = renderer_id.0.as_str(),
                "Cannot enqueue items: renderer not registered in runtime"
            );
            return Err(err);
        }

        // User-driven mutation: detach any playlist binding
        self.detach_binding_on_user_mutation(renderer_id, "enqueue_items");

        let item_count = items.len();
        let new_len = self
            .runtime
            .with_queue_mut(renderer_id, |queue| {
                queue.enqueue_many(items);
                queue.upcoming_len()
            })
            .ok_or_else(|| Self::runtime_entry_missing(renderer_id))?;

        debug!(
            renderer = renderer_id.0.as_str(),
            added = item_count,
            queue_len = new_len,
            "Enqueued playback items"
        );

        // Emit QueueUpdated event
        self.emit_renderer_event(RendererEvent::QueueUpdated {
            id: renderer_id.clone(),
            queue_length: new_len,
        });

        Ok(())
    }

    pub fn get_queue_snapshot(
        &self,
        renderer_id: &RendererId,
    ) -> anyhow::Result<Vec<PlaybackItem>> {
        if !self.runtime.has_entry(renderer_id) {
            let err = Self::runtime_entry_missing(renderer_id);
            warn!(
                renderer = renderer_id.0.as_str(),
                "Cannot snapshot queue: renderer not registered in runtime"
            );
            return Err(err);
        }

        self.runtime
            .queue_snapshot(renderer_id)
            .ok_or_else(|| Self::runtime_entry_missing(renderer_id))
    }

    pub fn get_full_queue_snapshot(
        &self,
        renderer_id: &RendererId,
    ) -> anyhow::Result<(Vec<PlaybackItem>, Option<usize>)> {
        if !self.runtime.has_entry(renderer_id) {
            // Renderer not yet initialized in runtime (just discovered via SSDP)
            // This is normal and will be fixed on first polling cycle
            debug!(
                renderer = renderer_id.0.as_str(),
                "Renderer not yet initialized in runtime, returning empty queue"
            );
            return Err(Self::runtime_entry_missing(renderer_id));
        }

        self.runtime
            .queue_full_snapshot(renderer_id)
            .ok_or_else(|| Self::runtime_entry_missing(renderer_id))
    }

    /// Play the current item from the queue without advancing the index.
    ///
    /// This is useful after a Stop operation to resume playback from the current
    /// position rather than skipping to the next track.
    pub fn play_current_from_queue(&self, renderer_id: &RendererId) -> anyhow::Result<()> {
        if !self.runtime.has_entry(renderer_id) {
            let err = Self::runtime_entry_missing(renderer_id);
            warn!(
                renderer = renderer_id.0.as_str(),
                "Cannot play current: renderer not registered in runtime"
            );
            return Err(err);
        }

        let Some((item, remaining)) = self.runtime.peek_current(renderer_id) else {
            debug!(
                renderer = renderer_id.0.as_str(),
                "play_current_from_queue: queue is empty or no current item"
            );
            self.runtime
                .set_playback_source(renderer_id, PlaybackSource::None);
            return Ok(());
        };

        debug!(
            renderer = renderer_id.0.as_str(),
            queue_len = remaining + 1,
            uri = item.uri.as_str(),
            "Playing current playback item from queue"
        );

        let renderer = self.music_renderer_by_id(renderer_id).ok_or_else(|| {
            warn!(
                renderer = renderer_id.0.as_str(),
                "Renderer disappeared before queue playback could start"
            );
            anyhow!("Renderer {} not found", renderer_id.0)
        })?;

        if matches!(renderer.info().protocol, RendererProtocol::OpenHomeOnly) {
            self.runtime
                .set_playback_source(renderer_id, PlaybackSource::None);
            return Err(op_not_supported(
                "play_current_from_queue",
                "OpenHomeOnly renderer",
            ));
        }

        let playback = (|| -> anyhow::Result<()> {
            let didl_metadata = item.to_didl_metadata();
            renderer.play_uri(&item.uri, &didl_metadata)?;
            Ok(())
        })();

        match playback {
            Ok(()) => {
                info!(
                    renderer = renderer_id.0.as_str(),
                    uri = item.uri.as_str(),
                    "Queue playback started (current item)"
                );
                self.runtime
                    .set_playback_source(renderer_id, PlaybackSource::FromQueue);
                Ok(())
            }
            Err(e) => {
                error!(
                    renderer = renderer_id.0.as_str(),
                    error = %e,
                    "Failed to play current item from queue"
                );
                self.runtime
                    .set_playback_source(renderer_id, PlaybackSource::None);
                Err(e)
            }
        }
    }

    pub fn play_next_from_queue(&self, renderer_id: &RendererId) -> anyhow::Result<()> {
        if !self.runtime.has_entry(renderer_id) {
            let err = Self::runtime_entry_missing(renderer_id);
            warn!(
                renderer = renderer_id.0.as_str(),
                "Cannot advance queue: renderer not registered in runtime"
            );
            return Err(err);
        }

        let Some((item, remaining_after)) = self.runtime.dequeue_next(renderer_id) else {
            debug!(
                renderer = renderer_id.0.as_str(),
                "play_next_from_queue: queue is empty"
            );
            self.runtime
                .set_playback_source(renderer_id, PlaybackSource::None);
            return Ok(());
        };

        let queue_before = remaining_after + 1;
        debug!(
            renderer = renderer_id.0.as_str(),
            queue_before,
            queue_after = remaining_after,
            uri = item.uri.as_str(),
            "Dequeued next playback item"
        );

        let renderer = self.music_renderer_by_id(renderer_id).ok_or_else(|| {
            warn!(
                renderer = renderer_id.0.as_str(),
                "Renderer disappeared before queue playback could start"
            );
            anyhow!("Renderer {} not found", renderer_id.0)
        })?;

        if matches!(renderer.info().protocol, RendererProtocol::OpenHomeOnly) {
            self.runtime
                .with_queue_mut(renderer_id, |queue| queue.enqueue_front(item))
                .ok_or_else(|| Self::runtime_entry_missing(renderer_id))?;
            self.runtime
                .set_playback_source(renderer_id, PlaybackSource::None);
            return Err(op_not_supported(
                "play_next_from_queue",
                "OpenHomeOnly renderer",
            ));
        }

        let playback = (|| -> anyhow::Result<()> {
            let didl_metadata = item.to_didl_metadata();
            renderer.play_uri(&item.uri, &didl_metadata)?;
            Ok(())
        })();

        if let Err(err) = playback {
            error!(
                renderer = renderer_id.0.as_str(),
                error = %err,
                "Failed to start playback for queued item"
            );
            if self
                .runtime
                .with_queue_mut(renderer_id, |queue| queue.enqueue_front(item))
                .is_none()
            {
                warn!(
                    renderer = renderer_id.0.as_str(),
                    "Failed to requeue item after playback error"
                );
            }
            self.runtime
                .set_playback_source(renderer_id, PlaybackSource::None);
            return Err(err);
        }

        self.runtime
            .set_playback_source(renderer_id, PlaybackSource::FromQueue);
        debug!(
            renderer = renderer_id.0.as_str(),
            queue_len = remaining_after,
            "Started playback from queue"
        );

        if let Some(snapshot) = self.runtime.queue_snapshot(renderer_id) {
            if let Some(next_item) = snapshot.first() {
                if let Some(upnp) = renderer.as_upnp() {
                    let known_supported = upnp.supports_set_next();
                    if known_supported || upnp.has_avtransport() {
                        let next_didl_metadata = next_item.to_didl_metadata();
                        match upnp.set_next_uri(&next_item.uri, &next_didl_metadata) {
                            Ok(_) => debug!(
                                renderer = renderer_id.0.as_str(),
                                "Prefetched next track via SetNextAVTransportURI"
                            ),
                            Err(err) => debug!(
                                renderer = renderer_id.0.as_str(),
                                error = %err,
                                "SetNextAVTransportURI failed for next queue item; continuing without prefetch"
                            ),
                        }
                    }
                }
            }
        }

        // Emit QueueUpdated event
        self.emit_renderer_event(RendererEvent::QueueUpdated {
            id: renderer_id.clone(),
            queue_length: remaining_after,
        });

        Ok(())
    }

    fn start_queue_playback_if_idle(&self, renderer_id: &RendererId) -> anyhow::Result<()> {
        let snapshot = self.runtime.snapshot_for(renderer_id);
        let renderer_playing = matches!(snapshot.state, Some(PlaybackState::Playing));
        if renderer_playing || self.runtime.is_playing_from_queue(renderer_id) {
            return Ok(());
        }

        let has_items = self
            .runtime
            .queue_snapshot(renderer_id)
            .map(|items| !items.is_empty())
            .unwrap_or(false);
        if !has_items {
            debug!(
                renderer = renderer_id.0.as_str(),
                "start_queue_playback_if_idle: queue is empty"
            );
            return Ok(());
        }

        self.play_next_from_queue(renderer_id)
    }

    /// Stop playback in response to user action (e.g., Stop button in UI).
    ///
    /// This method marks the stop as user-requested to prevent automatic
    /// advancement to the next track in the queue when the STOPPED event
    /// is received from the renderer.
    pub fn user_stop(&self, renderer_id: &RendererId) -> anyhow::Result<()> {
        // Mark that user requested stop before actually stopping
        self.runtime.mark_user_stop_requested(renderer_id);

        // Get renderer and call stop
        let renderer = self.music_renderer_by_id(renderer_id).ok_or_else(|| {
            warn!(
                renderer = renderer_id.0.as_str(),
                "Cannot stop: renderer not found in registry"
            );
            anyhow!("Renderer {} not found", renderer_id.0)
        })?;

        debug!(
            renderer = renderer_id.0.as_str(),
            "User-requested stop"
        );

        renderer.stop()
    }

    /// Subscribe to renderer events emitted by the control point runtime.
    ///
    /// Each subscriber receives all future events independently.
    pub fn subscribe_events(&self) -> Receiver<RendererEvent> {
        self.event_bus.subscribe()
    }

    /// Access the media server event bus for ContentDirectory notifications.
    pub fn media_server_events(&self) -> MediaServerEventBus {
        self.media_event_bus.clone()
    }

    /// Subscribe directly to media server events emitted by the control point.
    pub fn subscribe_media_server_events(&self) -> Receiver<MediaServerEvent> {
        self.media_event_bus.subscribe()
    }

    /// Attach a renderer's playback queue to a server-side playlist container.
    ///
    /// When attached, the queue will be automatically refreshed from the
    /// container whenever the server notifies us of changes via ContentDirectory
    /// events. The binding is broken if the user explicitly mutates the queue
    /// through methods like `clear_queue` or `enqueue_items`.
    /// Attach a renderer's queue to a playlist container.
    ///
    /// The queue will be automatically refreshed when the playlist changes on the server.
    pub fn attach_queue_to_playlist(
        &self,
        renderer_id: &RendererId,
        server_id: ServerId,
        container_id: String,
    ) {
        self.attach_queue_to_playlist_internal(renderer_id, server_id, container_id, false);
    }

    /// Attach a renderer's queue to a playlist container without doing the initial refresh.
    ///
    /// This is useful when the queue has already been manually populated and we just want
    /// to track future changes to the playlist.
    pub fn attach_queue_to_playlist_without_refresh(
        &self,
        renderer_id: &RendererId,
        server_id: ServerId,
        container_id: String,
    ) {
        self.attach_queue_to_playlist_internal(renderer_id, server_id, container_id, true);
    }

    /// Internal implementation with optional skip of initial refresh
    fn attach_queue_to_playlist_internal(
        &self,
        renderer_id: &RendererId,
        server_id: ServerId,
        container_id: String,
        skip_initial_refresh: bool,
    ) {
        let binding = PlaylistBinding {
            server_id: server_id.clone(),
            container_id: container_id.clone(),
            has_seen_update: false,
            pending_refresh: !skip_initial_refresh,
            auto_play_on_refresh: !skip_initial_refresh,
        };

        {
            let mut bindings = self.playlist_bindings.lock().unwrap();
            bindings.insert(renderer_id.clone(), binding.clone());
            info!(
                renderer = renderer_id.0.as_str(),
                server = server_id.0.as_str(),
                container = container_id.as_str(),
                skip_refresh = skip_initial_refresh,
                "Queue attached to playlist container"
            );
        } // Drop bindings lock here before calling refresh_attached_queue_for

        // Emit binding changed event to notify frontend
        self.emit_renderer_event(RendererEvent::BindingChanged {
            id: renderer_id.clone(),
            binding: Some(binding),
        });

        if !skip_initial_refresh {
            let mut auto_start_cb = |rid: &RendererId| self.start_queue_playback_if_idle(rid);
            if let Err(err) = refresh_attached_queue_for(
                &self.registry,
                &self.runtime,
                &self.playlist_bindings,
                renderer_id,
                &self.event_bus,
                Some(&mut auto_start_cb),
            ) {
                warn!(
                    renderer = renderer_id.0.as_str(),
                    server = server_id.0.as_str(),
                    container = container_id.as_str(),
                    error = %err,
                    "Initial playlist refresh after attachment failed"
                );
            }
        }
    }

    /// Detach a renderer's queue from its associated playlist container.
    ///
    /// After calling this, the queue will no longer be automatically refreshed
    /// from the server. If no binding existed, this is a no-op.
    pub fn detach_queue_playlist(&self, renderer_id: &RendererId) {
        let removed = {
            let mut bindings = self.playlist_bindings.lock().unwrap();
            bindings.remove(renderer_id)
        };

        if let Some(binding) = removed {
            info!(
                renderer = renderer_id.0.as_str(),
                server = binding.server_id.0.as_str(),
                container = binding.container_id.as_str(),
                "Queue detached from playlist container"
            );
            // Emit binding changed event to notify frontend
            self.emit_renderer_event(RendererEvent::BindingChanged {
                id: renderer_id.clone(),
                binding: None,
            });
        } else {
            debug!(
                renderer = renderer_id.0.as_str(),
                "detach_queue_playlist: no binding to remove"
            );
        }
    }

    /// Query the current playlist binding for a renderer's queue, if any.
    ///
    /// Returns `(server_id, container_id, has_seen_update)` if the queue is
    /// bound to a server playlist container, or `None` otherwise.
    pub fn current_queue_playlist_binding(
        &self,
        renderer_id: &RendererId,
    ) -> Option<(ServerId, String, bool)> {
        let bindings = self.playlist_bindings.lock().unwrap();
        bindings.get(renderer_id).map(|binding| {
            (
                binding.server_id.clone(),
                binding.container_id.clone(),
                binding.has_seen_update,
            )
        })
    }

    /// Internal helper to detach the playlist binding on user-driven mutations.
    ///
    /// This is called by public queue mutation methods (clear, enqueue, etc.)
    /// to ensure that explicit user actions break the automatic refresh binding.
    fn detach_binding_on_user_mutation(&self, renderer_id: &RendererId, reason: &str) {
        let mut bindings = self.playlist_bindings.lock().unwrap();
        if let Some(binding) = bindings.remove(renderer_id) {
            info!(
                renderer = renderer_id.0.as_str(),
                server = binding.server_id.0.as_str(),
                container = binding.container_id.as_str(),
                reason = reason,
                "Playlist binding auto-detached due to user mutation"
            );
        }
    }

    pub(crate) fn emit_renderer_event(&self, event: RendererEvent) {
        self.handle_renderer_event(&event);
        self.event_bus.broadcast(event);
    }

    fn handle_renderer_event(&self, event: &RendererEvent) {
        if let RendererEvent::StateChanged { id, state } = event {
            match state {
                PlaybackState::Stopped => {
                    // Check if user requested stop (via Stop button in UI)
                    if self.runtime.check_and_clear_user_stop_requested(id) {
                        debug!(
                            renderer = id.0.as_str(),
                            "Renderer stopped by user request; not auto-advancing"
                        );
                        self.runtime.set_playback_source(id, PlaybackSource::None);
                    } else if self.runtime.is_playing_from_queue(id) {
                        debug!(
                            renderer = id.0.as_str(),
                            "Renderer stopped after queue-driven playback; advancing"
                        );
                        if let Err(err) = self.play_next_from_queue(id) {
                            error!(
                                renderer = id.0.as_str(),
                                error = %err,
                                "Auto-advance failed; clearing queue playback state"
                            );
                            self.runtime.set_playback_source(id, PlaybackSource::None);
                        }
                    } else {
                        self.runtime.set_playback_source(id, PlaybackSource::None);
                    }
                }
                PlaybackState::Playing => {
                    self.runtime.mark_external_if_idle(id);
                }
                _ => {}
            }
        }
    }

    fn runtime_entry_missing(renderer_id: &RendererId) -> anyhow::Error {
        anyhow!(
            "Renderer {} not registered in control point runtime",
            renderer_id.0
        )
    }
}

#[derive(Clone, Default)]
struct RendererRuntimeSnapshot {
    state: Option<PlaybackState>,
    position: Option<PlaybackPositionInfo>,
    last_volume: Option<u16>,
    last_mute: Option<bool>,
    last_metadata: Option<TrackMetadata>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum PlaybackSource {
    #[default]
    None,
    FromQueue,
    External,
}

#[derive(Default)]
struct RendererRuntimeEntry {
    snapshot: RendererRuntimeSnapshot,
    queue: PlaybackQueue,
    playback_source: PlaybackSource,
    user_stop_requested: bool,
}

struct RuntimeState {
    entries: Mutex<HashMap<RendererId, RendererRuntimeEntry>>,
}

impl RuntimeState {
    fn new() -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
        }
    }

    fn snapshot_for(&self, id: &RendererId) -> RendererRuntimeSnapshot {
        let entries = self.entries.lock().unwrap();
        entries
            .get(id)
            .map(|entry| entry.snapshot.clone())
            .unwrap_or_default()
    }

    fn update_snapshot(&self, id: &RendererId, snapshot: RendererRuntimeSnapshot) {
        self.with_entry(id, |entry| {
            entry.snapshot = snapshot;
        });
    }

    fn has_entry(&self, id: &RendererId) -> bool {
        let entries = self.entries.lock().unwrap();
        entries.contains_key(id)
    }

    fn with_queue_mut<F, R>(&self, id: &RendererId, f: F) -> Option<R>
    where
        F: FnOnce(&mut PlaybackQueue) -> R,
    {
        let mut entries = self.entries.lock().unwrap();
        entries.get_mut(id).map(|entry| f(&mut entry.queue))
    }

    fn queue_snapshot(&self, id: &RendererId) -> Option<Vec<PlaybackItem>> {
        let entries = self.entries.lock().unwrap();
        entries.get(id).map(|entry| entry.queue.snapshot())
    }

    fn queue_full_snapshot(&self, id: &RendererId) -> Option<(Vec<PlaybackItem>, Option<usize>)> {
        let entries = self.entries.lock().unwrap();
        entries.get(id).map(|entry| entry.queue.full_snapshot())
    }

    fn dequeue_next(&self, id: &RendererId) -> Option<(PlaybackItem, usize)> {
        let mut entries = self.entries.lock().unwrap();
        let entry = entries.get_mut(id)?;
        let item = entry.queue.dequeue()?;
        let remaining = entry.queue.upcoming_len();
        Some((item, remaining))
    }

    fn peek_current(&self, id: &RendererId) -> Option<(PlaybackItem, usize)> {
        let entries = self.entries.lock().unwrap();
        let entry = entries.get(id)?;
        let item = entry.queue.peek()?.clone();
        let remaining = entry.queue.upcoming_len();
        Some((item, remaining))
    }

    fn set_playback_source(&self, id: &RendererId, source: PlaybackSource) {
        let mut entries = self.entries.lock().unwrap();
        if let Some(entry) = entries.get_mut(id) {
            entry.playback_source = source;
        }
    }

    fn playback_source(&self, id: &RendererId) -> PlaybackSource {
        let entries = self.entries.lock().unwrap();
        entries
            .get(id)
            .map(|entry| entry.playback_source)
            .unwrap_or(PlaybackSource::None)
    }

    fn is_playing_from_queue(&self, id: &RendererId) -> bool {
        matches!(self.playback_source(id), PlaybackSource::FromQueue)
    }

    fn mark_external_if_idle(&self, id: &RendererId) {
        let mut entries = self.entries.lock().unwrap();
        if let Some(entry) = entries.get_mut(id) {
            if matches!(entry.playback_source, PlaybackSource::None) {
                entry.playback_source = PlaybackSource::External;
            }
        }
    }

    fn with_entry<F, R>(&self, id: &RendererId, f: F) -> R
    where
        F: FnOnce(&mut RendererRuntimeEntry) -> R,
    {
        let mut entries = self.entries.lock().unwrap();
        let entry = entries
            .entry(id.clone())
            .or_insert_with(RendererRuntimeEntry::default);
        f(entry)
    }

    fn mark_user_stop_requested(&self, id: &RendererId) {
        let mut entries = self.entries.lock().unwrap();
        if let Some(entry) = entries.get_mut(id) {
            entry.user_stop_requested = true;
        }
    }

    fn check_and_clear_user_stop_requested(&self, id: &RendererId) -> bool {
        let mut entries = self.entries.lock().unwrap();
        if let Some(entry) = entries.get_mut(id) {
            let was_requested = entry.user_stop_requested;
            entry.user_stop_requested = false;
            was_requested
        } else {
            false
        }
    }
}

/// Internal helper to refresh a renderer's playback queue from its bound
/// playlist container.
///
/// This function is called automatically when a ContentDirectory event indicates
/// that the bound container has been updated. It attempts to preserve the
/// currently playing item when possible.
fn refresh_attached_queue_for(
    registry: &Arc<RwLock<DeviceRegistry>>,
    runtime: &Arc<RuntimeState>,
    bindings: &Arc<Mutex<HashMap<RendererId, PlaylistBinding>>>,
    renderer_id: &RendererId,
    event_bus: &RendererEventBus,
    mut after_refresh: Option<&mut dyn FnMut(&RendererId) -> anyhow::Result<()>>,
) -> anyhow::Result<()> {
    // Step 1: Check binding and mark refresh as in-progress
    let (server_id, container_id, auto_play) = {
        let mut bindings_lock = bindings.lock().unwrap();
        let binding = match bindings_lock.get_mut(renderer_id) {
            Some(b) => b,
            None => {
                debug!(
                    renderer = renderer_id.0.as_str(),
                    "refresh_attached_queue_for: no binding present"
                );
                return Ok(());
            }
        };

        if !binding.pending_refresh {
            debug!(
                renderer = renderer_id.0.as_str(),
                "refresh_attached_queue_for: pending_refresh is false, nothing to do"
            );
            return Ok(());
        }

        // Mark as processed
        binding.pending_refresh = false;
        let auto_play = binding.auto_play_on_refresh;
        binding.auto_play_on_refresh = false;
        (
            binding.server_id.clone(),
            binding.container_id.clone(),
            auto_play,
        )
    };

    // Step 2: Fetch MediaServerInfo from registry
    let server_info = {
        let reg = registry.read().unwrap();
        reg.get_server(&server_id)
    };

    let server_info = match server_info {
        Some(info) => info,
        None => {
            warn!(
                renderer = renderer_id.0.as_str(),
                server = server_id.0.as_str(),
                "refresh_attached_queue_for: server not found in registry"
            );
            return Ok(());
        }
    };

    if !server_info.online {
        debug!(
            renderer = renderer_id.0.as_str(),
            server = server_id.0.as_str(),
            "refresh_attached_queue_for: server offline, skipping refresh"
        );
        return Ok(());
    }

    if !server_info.has_content_directory {
        debug!(
            renderer = renderer_id.0.as_str(),
            server = server_id.0.as_str(),
            "refresh_attached_queue_for: server has no ContentDirectory"
        );
        return Ok(());
    }

    // Step 3: Create MusicServer and browse container
    let music_server = MusicServer::from_info(&server_info, Duration::from_secs(5))?;

    let entries = match music_server.browse_children(&container_id, 0, 64) {
        Ok(e) => e,
        Err(err) => {
            warn!(
                renderer = renderer_id.0.as_str(),
                server = server_id.0.as_str(),
                container = container_id.as_str(),
                error = %err,
                "Failed to browse playlist container for refresh"
            );
            return Err(err);
        }
    };

    // Step 4: Convert MediaEntry to PlaybackItem
    let new_items: Vec<PlaybackItem> = entries
        .iter()
        .filter_map(|entry| playback_item_from_entry(&music_server, entry))
        .collect();

    if new_items.is_empty() {
        debug!(
            renderer = renderer_id.0.as_str(),
            server = server_id.0.as_str(),
            container = container_id.as_str(),
            "Refreshed playlist is empty, clearing queue"
        );
        runtime.with_queue_mut(renderer_id, |queue| queue.clear());

        // Emit QueueUpdated event
        event_bus.broadcast(RendererEvent::QueueUpdated {
            id: renderer_id.clone(),
            queue_length: 0,
        });

        return Ok(());
    }

    // Step 5: Intelligent refresh: try to keep current item if it's still in the new list
    // Get the full queue snapshot to access the item currently being played
    let (full_queue, current_idx) = runtime
        .queue_full_snapshot(renderer_id)
        .unwrap_or((vec![], None));

    // Get the item currently being played (at current_index), not the next one in queue
    let current_item = current_idx.and_then(|idx| full_queue.get(idx).cloned());

    let item_found_at = current_item.as_ref().and_then(|current| {
        new_items.iter().position(|new_item| {
            // Match by object_id if both have it
            if let (Some(current_obj), Some(new_obj)) = (&current.object_id, &new_item.object_id) {
                return current_obj == new_obj;
            }
            // Fallback: match by URI
            current.uri == new_item.uri
        })
    });

    let final_queue_len = runtime
        .with_queue_mut(renderer_id, |queue| {
            queue.clear();

            if let Some(idx) = item_found_at {
                // Current item found: load the ENTIRE new playlist and position at that item
                // This preserves items before the current track (as "already played")
                for item in new_items.iter() {
                    queue.enqueue(item.clone());
                }
                queue.set_current_index(Some(idx));
                info!(
                    renderer = renderer_id.0.as_str(),
                    server = server_id.0.as_str(),
                    container = container_id.as_str(),
                    total_items = new_items.len(),
                    current_index = idx,
                    upcoming = new_items.len().saturating_sub(idx + 1),
                    current_preserved = true,
                    "Refreshed queue from playlist container"
                );
                new_items.len()
            } else if let Some(ref current) = current_item {
                // Current item NOT found: insert it at the beginning, then add new items
                // This preserves the currently playing track and prevents it from being lost
                queue.enqueue(current.clone());
                for item in new_items.iter() {
                    queue.enqueue(item.clone());
                }
                queue.set_current_index(Some(0));
                info!(
                    renderer = renderer_id.0.as_str(),
                    server = server_id.0.as_str(),
                    container = container_id.as_str(),
                    total_items = new_items.len() + 1,
                    current_index = 0,
                    upcoming = new_items.len(),
                    current_preserved = true,
                    current_reinserted = true,
                    "Refreshed queue from playlist container (current item reinserted at start)"
                );
                new_items.len() + 1
            } else {
                // No current item: replace with full new list
                for item in new_items.iter() {
                    queue.enqueue(item.clone());
                }
                queue.set_current_index(None);
                info!(
                    renderer = renderer_id.0.as_str(),
                    server = server_id.0.as_str(),
                    container = container_id.as_str(),
                    total_items = new_items.len(),
                    current_preserved = false,
                    "Refreshed queue from playlist container (no current item)"
                );
                new_items.len()
            }
        })
        .unwrap_or(0);

    // Emit QueueUpdated event
    event_bus.broadcast(RendererEvent::QueueUpdated {
        id: renderer_id.clone(),
        queue_length: final_queue_len,
    });

    if auto_play {
        if let Some(callback) = after_refresh.as_deref_mut() {
            if let Err(err) = callback(renderer_id) {
                warn!(
                    renderer = renderer_id.0.as_str(),
                    server = server_id.0.as_str(),
                    container = container_id.as_str(),
                    error = %err,
                    "Failed to auto-start playback after playlist refresh"
                );
            }
        }
    }

    Ok(())
}

/// Helper to detect if a MediaResource is audio content.
fn is_audio_resource(res: &MediaResource) -> bool {
    let lower = res.protocol_info.to_ascii_lowercase();
    if lower.contains("audio/") {
        return true;
    }
    // Check MIME type in protocolInfo (format: protocol:network:contentFormat:additionalInfo)
    lower
        .split(':')
        .nth(2)
        .map(|mime| mime.starts_with("audio/"))
        .unwrap_or(false)
}

/// Helper to convert a MediaEntry to a PlaybackItem.
fn playback_item_from_entry(server: &MusicServer, entry: &MediaEntry) -> Option<PlaybackItem> {
    // Ignore containers
    if entry.is_container {
        return None;
    }

    // Skip "live stream" entries (heuristic from example)
    if entry.title.to_ascii_lowercase().contains("live stream") {
        return None;
    }

    // Find an audio resource
    let resource = entry.resources.iter().find(|res| is_audio_resource(res))?;

    let mut item = PlaybackItem::new(resource.uri.clone());
    item.title = Some(entry.title.clone());
    item.server_id = Some(server.id().clone());
    item.object_id = Some(entry.id.clone());
    item.artist = entry.artist.clone();
    item.album = entry.album.clone();
    item.genre = entry.genre.clone();
    item.album_art_uri = entry.album_art_uri.clone();
    item.date = entry.date.clone();
    item.track_number = entry.track_number.clone();
    item.creator = entry.creator.clone();

    Some(item)
}

/// Parse "HH:MM:SS" style time strings to seconds.
///
/// Returns None for empty or sentinel values such as "NOT_IMPLEMENTED" or "-:--:--".
fn parse_hms_to_secs(s: &str) -> Option<u64> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }

    // Common sentinel values for "no information" in UPnP implementations.
    if s == "NOT_IMPLEMENTED" || s == "-:--:--" {
        return None;
    }

    let parts: Vec<_> = s.split(':').collect();
    if parts.len() != 3 {
        return None;
    }

    let hours: u64 = parts[0].parse().ok()?;
    let minutes: u64 = parts[1].parse().ok()?;
    let seconds: u64 = parts[2].parse().ok()?;

    Some(hours * 3600 + minutes * 60 + seconds)
}

fn parse_optional_hms_to_secs(value: &Option<String>) -> Option<u64> {
    value.as_ref().and_then(|s| parse_hms_to_secs(s))
}

/// Compute a logical playback state by combining the raw AVTransport state
/// with previous and current position information.
///
/// This is designed to compensate for buggy LinkPlay/Arylic devices that
/// report:
///   - STOPPED while the time actually advances,
///   - NO_MEDIA_PRESENT while track duration is known.
fn compute_logical_playback_state(
    raw: &PlaybackState,
    prev_position: Option<&PlaybackPositionInfo>,
    current_position: Option<&PlaybackPositionInfo>,
) -> PlaybackState {
    use PlaybackState::*;

    // Rule 1: Arylic / LinkPlay sometimes report STOPPED while the stream is
    // actually playing. If we detect that the relative time advances between
    // two polls, we treat this as Playing.
    if let Stopped = raw {
        if let (Some(prev), Some(curr)) = (prev_position, current_position) {
            if let (Some(prev_rel), Some(curr_rel)) = (
                parse_optional_hms_to_secs(&prev.rel_time),
                parse_optional_hms_to_secs(&curr.rel_time),
            ) {
                if curr_rel > prev_rel {
                    let delta = curr_rel - prev_rel;
                    // Our poll loop runs every 1s; accept small jitter in the delta.
                    if delta <= 5 {
                        return Playing;
                    }
                }
            }
        }
    }

    // Rule 2: Some devices report NO_MEDIA_PRESENT while exposing a non-zero
    // track duration. In practice this behaves like a stopped transport with
    // a loaded track.
    if let NoMedia = raw {
        let duration_secs = current_position
            .and_then(|p| parse_optional_hms_to_secs(&p.track_duration))
            .or_else(|| prev_position.and_then(|p| parse_optional_hms_to_secs(&p.track_duration)));

        if matches!(duration_secs, Some(d) if d > 0) {
            return Stopped;
        }
    }

    // Fallback: keep the raw (already normalized) state.
    raw.clone()
}

fn playback_state_equal(a: &PlaybackState, b: &PlaybackState) -> bool {
    match (a, b) {
        (PlaybackState::Unknown(lhs), PlaybackState::Unknown(rhs)) => lhs == rhs,
        _ => std::mem::discriminant(a) == std::mem::discriminant(b),
    }
}

fn playback_position_equal(a: &PlaybackPositionInfo, b: &PlaybackPositionInfo) -> bool {
    a.track == b.track
        && a.rel_time == b.rel_time
        && a.abs_time == b.abs_time
        && a.track_duration == b.track_duration
        && a.track_metadata == b.track_metadata
        && a.track_uri == b.track_uri
}

/// Extract TrackMetadata from DIDL-Lite XML in PlaybackPositionInfo.
fn extract_track_metadata(position: &PlaybackPositionInfo) -> Option<TrackMetadata> {
    let didl_xml = match position.track_metadata.as_ref() {
        Some(xml) => xml,
        None => {
            debug!("Position info has no track_metadata (DIDL-Lite XML)");
            return None;
        }
    };

    // Parse DIDL-Lite XML
    let didl = match pmodidl::parse_metadata::<pmodidl::DIDLLite>(didl_xml) {
        Ok(parsed) => parsed.data,
        Err(err) => {
            debug!(error = %err, "Failed to parse DIDL-Lite metadata from GetPositionInfo");
            return None;
        }
    };

    // Extract first item metadata
    let item = match didl.items.first() {
        Some(item) => item,
        None => {
            debug!("DIDL-Lite has no items");
            return None;
        }
    };

    Some(TrackMetadata {
        title: Some(item.title.clone()),
        artist: item.artist.clone(),
        album: item.album.clone(),
        genre: item.genre.clone(),
        album_art_uri: item.album_art.clone(),
        date: item.date.clone(),
        track_number: item.original_track_number.clone(),
        creator: item.creator.clone(),
    })
}
