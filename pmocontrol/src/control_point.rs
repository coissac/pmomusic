use std::io;
use std::sync::{Arc, Mutex, RwLock};
use std::thread;
use std::time::Duration;

use anyhow::anyhow;
use crossbeam_channel::Receiver;
use pmodidl::{DIDLLite, Item as DidlItem, Resource as DidlResource};
use pmoupnp::ssdp::SsdpClient;
use quick_xml::se::to_string as to_didl_string;
use tracing::{debug, error, info, warn};

use crate::discovery::manager::UDNRegistry;
use crate::errors::ControlPointError;
use crate::events::{MediaServerEventBus, RendererEventBus};
use crate::media_server::{MediaBrowser, MusicServer, playback_item_from_entry};
use crate::media_server_events::spawn_media_server_event_runtime;
use crate::model::{MediaServerEvent, RendererEvent};
use crate::model::{PlaybackState, TrackMetadata};
use crate::music_renderer::{MusicRenderer, PlaybackPositionInfo, PlaylistBinding};

use crate::{DeviceId, DeviceIdentity, DeviceOnline, PlaybackSource};

#[cfg(feature = "pmoserver")]
use crate::openapi::{
    CurrentTrackMetadata, FullRendererSnapshot, QueueItem, QueueSnapshotView, RendererBindingView,
    RendererStateView,
};
use crate::queue::{EnqueueMode, PlaybackItem, QueueBackend, QueueSnapshot};
use crate::registry::DeviceRegistry;

/// Control point minimal :
/// - lance un SsdpClient dans un thread,
/// - passe les SsdpEvent au DiscoveryManager,
/// - applique les DeviceUpdate dans le DeviceRegistry.
///
/// Le runtime est **l'unique source de vérité** pour l'état des renderers :
/// les clients doivent toujours consommer des snapshots consolidés côté serveur
/// et n'utiliser les événements SSE que comme signaux de rafraîchissement.
pub struct ControlPoint {
    registry: Arc<RwLock<DeviceRegistry>>,
    // udn_cache: Arc<Mutex<UDNRegistry>>,
    event_bus: RendererEventBus,
    media_event_bus: MediaServerEventBus,
}

impl ControlPoint {
    /// Crée un ControlPoint et lance le thread de découverte SSDP.
    ///
    /// `timeout_secs` : timeout HTTP pour la récupération des descriptions UPnP.
    pub fn spawn(timeout_secs: u64) -> io::Result<Self> {
        let event_bus = RendererEventBus::new();
        let udn_cache = Arc::new(Mutex::new(UDNRegistry::new()));
        let media_event_bus = MediaServerEventBus::new();
        let registry = Arc::new(RwLock::new(DeviceRegistry::new(
            &event_bus,
            &media_event_bus,
        )));

        // SsdpClient
        let client = SsdpClient::new()?; // pmoupnp::ssdp::SsdpClient

        // Clone pour le thread de renouvellement périodique
        let client_for_renewal = client.clone();

        // Arc utilisé dans le thread
        let registry_for_thread = Arc::clone(&registry);
        let udn_cache_for_thread = Arc::clone(&udn_cache);

        // Thread de découverte
        thread::spawn(move || {
            use crate::discovery::UpnpDiscoveryManager;

            // Créer le gestionnaire de découverte UPNP
            let mut discovery =
                UpnpDiscoveryManager::new(registry_for_thread, udn_cache_for_thread);

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

            // La closure passe les événements SSDP au gestionnaire de découverte
            // Le registry émet automatiquement les événements Online/Offline
            client.run_event_loop(move |event| {
                discovery.handle_ssdp_event(event);
            });
        });

        // Thread de renouvellement périodique des M-SEARCH
        // Envoie des requêtes de découverte toutes les 60 secondes pour forcer
        // les nouveaux appareils à se présenter
        thread::spawn(move || {
            let search_targets = [
                "ssdp:all",
                "urn:schemas-upnp-org:device:MediaRenderer:1",
                "urn:av-openhome-org:device:MediaRenderer:1",
                "urn:schemas-upnp-org:device:MediaServer:1",
                "urn:schemas-wiimu-com:service:PlayQueue:1",
            ];

            loop {
                // Attendre 10 secondes avant le prochain cycle pour découverte rapide
                thread::sleep(Duration::from_secs(10));

                debug!("Sending periodic M-SEARCH for device discovery");

                // Envoyer les M-SEARCH
                for st in &search_targets {
                    if let Err(e) = client_for_renewal.send_msearch(st, 3) {
                        warn!("Failed to send periodic M-SEARCH for {}: {}", st, e);
                    }
                    thread::sleep(Duration::from_millis(200));
                }
            }
        });

        // Thread de vérification de présence périodique
        // Vérifie toutes les 60 secondes les timeouts des devices
        let registry_for_timeout = Arc::clone(&registry);

        thread::spawn(move || {
            loop {
                thread::sleep(Duration::from_secs(60));

                // Le registry vérifie les timeouts et émet automatiquement les événements Offline
                if let Ok(mut reg) = registry_for_timeout.write() {
                    reg.check_timeouts();
                }
            }
        });

        // Thread de découverte mDNS pour Chromecast
        let registry_for_mdns = Arc::clone(&registry);
        let udn_cache_for_mdns = Arc::clone(&udn_cache);
        thread::spawn(move || {
            use crate::discovery::ChromecastDiscoveryManager;
            use futures_util::StreamExt;

            // Créer le gestionnaire de découverte UPNP
            let mut discovery_manager =
                ChromecastDiscoveryManager::new(registry_for_mdns, udn_cache_for_mdns);

            debug!("Starting mDNS discovery thread for Chromecast devices");

            const SERVICE_NAME: &str = "_googlecast._tcp.local";

            // Run async discovery in a blocking task
            async_std::task::block_on(async {
                // Create mDNS discovery stream with 15 second query interval
                // (shorter interval for faster initial discovery)
                match mdns::discover::all(SERVICE_NAME, Duration::from_secs(15)) {
                    Ok(discovery) => {
                        let stream = discovery.listen();
                        futures_util::pin_mut!(stream);

                        debug!("mDNS discovery stream started for Chromecast devices");

                        // Listen to mDNS responses
                        while let Some(result) = stream.next().await {
                            match result {
                                Ok(response) => {
                                    debug!(
                                        "Received mDNS response with {} records",
                                        response.records().count()
                                    );

                                    discovery_manager.handle_mdns_response(response);
                                }
                                Err(e) => {
                                    warn!("mDNS discovery error: {}", e);
                                }
                            }
                        }

                        warn!("mDNS discovery stream ended unexpectedly");
                    }
                    Err(e) => {
                        error!("Failed to start mDNS discovery: {}", e);
                    }
                }
            });
        });

        let polling_cp = ControlPoint {
            registry: Arc::clone(&registry),
            // udn_cache: udn_cache.clone(),
            event_bus: event_bus.clone(),
            media_event_bus: media_event_bus.clone(),
        };

        thread::spawn(move || {
            use std::collections::HashMap;

            // Local cache for change detection (not a source of truth)
            let mut polling_cache: HashMap<DeviceId, RendererRuntimeSnapshot> = HashMap::new();
            let mut tick: u32 = 0;

            loop {
                // Get renderers directly from registry - they already contain backends
                let renderers = {
                    let reg = polling_cp.registry.read().unwrap();
                    reg.list_renderers().unwrap_or_else(|_| vec![])
                };

                for renderer in renderers {
                    if !renderer.is_online() {
                        continue;
                    }

                    let renderer_id = renderer.id();

                    // Get previous snapshot from local cache
                    let prev_snapshot =
                        polling_cache.get(&renderer_id).cloned().unwrap_or_default();
                    let mut new_snapshot = prev_snapshot.clone();
                    let prev_position = prev_snapshot.position.clone();

                    // Poll position every tick (1s) for smooth UI progress
                    if let Ok(position) = renderer.playback_position() {
                        let has_changed = match prev_snapshot.position.as_ref() {
                            Some(prev) => !playback_position_equal(prev, &position),
                            None => true,
                        };

                        if has_changed {
                            polling_cp.emit_renderer_event(RendererEvent::PositionChanged {
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
                                    polling_cp.emit_renderer_event(
                                        RendererEvent::MetadataChanged {
                                            id: renderer_id.clone(),
                                            metadata: metadata.clone(),
                                        },
                                    );
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
                            polling_cp.emit_renderer_event(RendererEvent::StateChanged {
                                id: renderer_id.clone(),
                                state: logical_state.clone(),
                            });
                        }

                        new_snapshot.state = Some(logical_state);
                    }

                    // Poll volume and mute every second (every 2 ticks at 500ms)
                    // for responsive volume control feedback
                    if tick % 2 == 0 {
                        if let Ok(volume) = renderer.volume() {
                            if prev_snapshot.last_volume != Some(volume) {
                                polling_cp.emit_renderer_event(RendererEvent::VolumeChanged {
                                    id: renderer_id.clone(),
                                    volume,
                                });
                            }

                            new_snapshot.last_volume = Some(volume);
                        }

                        if let Ok(mute) = renderer.mute() {
                            if prev_snapshot.last_mute != Some(mute) {
                                polling_cp.emit_renderer_event(RendererEvent::MuteChanged {
                                    id: renderer_id.clone(),
                                    mute,
                                });
                            }

                            new_snapshot.last_mute = Some(mute);
                        }
                    }

                    // Update local cache
                    polling_cache.insert(renderer_id, new_snapshot);
                }

                tick = tick.wrapping_add(1);
                // 250ms polling for smoother UI updates and fluid progress bar
                thread::sleep(Duration::from_millis(250));
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
                            // Find all renderers bound to the updated containers
                            let renderers_to_refresh: Vec<(DeviceId, Arc<MusicRenderer>)> = {
                                let reg = registry_for_media_worker.read().unwrap();
                                match reg.list_renderers() {
                                    Ok(renderers) => renderers
                                        .into_iter()
                                        .filter_map(|renderer| {
                                            // Mark binding for refresh if it matches
                                            if renderer.mark_binding_for_refresh(&server_id, &container_ids) {
                                                Some((renderer.id(), renderer))
                                            } else {
                                                None
                                            }
                                        })
                                        .collect(),
                                    Err(e) => {
                                        warn!(error = %e, "Failed to list renderers for container update");
                                        Vec::new()
                                    }
                                }
                            };

                            // Trigger refresh for each affected renderer (outside of registry lock)
                            for (renderer_id, _renderer) in renderers_to_refresh {
                                debug!(
                                    renderer = renderer_id.0.as_str(),
                                    server = server_id.0.as_str(),
                                    "Triggering queue refresh for bound playlist"
                                );

                                if let Err(err) = refresh_attached_queue_for(
                                    &registry_for_media_worker,
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
                        MediaServerEvent::Online { server_id, info } => {
                            debug!(
                                server = server_id.0.as_str(),
                                friendly_name = info.friendly_name.as_str(),
                                "MediaServer came online"
                            );
                        }
                        MediaServerEvent::Offline { server_id } => {
                            debug!(server = server_id.0.as_str(), "MediaServer went offline");
                        }
                    }
                }
            })?;

        // Periodic refresh worker for bound playlists
        // Every 60 seconds, trigger a refresh for all renderers with active bindings
        let registry_for_periodic = Arc::clone(&registry);
        let event_bus_for_periodic = event_bus.clone();

        thread::Builder::new()
            .name("cp-playlist-periodic-refresh".into())
            .spawn(move || {
                loop {
                    // Sleep for 60 seconds between refresh cycles
                    thread::sleep(Duration::from_secs(60));

                    // Collect all renderers with active bindings and mark them for refresh
                    let renderers_to_refresh: Vec<DeviceId> = {
                        let reg = registry_for_periodic.read().unwrap();
                        match reg.list_renderers() {
                            Ok(renderers) => renderers
                                .into_iter()
                                .filter_map(|renderer| {
                                    // Mark binding for refresh if it exists
                                    if renderer.mark_pending_refresh() {
                                        Some(renderer.id())
                                    } else {
                                        None
                                    }
                                })
                                .collect(),
                            Err(e) => {
                                warn!(error = %e, "Failed to list renderers for periodic refresh");
                                Vec::new()
                            }
                        }
                    };

                    // Trigger refresh for each bound renderer (outside of lock)
                    for renderer_id in renderers_to_refresh {
                        debug!(
                            renderer = renderer_id.0.as_str(),
                            "Periodic refresh triggered for bound playlist"
                        );

                        if let Err(err) = refresh_attached_queue_for(
                            &registry_for_periodic,
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

        // Thread de surveillance des sleep timers
        // Vérifie toutes les secondes les timers actifs et émet des événements
        let registry_for_timer = Arc::clone(&registry);
        let event_bus_for_timer = event_bus.clone();

        thread::spawn(move || {
            use std::collections::HashMap;

            // Track last emitted tick for each renderer to avoid spamming events
            let mut last_tick: HashMap<DeviceId, u32> = HashMap::new();

            loop {
                thread::sleep(Duration::from_secs(1));

                // Get all renderers with active timers
                let renderers = {
                    let reg = registry_for_timer.read().unwrap();
                    match reg.list_renderers() {
                        Ok(renderers) => renderers,
                        Err(err) => {
                            warn!(error = %err, "Failed to list renderers in timer watchdog");
                            continue;
                        }
                    }
                };

                for renderer in &renderers {
                    // Skip renderers without active timers
                    if !renderer.is_sleep_timer_active() {
                        continue;
                    }

                    let renderer_id = renderer.id();
                    let (is_active, duration, remaining) = renderer.sleep_timer_state();

                    if !is_active {
                        continue;
                    }

                    let remaining_seconds = remaining.unwrap_or(0);

                    // Check if timer has expired
                    if renderer.is_sleep_timer_expired() {
                        debug!(
                            renderer = renderer_id.0.as_str(),
                            "Sleep timer expired, stopping playback"
                        );

                        // Mark this as a user-requested stop to prevent auto-advance
                        renderer.mark_user_stop_requested();

                        // Stop playback
                        if let Err(err) = renderer.stop() {
                            warn!(
                                renderer = renderer_id.0.as_str(),
                                error = %err,
                                "Failed to stop renderer when timer expired"
                            );
                        }

                        // Cancel the timer
                        renderer.cancel_sleep_timer();

                        // Emit TimerExpired event
                        event_bus_for_timer.broadcast(RendererEvent::TimerExpired {
                            id: renderer_id.clone(),
                        });

                        // Remove from tick tracking
                        last_tick.remove(&renderer_id);
                    } else {
                        // Emit tick event every second
                        let should_emit_tick = last_tick
                            .get(&renderer_id)
                            .map(|&last| remaining_seconds != last)
                            .unwrap_or(true);

                        if should_emit_tick {
                            event_bus_for_timer.broadcast(RendererEvent::TimerTick {
                                id: renderer_id.clone(),
                                remaining_seconds,
                            });
                            last_tick.insert(renderer_id, remaining_seconds);
                        }
                    }
                }
            }
        });

        Ok(Self {
            registry,
            // udn_cache,
            event_bus,
            media_event_bus,
        })
    }

    /// Accès au DeviceRegistry partagé.
    pub fn registry(&self) -> Arc<RwLock<DeviceRegistry>> {
        Arc::clone(&self.registry)
    }

    /// Snapshot list of music renderers (protocol-agnostic view).
    pub fn list_music_renderers(&self) -> Vec<Arc<MusicRenderer>> {
        let reg = self.registry.read().unwrap();
        reg.list_renderers().unwrap_or_else(|_| vec![])
    }

    /// Return the first music renderer in the registry, if any.
    pub fn default_music_renderer(&self) -> Option<Arc<MusicRenderer>> {
        let reg = self.registry.read().unwrap();
        reg.list_renderers().ok()?.into_iter().next()
    }

    /// Lookup a music renderer by id.
    pub fn music_renderer_by_id(&self, id: &DeviceId) -> Option<Arc<MusicRenderer>> {
        let reg = self.registry.read().unwrap();
        reg.get_renderer(id)
    }

    /// Snapshot list of media servers currently known by the registry.
    pub fn list_media_servers(&self) -> Result<Vec<Arc<MusicServer>>, ControlPointError> {
        let reg = self.registry.read().unwrap();
        reg.list_servers()
    }

    /// Lookup a media server by id.
    pub fn media_server(&self, id: &DeviceId) -> Option<Arc<MusicServer>> {
        let reg = self.registry.read().unwrap();
        reg.get_server(id)
    }

    /// Clears the renderer queue while preserving the playlist binding invariant.
    ///
    /// Invariant reminder: every user-driven queue mutation must call
    /// `detach_playlist_binding` beforehand so that any server-side playlist
    /// attachment stays consistent with the local `QueueBackend` snapshot.
    /// The actual structural change then goes through the backend helpers
    /// (`QueueBackend::clear_queue` via `MusicRenderer::get_queue_mut()`).
    pub fn clear_queue(&self, renderer_id: &DeviceId) -> Result<(), ControlPointError> {
        // User-driven mutation: detach any playlist binding
        self.detach_playlist_binding(renderer_id, "clear_queue");

        // Clear the queue on the backend (backend-agnostic)
        let renderer = self.music_renderer_by_id(renderer_id).ok_or_else(|| {
            ControlPointError::SnapshotError(format!("Renderer {} not found", renderer_id.0))
        })?;

        // Get queue length before clearing
        let removed = renderer.upcoming_len().unwrap_or(0);

        renderer.clear_queue()?;

        // Sync backend state to local cache
        renderer.sync_queue_state()?;

        debug!(
            renderer = renderer_id.0.as_str(),
            items_removed = removed,
            queue_len = 0,
            "Cleared playback queue"
        );

        // Note: QueueUpdated event is emitted automatically by MusicRenderer::clear_queue()

        Ok(())
    }

    /// Appends playback items to the renderer queue and enforces the playlist
    /// binding invariant for user-driven mutations.
    ///
    /// Each caller-triggered queue mutation must first detach any playlist binding
    /// to avoid diverging from the server container, then manipulate the queue
    /// strictly through the `QueueBackend` helpers (here `QueueBackend::enqueue_items`
    /// accessed via `MusicRenderer::get_queue_mut()`).
    pub fn enqueue_items(
        &self,
        renderer_id: &DeviceId,
        items: Vec<PlaybackItem>,
    ) -> Result<(), ControlPointError> {
        self.enqueue_items_with_mode(renderer_id, items, EnqueueMode::AppendToEnd)
    }

    /// Enqueue items to a renderer's queue with a specific enqueue mode.
    ///
    /// This is the low-level version that allows specifying the enqueue mode.
    /// User-driven operations should detach any playlist binding.
    pub fn enqueue_items_with_mode(
        &self,
        renderer_id: &DeviceId,
        items: Vec<PlaybackItem>,
        mode: EnqueueMode,
    ) -> Result<(), ControlPointError> {
        // User-driven mutation: detach any playlist binding
        self.detach_playlist_binding(renderer_id, "enqueue_items");

        // Enqueue items using QueueBackend abstraction (works for both backends)
        let item_count = items.len();
        let renderer = self.music_renderer_by_id(renderer_id).ok_or_else(|| {
            ControlPointError::SnapshotError(format!("Renderer {} not found", renderer_id.0))
        })?;

        renderer.enqueue_items(items, mode)?;
        let new_len = renderer.upcoming_len()?;

        debug!(
            renderer = renderer_id.0.as_str(),
            added = item_count,
            queue_len = new_len,
            mode = ?mode,
            "Enqueued playback items"
        );

        // Note: QueueUpdated event is emitted automatically by MusicRenderer::enqueue_items()

        Ok(())
    }

    /// Shuffles the queue of a renderer and restarts playback from the first track.
    ///
    /// This method:
    /// 1. Detaches the queue from any attached playlist
    /// 2. Stops playback
    /// 3. Randomizes the order of tracks in the queue
    /// 4. Starts playback from the first track
    ///
    /// Note: QueueUpdated event is emitted automatically by MusicRenderer::shuffle_queue()
    /// via its internal call to replace_queue().
    pub fn shuffle_queue(&self, renderer_id: &DeviceId) -> Result<(), ControlPointError> {
        let renderer = self.music_renderer_by_id(renderer_id).ok_or_else(|| {
            ControlPointError::SnapshotError(format!("Renderer {} not found", renderer_id.0))
        })?;

        // Perform the shuffle (this also detaches playlist and restarts playback)
        renderer.shuffle_queue()?;

        debug!(
            renderer = renderer_id.0.as_str(),
            queue_len = renderer.len().unwrap_or(0),
            "Shuffled playback queue"
        );

        Ok(())
    }

    /// Read-only snapshot of the queue items and current index for a renderer.
    ///
    /// Returns both the queue items and the current playing index.
    /// This is the authoritative queue view for UI/REST layers.
    pub fn get_full_queue_snapshot(
        &self,
        renderer_id: &DeviceId,
    ) -> Result<(Vec<PlaybackItem>, Option<usize>), ControlPointError> {
        let renderer = self.music_renderer_by_id(renderer_id).ok_or_else(|| {
            ControlPointError::SnapshotError(format!("Renderer {} not found", renderer_id.0))
        })?;

        let snapshot = renderer.queue_snapshot()?;
        Ok((snapshot.items, snapshot.current_index))
    }

    /// Read-only accessor to the last known metadata for the renderer.
    ///
    /// Useful for UI layers that want to display the currently playing
    /// track even when the renderer is not returning metadata via UPnP.
    pub fn get_current_track_metadata(&self, renderer_id: &DeviceId) -> Option<TrackMetadata> {
        self.music_renderer_by_id(renderer_id)
            .and_then(|r| r.last_metadata())
    }

    /// Gets the backend queue snapshot for renderers with persistent queues.
    ///
    /// Returns the queue snapshot if the renderer has a backend queue,
    /// or None if it doesn't.
    pub fn get_renderer_queue_snapshot(
        &self,
        renderer_id: &DeviceId,
    ) -> Result<QueueSnapshot, ControlPointError> {
        let renderer = self.music_renderer_by_id(renderer_id).ok_or_else(|| {
            ControlPointError::SnapshotError(format!("Renderer {} not found", renderer_id.0))
        })?;
        renderer.queue_snapshot()
    }

    /// Gets the length of the backend queue for renderers with persistent queues.
    ///
    /// Returns the queue length.
    pub fn get_renderer_queue_length(
        &self,
        renderer_id: &DeviceId,
    ) -> Result<usize, ControlPointError> {
        Ok(self.get_renderer_queue_snapshot(renderer_id)?.len())
    }

    /// Build a fully consistent snapshot for UI consumers (state + queue + binding).
    #[cfg(feature = "pmoserver")]
    pub fn renderer_full_snapshot(
        &self,
        renderer_id: &DeviceId,
    ) -> anyhow::Result<FullRendererSnapshot> {
        let renderer = self
            .music_renderer_by_id(renderer_id)
            .ok_or_else(|| anyhow!("Renderer {} not found", renderer_id.0))?;
        let info = renderer.info();

        // Query current state directly from renderer
        let current_state = renderer.playback_state().ok();
        let current_position = renderer.playback_position().ok();
        let current_volume = renderer.volume().ok();
        let current_mute = renderer.mute().ok();
        let last_metadata = renderer.last_metadata();

        // Get queue from renderer (works for all backends)
        let queue_snapshot = renderer
            .queue_snapshot()
            .map_err(|e| anyhow!("Failed to get queue snapshot: {}", e))?;

        let queue_items = queue_snapshot.items;
        let mut queue_current_index = queue_snapshot.current_index;

        let playback_source = renderer.playback_source();
        let queue_len = queue_items.len();

        // Try heuristics to determine current_index if not set
        if queue_current_index.is_none() {
            if let Some(position) = current_position.as_ref() {
                if let Some(uri) = position.track_uri.as_ref() {
                    if let Some(idx) = queue_items.iter().position(|item| item.uri == *uri) {
                        queue_current_index = Some(idx);
                    }
                } else if let Some(track_no) = position.track {
                    let zero_based = track_no.saturating_sub(1) as usize;
                    if zero_based < queue_items.len() {
                        queue_current_index = Some(zero_based);
                    }
                }
            }

            // Final fallback: if playing from queue and no index, assume first track
            if queue_current_index.is_none()
                && matches!(playback_source, PlaybackSource::FromQueue)
                && current_state
                    .as_ref()
                    .map(|state| matches!(state, PlaybackState::Playing | PlaybackState::Paused))
                    .unwrap_or(false)
                && !queue_items.is_empty()
            {
                queue_current_index = Some(0);
            }
        }

        let queue_view_items: Vec<QueueItem> = queue_items
            .iter()
            .enumerate()
            .map(|(index, item)| QueueItem {
                index,
                uri: item.uri.clone(),
                title: item.metadata.as_ref().and_then(|m| m.title.clone()),
                artist: item.metadata.as_ref().and_then(|m| m.artist.clone()),
                album: item.metadata.as_ref().and_then(|m| m.album.clone()),
                album_art_uri: item.metadata.as_ref().and_then(|m| m.album_art_uri.clone()),
                server_id: Some(item.media_server_id.0.clone()),
                object_id: Some(item.didl_id.clone()),
            })
            .collect();

        let queue_view = QueueSnapshotView {
            renderer_id: renderer_id.0.clone(),
            items: queue_view_items,
            current_index: queue_current_index,
        };

        let binding = self.current_queue_playlist_binding(renderer_id).map(
            |(server_id, container_id, has_seen_update)| RendererBindingView {
                server_id: server_id.0,
                container_id,
                has_seen_update,
            },
        );

        let (position_ms, duration_ms) = convert_runtime_position(current_position.as_ref());
        let queue_current_metadata = queue_current_index
            .and_then(|idx| queue_items.get(idx))
            .map(current_track_from_playback_item);

        // Prefer queue metadata, fallback to cached metadata
        let current_track = queue_current_metadata.or_else(|| {
            last_metadata.as_ref().map(|meta| CurrentTrackMetadata {
                title: meta.title.clone(),
                artist: meta.artist.clone(),
                album: meta.album.clone(),
                album_art_uri: meta.album_art_uri.clone(),
            })
        });

        let state_view = RendererStateView {
            id: renderer_id.0.clone(),
            friendly_name: info.friendly_name().to_string(),
            transport_state: current_state
                .as_ref()
                .map(|state| state.as_str().to_string())
                .unwrap_or_else(|| "UNKNOWN".to_string()),
            position_ms,
            duration_ms,
            volume: current_volume.and_then(|value| u8::try_from(value).ok()),
            mute: current_mute,
            queue_len,
            attached_playlist: binding.clone(),
            current_track,
        };

        Ok(FullRendererSnapshot {
            state: state_view,
            queue: queue_view,
            binding,
        })
    }

    /// Clears the renderer's queue.
    ///
    /// Works for both internal queues and persistent backend queues (OpenHome).
    pub fn clear_renderer_queue(&self, renderer_id: &DeviceId) -> Result<(), ControlPointError> {
        let renderer = self.music_renderer_by_id(renderer_id).ok_or_else(|| {
            ControlPointError::SnapshotError(format!("Renderer {} not found", renderer_id.0))
        })?;
        renderer.clear_queue()
    }

    /// Adds a track to the renderer's backend queue.
    ///
    /// For renderers with persistent queues, this adds the track to the queue.
    /// For other renderers, this returns an error.
    ///
    /// Returns the backend-specific track ID if applicable.
    pub fn add_track_to_renderer(
        &self,
        renderer_id: &DeviceId,
        uri: &str,
        metadata: &str,
        after_id: Option<u32>,
        play: bool,
    ) -> anyhow::Result<Option<u32>> {
        let renderer = self
            .music_renderer_by_id(renderer_id)
            .ok_or_else(|| anyhow!("Renderer {} not found", renderer_id.0))?;
        let track_id = renderer.add_track_to_queue(uri, metadata, after_id, play)?;
        renderer.sync_queue_state()?;
        Ok(track_id)
    }

    /// Selects and plays a specific track from the renderer's backend queue.
    ///
    /// For renderers with persistent queues, this uses the track ID.
    /// For other renderers, this returns an error.
    pub fn select_renderer_track(
        &self,
        renderer_id: &DeviceId,
        track_id: u32,
    ) -> Result<(), ControlPointError> {
        let renderer = self.music_renderer_by_id(renderer_id).ok_or_else(|| {
            ControlPointError::ControlPoint(format!("Renderer {} not found", renderer_id.0))
        })?;
        renderer.select_queue_track(track_id)?;
        renderer.set_playback_source(PlaybackSource::FromQueue);
        renderer.sync_queue_state()
    }

    /// Plays the current queue item without advancing the index.
    ///
    /// Useful after a Stop operation to resume playback from the same track.
    /// The method only reads queue content via the runtime helpers and
    /// delegates potential structural mutations to `QueueBackend` (when an item
    /// needs to be restored after a playback error).
    pub fn play_current_from_queue(&self, renderer_id: &DeviceId) -> Result<(), ControlPointError> {
        let renderer = self.music_renderer_by_id(renderer_id).ok_or_else(|| {
            ControlPointError::SnapshotError(format!("Renderer {} not found", renderer_id.0))
        })?;

        // Use generic queue access (works for all backends)
        let Some((item, remaining)) = renderer.peek_current()? else {
            debug!(
                renderer = renderer_id.0.as_str(),
                "play_current_from_queue: queue is empty or no current item"
            );
            renderer.set_playback_source(PlaybackSource::None);
            return Ok(());
        };

        debug!(
            renderer = renderer_id.0.as_str(),
            queue_len = remaining + 1,
            uri = item.uri.as_str(),
            "Playing current playback item from queue"
        );

        // Temporarily disable auto-advance to prevent race condition
        // when renderer sends Stopped event during SetAVTransportURI
        renderer.set_playback_source(PlaybackSource::None);

        // Start playback using play_from_queue which preserves the queue
        if let Err(err) = renderer.play_from_queue() {
            error!(
                renderer = renderer_id.0.as_str(),
                error = %err,
                "Failed to play current item from queue"
            );
            renderer.set_playback_source(PlaybackSource::None);
            return Err(err);
        }

        info!(
            renderer = renderer_id.0.as_str(),
            uri = item.uri.as_str(),
            "Queue playback started (current item)"
        );

        // Save metadata in renderer for current track availability
        // even if the renderer doesn't return metadata in GetPositionInfo
        let metadata = playback_item_track_metadata(&item);
        renderer.set_last_metadata(Some(metadata));

        renderer.set_playback_source(PlaybackSource::FromQueue);
        Ok(())
    }

    /// Advances the queue by one item, starts playback and updates the snapshot.
    pub fn play_next_from_queue(&self, renderer_id: &DeviceId) -> Result<(), ControlPointError> {
        let renderer = self.music_renderer_by_id(renderer_id).ok_or_else(|| {
            ControlPointError::SnapshotError(format!("Renderer {} not found", renderer_id.0))
        })?;

        // Check if queue is empty before trying to play next
        if renderer.len()? == 0 {
            debug!(
                renderer = renderer_id.0.as_str(),
                "play_next_from_queue: queue is empty"
            );
            renderer.set_playback_source(PlaybackSource::None);
            return Ok(());
        }

        // Temporarily disable auto-advance to prevent race condition
        renderer.set_playback_source(PlaybackSource::None);

        // Use the backend's play_next which handles queue advancement correctly for each backend type
        if let Err(err) = renderer.play_next_from_queue() {
            error!(
                renderer = renderer_id.0.as_str(),
                error = %err,
                "Failed to play next item from queue"
            );
            renderer.set_playback_source(PlaybackSource::None);
            return Err(err);
        }

        // Get current item metadata for tracking
        if let Some((item, _)) = renderer.peek_current()? {
            let metadata = playback_item_track_metadata(&item);
            renderer.set_last_metadata(Some(metadata));
        }

        renderer.set_playback_source(PlaybackSource::FromQueue);
        debug!(
            renderer = renderer_id.0.as_str(),
            "Playing next item from queue"
        );

        // Prefetch next track if supported
        self.prefetch_next_track(&renderer, renderer_id);

        // Note: QueueUpdated event is emitted automatically by MusicRenderer::play_next_from_queue()

        Ok(())
    }

    /// Prefetches the next track in the queue if the renderer supports it.
    fn prefetch_next_track(&self, renderer: &Arc<MusicRenderer>, renderer_id: &DeviceId) {
        // Only attempt prefetch if the renderer supports it
        if !renderer.supports_set_next() {
            return;
        }

        // Get the next item from the queue using peek_current
        let Ok(Some((_, remaining))) = renderer.peek_current() else {
            return;
        };

        if remaining == 0 {
            return;
        }

        let queue_snapshot = match renderer.queue_snapshot() {
            Ok(snapshot) => snapshot,
            Err(_) => return,
        };

        // Get next item (current + 1)
        let next_index = queue_snapshot.current_index.map(|i| i + 1).unwrap_or(0);
        let Some(next_item) = queue_snapshot.items.get(next_index) else {
            return;
        };

        let next_didl_metadata = playback_item_to_didl(next_item);
        match renderer.set_next_uri(&next_item.uri, &next_didl_metadata) {
            Ok(_) => debug!(
                renderer = renderer_id.0.as_str(),
                "Prefetched next track via SetNextAVTransportURI"
            ),
            Err(err) => debug!(
                renderer = renderer_id.0.as_str(),
                error = %err,
                "SetNextAVTransportURI failed; continuing without prefetch"
            ),
        }
    }

    /// Jumps to a specific index in the queue and starts playback.
    pub fn play_queue_index(
        &self,
        renderer_id: &DeviceId,
        index: usize,
    ) -> Result<(), ControlPointError> {
        let renderer = self.music_renderer_by_id(renderer_id).ok_or_else(|| {
            ControlPointError::SnapshotError(format!("Renderer {} not found", renderer_id.0))
        })?;

        // Check if index is valid
        if index >= renderer.len()? {
            debug!(
                renderer = renderer_id.0.as_str(),
                index, "play_queue_index: index out of bounds"
            );
            renderer.set_playback_source(PlaybackSource::None);
            return Ok(());
        }

        // Temporarily disable auto-advance to prevent race condition
        renderer.set_playback_source(PlaybackSource::None);

        // Use the backend's play_from_index which handles everything correctly
        if let Err(err) = renderer.play_from_index(index) {
            error!(
                renderer = renderer_id.0.as_str(),
                index,
                error = %err,
                "Failed to play from queue index"
            );
            renderer.set_playback_source(PlaybackSource::None);
            return Err(err);
        }

        info!(
            renderer = renderer_id.0.as_str(),
            index, "Queue playback started at index"
        );

        // Get current item metadata for tracking
        if let Some((item, _)) = renderer.peek_current()? {
            let metadata = playback_item_track_metadata(&item);
            renderer.set_last_metadata(Some(metadata));
        }

        renderer.set_playback_source(PlaybackSource::FromQueue);
        Ok(())
    }

    /// Stop playback in response to user action (e.g., Stop button in UI).
    ///
    /// This method marks the stop as user-requested to prevent automatic
    /// advancement to the next track in the queue when the STOPPED event
    /// is received from the renderer.
    pub fn user_stop(&self, renderer_id: &DeviceId) -> Result<(), ControlPointError> {
        // Get renderer and call stop
        let renderer = self.music_renderer_by_id(renderer_id).ok_or_else(|| {
            ControlPointError::SnapshotError(format!("Renderer {} not found", renderer_id.0))
        })?;

        // Mark that user requested stop before actually stopping
        renderer.mark_user_stop_requested();

        debug!(renderer = renderer_id.0.as_str(), "User-requested stop");

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
    /// through methods like `clear_queue` or `enqueue_items`, so this method is
    /// part of the queue-mutation surface area.
    /// Attach a renderer's queue to a playlist container.
    ///
    /// The queue will be automatically refreshed when the playlist changes on the server.
    pub fn attach_queue_to_playlist(
        &self,
        renderer_id: &DeviceId,
        server_id: DeviceId,
        container_id: String,
    ) -> Result<(), ControlPointError> {
        self.attach_queue_to_playlist_with_options(renderer_id, server_id, container_id, false)
    }

    /// Attach a renderer queue to a playlist with explicit `auto_play` behaviour.
    ///
    /// Same queue-mutation guarantees as [`attach_queue_to_playlist`].
    pub fn attach_queue_to_playlist_with_options(
        &self,
        renderer_id: &DeviceId,
        server_id: DeviceId,
        container_id: String,
        auto_play: bool,
    ) -> Result<(), ControlPointError> {
        self.attach_queue_to_playlist_internal(renderer_id, &server_id, &container_id, auto_play)
    }

    /// Internal implementation shared by every attach wrapper.
    fn attach_queue_to_playlist_internal(
        &self,
        renderer_id: &DeviceId,
        server_id: &DeviceId,
        container_id: &str,
        auto_play: bool,
    ) -> Result<(), ControlPointError> {
        // CRITICAL: When attaching a new playlist to a renderer, we must UNCONDITIONALLY
        // clear the RENDERER queue first (but NOT the local queue cache, which will be
        // replaced by refresh_attached_queue_for() using replace_entire_playlist()).
        //
        // Attach workflow: Clear renderer → Fill with new playlist
        // Update workflow: Gentle sync (preserve current item, use LCS)
        info!(
            renderer = renderer_id.0.as_str(),
            server = server_id.0.as_str(),
            container = container_id,
            "Attaching new playlist: clearing renderer queue"
        );

        // Prepare the renderer for the new playlist (backend-agnostic)
        let renderer = self.music_renderer_by_id(renderer_id).ok_or_else(|| {
            ControlPointError::ControlPoint(format!("Renderer {} not found", renderer_id.0))
        })?;
        renderer.clear_for_playlist_attach()?;

        // Sync backend state to local cache (backend-agnostic)
        renderer.sync_queue_state()?;

        // Clear the local queue (detach binding + clear runtime queue structure)
        self.detach_playlist_binding(renderer_id, "attach_new_playlist");
        renderer.clear_queue()?;

        debug!(
            renderer = renderer_id.0.as_str(),
            "Cleared renderer and local queue for new playlist"
        );

        let binding = PlaylistBinding {
            server_id: server_id.clone(),
            container_id: container_id.to_string(),
            has_seen_update: false,
            pending_refresh: true,
            auto_play_on_refresh: auto_play,
        };

        let renderer = self.music_renderer_by_id(renderer_id).ok_or_else(|| {
            ControlPointError::ControlPoint(format!("Renderer {} not found", renderer_id.0))
        })?;

        renderer.set_playlist_binding(Some(binding));
        info!(
            renderer = renderer_id.0.as_str(),
            server = server_id.0.as_str(),
            container = container_id,
            auto_play,
            "Queue attached to playlist container"
        );

        // Note: BindingChanged event is emitted automatically by MusicRenderer::set_playlist_binding()

        // For initial attach with auto_play, force playback start (don't check if idle)
        let mut auto_start_cb = |rid: &DeviceId| {
            debug!(
                renderer = rid.0.as_str(),
                "Attach callback: forcing playback start (not checking if idle)"
            );
            self.play_current_from_queue(rid)
        };
        let callback: Option<&mut dyn FnMut(&DeviceId) -> Result<(), ControlPointError>> =
            if auto_play {
                Some(&mut auto_start_cb)
            } else {
                None
            };

        refresh_attached_queue_for(&self.registry, renderer_id, &self.event_bus, callback)
    }

    /// Detach a renderer's queue from its associated playlist container.
    ///
    /// Public mutation API paired with `attach_queue_to_playlist*`. After calling
    /// this, the queue will no longer be automatically refreshed from the server.
    /// If no binding existed, this is a no-op.
    pub fn detach_queue_playlist(&self, renderer_id: &DeviceId) {
        self.detach_playlist_binding(renderer_id, "api_detach");
    }

    /// Transfers the queue and playlist binding from one renderer to another.
    ///
    /// This method performs a complete transfer:
    /// 1. Takes a snapshot of the source renderer's queue (including playlist binding)
    /// 2. Clears the destination renderer's queue
    /// 3. Fills the destination renderer's queue with the source snapshot
    /// 4. If the source had a playlist binding, recreates it on the destination
    /// 5. Stops playback on the source renderer
    /// 6. Starts playback on the destination renderer at the same position
    /// 7. Clears the source renderer's queue
    ///
    /// This is useful for seamlessly moving playback from one device to another
    /// while preserving the queue state and playlist synchronization.
    pub fn transfer_queue(
        &self,
        source_renderer_id: &DeviceId,
        dest_renderer_id: &DeviceId,
    ) -> Result<(), ControlPointError> {
        // 1. Get snapshot from source renderer
        let source_snapshot = self.get_renderer_queue_snapshot(source_renderer_id)?;
        let source_binding = self.current_queue_playlist_binding(source_renderer_id);

        tracing::info!(
            source = source_renderer_id.0.as_str(),
            dest = dest_renderer_id.0.as_str(),
            items = source_snapshot.items.len(),
            current_index = ?source_snapshot.current_index,
            has_binding = source_binding.is_some(),
            "Transferring queue between renderers"
        );

        // 2. Clear destination queue
        self.clear_renderer_queue(dest_renderer_id)?;

        // 3. Fill destination queue with source items
        let dest_renderer = self.music_renderer_by_id(dest_renderer_id).ok_or_else(|| {
            ControlPointError::SnapshotError(format!(
                "Destination renderer {} not found",
                dest_renderer_id.0
            ))
        })?;

        dest_renderer
            .replace_queue(source_snapshot.items.clone(), source_snapshot.current_index)?;

        // 4. Recreate playlist binding on destination if source had one
        if let Some((server_id, container_id, _)) = source_binding {
            tracing::debug!(
                dest = dest_renderer_id.0.as_str(),
                server = server_id.0.as_str(),
                container = container_id.as_str(),
                "Recreating playlist binding on destination renderer"
            );
            self.attach_queue_to_playlist(dest_renderer_id, server_id, container_id)?;
        }

        // 5. Stop playback on source renderer
        let source_renderer = self
            .music_renderer_by_id(source_renderer_id)
            .ok_or_else(|| {
                ControlPointError::SnapshotError(format!(
                    "Source renderer {} not found",
                    source_renderer_id.0
                ))
            })?;

        if let Err(e) = source_renderer.stop() {
            tracing::warn!(
                source = source_renderer_id.0.as_str(),
                error = ?e,
                "Failed to stop source renderer (continuing transfer)"
            );
        }

        // 5b. Detach playlist binding from source renderer
        self.detach_queue_playlist(source_renderer_id);
        tracing::debug!(
            source = source_renderer_id.0.as_str(),
            "Detached playlist binding from source renderer"
        );

        // 6. Start playback on destination renderer (if there was a current item)
        if source_snapshot.current_index.is_some() && !source_snapshot.items.is_empty() {
            // play() détecte automatiquement la queue et joue le track courant
            // (comportement unifié pour tous les backends)
            if let Err(e) = dest_renderer.play() {
                tracing::warn!(
                    dest = dest_renderer_id.0.as_str(),
                    error = ?e,
                    "Failed to start playback on destination renderer"
                );
            }
        }

        // 7. Clear source queue
        if let Err(e) = self.clear_renderer_queue(source_renderer_id) {
            tracing::warn!(
                source = source_renderer_id.0.as_str(),
                error = ?e,
                "Failed to clear source renderer queue after transfer"
            );
        }

        tracing::info!(
            source = source_renderer_id.0.as_str(),
            dest = dest_renderer_id.0.as_str(),
            "Queue transfer completed successfully"
        );

        Ok(())
    }

    /// Query the current playlist binding for a renderer's queue, if any.
    ///
    /// Returns `(server_id, container_id, has_seen_update)` if the queue is
    /// bound to a server playlist container, or `None` otherwise.
    pub fn current_queue_playlist_binding(
        &self,
        renderer_id: &DeviceId,
    ) -> Option<(DeviceId, String, bool)> {
        let renderer = self.music_renderer_by_id(renderer_id)?;
        renderer.get_playlist_binding().map(|binding| {
            (
                binding.server_id.clone(),
                binding.container_id.clone(),
                binding.has_seen_update,
            )
        })
    }

    /// Internal helper to detach any playlist binding and notify observers.
    ///
    /// Invariant: every user-driven queue mutation **must** call this method so
    /// that bindings never become out of sync with the local queue snapshot.
    fn detach_playlist_binding(&self, renderer_id: &DeviceId, reason: &str) {
        let renderer = match self.music_renderer_by_id(renderer_id) {
            Some(r) => r,
            None => return,
        };

        let had_binding = renderer.get_playlist_binding();
        renderer.clear_playlist_binding();

        // Note: BindingChanged event is emitted automatically by MusicRenderer::clear_playlist_binding()
        // only if there was a binding to remove

        if let Some(binding) = had_binding {
            info!(
                renderer = renderer_id.0.as_str(),
                server = binding.server_id.0.as_str(),
                container = binding.container_id.as_str(),
                reason = reason,
                "Playlist binding detached"
            );
        } else {
            debug!(
                renderer = renderer_id.0.as_str(),
                reason = reason,
                "detach_playlist_binding: no binding to remove"
            );
        }
    }

    pub(crate) fn emit_renderer_event(&self, event: RendererEvent) {
        self.handle_renderer_event(&event);
        self.event_bus.broadcast(event);
    }

    fn handle_renderer_event(&self, event: &RendererEvent) {
        if let RendererEvent::StateChanged { id, state } = event {
            let Some(renderer) = self.music_renderer_by_id(id) else {
                return;
            };

            match state {
                PlaybackState::Stopped => {
                    // Check if user requested stop (via Stop button in UI)
                    if renderer.check_and_clear_user_stop_requested() {
                        debug!(
                            renderer = id.0.as_str(),
                            "Renderer stopped by user request; not auto-advancing"
                        );
                        renderer.set_playback_source(PlaybackSource::None);
                    } else if renderer.is_playing_from_queue() {
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
                            renderer.set_playback_source(PlaybackSource::None);
                        }
                    } else {
                        renderer.set_playback_source(PlaybackSource::None);
                    }
                }
                PlaybackState::Playing => {
                    renderer.mark_external_if_idle();
                }
                _ => {}
            }
        }
    }
}

#[cfg(feature = "pmoserver")]
fn convert_runtime_position(position: Option<&PlaybackPositionInfo>) -> (Option<u64>, Option<u64>) {
    match position {
        Some(info) => {
            let position_ms = parse_hms_to_ms(info.rel_time.as_deref());
            let duration_ms = parse_hms_to_ms(info.track_duration.as_deref());

            // Validate that position doesn't exceed duration
            // If position > duration, the renderer is reporting invalid data
            // (common during track initialization on some UPNP renderers)
            match (position_ms, duration_ms) {
                (Some(pos), Some(dur)) if pos > dur => {
                    // Position exceeds duration - invalid state during initialization
                    // Return None for position to avoid showing bogus timestamps
                    (None, duration_ms)
                }
                _ => (position_ms, duration_ms),
            }
        }
        None => (None, None),
    }
}

#[cfg(feature = "pmoserver")]
fn parse_hms_to_ms(hms: Option<&str>) -> Option<u64> {
    let value = hms?;
    let parts: Vec<&str> = value.split(':').collect();
    if parts.len() != 3 {
        return None;
    }

    let hours: u64 = parts[0].parse().ok()?;
    let minutes: u64 = parts[1].parse().ok()?;
    let seconds: u64 = parts[2].parse().ok()?;

    Some((hours * 3600 + minutes * 60 + seconds) * 1000)
}

/// Snapshot of renderer state used for change detection in the polling thread.
/// This is a local cache, not a source of truth.
#[derive(Clone, Default)]
struct RendererRuntimeSnapshot {
    state: Option<PlaybackState>,
    position: Option<PlaybackPositionInfo>,
    last_volume: Option<u16>,
    last_mute: Option<bool>,
    last_metadata: Option<TrackMetadata>,
}

/// Internal helper to refresh a renderer's playback queue from its bound
/// playlist container.
///
/// This function is called automatically when a ContentDirectory event indicates
/// that the bound container has been updated. It attempts to preserve the
/// currently playing item when possible.
fn refresh_attached_queue_for(
    registry: &Arc<RwLock<DeviceRegistry>>,
    renderer_id: &DeviceId,
    event_bus: &RendererEventBus,
    mut after_refresh: Option<&mut dyn FnMut(&DeviceId) -> Result<(), ControlPointError>>,
) -> Result<(), ControlPointError> {
    // Step 1: Get renderer from registry
    let renderer = {
        let reg = registry.read().unwrap();
        reg.get_renderer(renderer_id)
    };

    let renderer = match renderer {
        Some(r) => r,
        None => {
            debug!(
                renderer = renderer_id.0.as_str(),
                "refresh_attached_queue_for: renderer not found"
            );
            return Ok(());
        }
    };

    // Check if there's a binding and if it needs refresh
    if !renderer.has_pending_refresh() {
        debug!(
            renderer = renderer_id.0.as_str(),
            "refresh_attached_queue_for: no pending refresh needed"
        );
        return Ok(());
    }

    let (server_id, container_id) = {
        let binding = match renderer.get_playlist_binding() {
            Some(b) => b,
            None => {
                debug!(
                    renderer = renderer_id.0.as_str(),
                    "refresh_attached_queue_for: no binding present"
                );
                return Ok(());
            }
        };

        (binding.server_id.clone(), binding.container_id.clone())
    };

    // Reset the pending_refresh flag and consume auto_play
    renderer.reset_pending_refresh();
    let auto_play = renderer.consume_auto_play();

    // Step 2: Get server from registry
    let music_server = {
        let reg = registry.read().unwrap();
        reg.get_server(&server_id)
    };

    let music_server = match music_server {
        Some(s) => s,
        None => {
            warn!(
                renderer = renderer_id.0.as_str(),
                server = server_id.0.as_str(),
                "refresh_attached_queue_for: server not found in registry"
            );
            return Ok(());
        }
    };

    if !music_server.is_online() {
        debug!(
            renderer = renderer_id.0.as_str(),
            server = server_id.0.as_str(),
            "refresh_attached_queue_for: server offline, skipping refresh"
        );
        return Ok(());
    }

    // Step 3: Browse container

    const MAX_BROWSE_ATTEMPTS: usize = 3;
    const BROWSE_RETRY_DELAY_MS: u64 = 200;
    let mut attempt = 1;
    let entries = loop {
        match music_server.browse_children(&container_id, 0, 64) {
            Ok(e) => break e,
            Err(err) => {
                if attempt >= MAX_BROWSE_ATTEMPTS {
                    warn!(
                        renderer = renderer_id.0.as_str(),
                        server = server_id.0.as_str(),
                        container = container_id.as_str(),
                        attempts = attempt,
                        error = %err,
                        "Failed to browse playlist container for refresh"
                    );
                    return Err(err);
                }

                debug!(
                    renderer = renderer_id.0.as_str(),
                    server = server_id.0.as_str(),
                    container = container_id.as_str(),
                    attempt,
                    error = %err,
                    "Browse attempt failed, retrying"
                );
                thread::sleep(Duration::from_millis(
                    BROWSE_RETRY_DELAY_MS * attempt as u64,
                ));
                attempt += 1;
            }
        }
    };

    debug!(
        renderer = renderer_id.0.as_str(),
        server = server_id.0.as_str(),
        container = container_id.as_str(),
        total_entries = entries.len(),
        containers = entries.iter().filter(|e| e.is_container).count(),
        items_count = entries.iter().filter(|e| !e.is_container).count(),
        "Browse returned entries for playlist refresh"
    );

    // Step 4: Convert MediaEntry to PlaybackItem
    let new_items: Vec<PlaybackItem> = entries
        .iter()
        .filter_map(|entry| playback_item_from_entry(music_server.clone(), entry))
        .collect();

    if new_items.is_empty() {
        warn!(
            renderer = renderer_id.0.as_str(),
            server = server_id.0.as_str(),
            container = container_id.as_str(),
            total_entries = entries.len(),
            "Refreshed playlist is empty, clearing queue"
        );
        renderer.clear_queue()?;

        // Emit QueueUpdated event
        event_bus.broadcast(RendererEvent::QueueUpdated {
            id: renderer_id.clone(),
            queue_length: 0,
        });

        return Ok(());
    }

    // Step 5: GENTLE SYNCHRONIZATION using sync_queue()
    // This uses LCS algorithm to minimize playlist operations and avoid interrupting playback

    info!(
        renderer = renderer_id.0.as_str(),
        server = server_id.0.as_str(),
        container = container_id.as_str(),
        total_items = new_items.len(),
        "Refreshing playlist with sync_queue"
    );

    renderer.sync_queue(new_items)?;

    let final_queue_len = {
        let snapshot = renderer.queue_snapshot()?;
        snapshot.items.len()
    };

    // Emit QueueUpdated event
    event_bus.broadcast(RendererEvent::QueueUpdated {
        id: renderer_id.clone(),
        queue_length: final_queue_len,
    });

    if auto_play {
        if let Some(callback) = after_refresh.as_deref_mut() {
            debug!(
                renderer = renderer_id.0.as_str(),
                server = server_id.0.as_str(),
                container = container_id.as_str(),
                "Auto-play enabled: calling callback to start playback"
            );
            if let Err(err) = callback(renderer_id) {
                warn!(
                    renderer = renderer_id.0.as_str(),
                    server = server_id.0.as_str(),
                    container = container_id.as_str(),
                    error = %err,
                    "Failed to auto-start playback after playlist refresh"
                );
            } else {
                info!(
                    renderer = renderer_id.0.as_str(),
                    server = server_id.0.as_str(),
                    container = container_id.as_str(),
                    "Auto-play callback completed successfully"
                );
            }
        } else {
            debug!(
                renderer = renderer_id.0.as_str(),
                "Auto-play enabled but no callback provided"
            );
        }
    } else {
        debug!(
            renderer = renderer_id.0.as_str(),
            "Auto-play disabled, skipping playback start"
        );
    }

    Ok(())
}

fn didl_item_from_playback_item(item: &PlaybackItem) -> DidlItem {
    let metadata = item.metadata.as_ref();
    let title = metadata
        .and_then(|m| m.title.as_deref())
        .unwrap_or("Unknown")
        .to_string();
    let creator = metadata
        .and_then(|m| m.creator.clone())
        .or_else(|| metadata.and_then(|m| m.artist.clone()));

    DidlItem {
        id: item.didl_id.clone(),
        parent_id: "-1".to_string(),
        restricted: Some("1".to_string()),
        title,
        creator,
        class: "object.item.audioItem.musicTrack".to_string(),
        artist: metadata.and_then(|m| m.artist.clone()),
        album: metadata.and_then(|m| m.album.clone()),
        genre: metadata.and_then(|m| m.genre.clone()),
        album_art: metadata.and_then(|m| m.album_art_uri.clone()),
        album_art_pk: None,
        date: metadata.and_then(|m| m.date.clone()),
        original_track_number: metadata.and_then(|m| m.track_number.clone()),
        resources: vec![DidlResource {
            protocol_info: item.protocol_info.clone(),
            bits_per_sample: None,
            sample_frequency: None,
            nr_audio_channels: None,
            duration: None,
            url: item.uri.clone(),
        }],
        descriptions: Vec::new(),
    }
}

fn playback_item_to_didl(item: &PlaybackItem) -> String {
    let didl_item = didl_item_from_playback_item(item);
    let didl = DIDLLite {
        xmlns: "urn:schemas-upnp-org:metadata-1-0/DIDL-Lite/".to_string(),
        xmlns_upnp: Some("urn:schemas-upnp-org:metadata-1-0/upnp/".to_string()),
        xmlns_dc: Some("http://purl.org/dc/elements/1.1/".to_string()),
        xmlns_dlna: None,
        xmlns_sec: None,
        xmlns_pv: None,
        containers: Vec::new(),
        items: vec![didl_item],
    };

    match to_didl_string(&didl) {
        Ok(xml) => xml,
        Err(err) => {
            warn!(error = %err, "Failed to serialize DIDL-Lite metadata");
            String::new()
        }
    }
}

fn playback_item_track_metadata(item: &PlaybackItem) -> TrackMetadata {
    item.metadata.clone().unwrap_or_else(|| TrackMetadata {
        title: None,
        artist: None,
        album: None,
        genre: None,
        album_art_uri: None,
        date: None,
        track_number: None,
        creator: None,
    })
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
    // Rule 1: Arylic / LinkPlay sometimes report STOPPED while the stream is
    // actually playing. If we detect that the relative time advances between
    // two polls, we treat this as Playing.
    if let PlaybackState::Stopped = raw {
        if let (Some(prev), Some(curr)) = (prev_position, current_position) {
            if let (Some(prev_rel), Some(curr_rel)) = (
                parse_optional_hms_to_secs(&prev.rel_time),
                parse_optional_hms_to_secs(&curr.rel_time),
            ) {
                if curr_rel > prev_rel {
                    let delta = curr_rel - prev_rel;
                    // Our poll loop runs every 1s; accept small jitter in the delta.
                    if delta <= 5 {
                        return PlaybackState::Playing;
                    }
                }
            }
        }
    }

    // Rule 2: Some devices report NO_MEDIA_PRESENT while exposing a non-zero
    // track duration. In practice this behaves like a stopped transport with
    // a loaded track.
    if let PlaybackState::NoMedia = raw {
        let duration_secs = current_position
            .and_then(|p| parse_optional_hms_to_secs(&p.track_duration))
            .or_else(|| prev_position.and_then(|p| parse_optional_hms_to_secs(&p.track_duration)));

        if matches!(duration_secs, Some(d) if d > 0) {
            return PlaybackState::Stopped;
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

#[cfg(feature = "pmoserver")]
fn current_track_from_playback_item(item: &PlaybackItem) -> CurrentTrackMetadata {
    let meta = item.metadata.as_ref();
    CurrentTrackMetadata {
        title: meta.and_then(|m| m.title.clone()),
        artist: meta.and_then(|m| m.artist.clone()),
        album: meta.and_then(|m| m.album.clone()),
        album_art_uri: meta.and_then(|m| m.album_art_uri.clone()),
    }
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

    debug!(
        title = item.title.as_str(),
        has_album_art = item.album_art.is_some(),
        album_art_uri = item.album_art.as_deref(),
        "Extracted metadata from position info"
    );

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
