use std::collections::{HashMap, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::marker::PhantomData;
use std::net::{IpAddr, TcpListener, TcpStream, UdpSocket};
use std::ops::{Deref, DerefMut};
use std::sync::{Arc, Mutex, MutexGuard, RwLock};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context};
use crossbeam_channel::{unbounded, Receiver, Sender};
use pmodidl::{DIDLLite, Item as DidlItem, Resource as DidlResource};
use pmoupnp::ssdp::SsdpClient;
use quick_xml::se::to_string as to_didl_string;
use thiserror::Error;
use tracing::{debug, error, info, trace, warn};
use ureq::{http, Agent};
use xmltree::{Element, XMLNode};

pub mod music_queue;
pub mod openhome_queue;

pub const OPENHOME_SNAPSHOT_CACHE_TTL: Duration = Duration::from_secs(2);

use crate::capabilities::{
    PlaybackPosition, PlaybackPositionInfo, PlaybackState, PlaybackStatus, TransportControl,
    VolumeControl,
};
use crate::control_point::music_queue::MusicQueue;
use crate::control_point::openhome_queue::OpenHomeQueue;
use crate::discovery::DiscoveryManager;
use crate::events::{MediaServerEventBus, RendererEventBus};
use crate::media_server::{MediaBrowser, MediaEntry, MediaServerInfo, MusicServer, ServerId};
use crate::media_server_events::spawn_media_server_event_runtime;
use crate::model::TrackMetadata;
use crate::model::{MediaServerEvent, RendererEvent, RendererId, RendererInfo};
use crate::music_renderer::{
    set_openhome_queue_provider, OpenHomeQueueProvider, RendererRuntimeState,
};
#[cfg(feature = "pmoserver")]
use crate::openapi::{
    CurrentTrackMetadata, FullRendererSnapshot, QueueItem, QueueSnapshotView, RendererBindingView,
    RendererStateView,
};
use crate::openhome::{
    build_info_client, build_playlist_client, build_product_client, OhServiceKind,
};
use crate::openhome_client::parse_track_metadata_from_didl;
use crate::openhome_playlist::{OpenHomePlaylistSnapshot, OpenHomePlaylistTrack};
use crate::openhome_renderer::{format_seconds, map_openhome_state};
use crate::provider::HttpXmlDescriptionProvider;
use crate::queue_backend::{EnqueueMode, PlaybackItem, QueueBackend};
use crate::queue_interne::InternalQueue;
use crate::registry::{DeviceRegistry, DeviceRegistryRead, DeviceUpdate};
use crate::upnp_renderer::UpnpRenderer;
use crate::MusicRenderer;

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

#[derive(Debug, Error)]
pub enum OpenHomeAccessError {
    #[error("Renderer {0} not found")]
    RendererNotFound(String),
    #[error("Renderer {0} has no OpenHome playlist service")]
    PlaylistNotSupported(String),
}

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
    event_bus: RendererEventBus,
    media_event_bus: MediaServerEventBus,
    runtime: Arc<RuntimeState>,
    /// Optional attachment between a renderer playback queue and a
    /// server-side DIDL-Lite playlist container.
    ///
    /// Key   : RendererId
    /// Value : PlaylistBinding
    playlist_bindings: Arc<Mutex<HashMap<RendererId, PlaylistBinding>>>,
    /// Cache of MusicRenderer instances to avoid recreating them.
    /// This is critical for Chromecast which maintains a persistent TLS connection.
    ///
    /// Key   : RendererId
    /// Value : MusicRenderer
    renderer_cache: Arc<Mutex<HashMap<RendererId, MusicRenderer>>>,
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
        set_openhome_queue_provider(Arc::new(RuntimeOpenHomeQueueProvider {
            runtime: Arc::clone(&runtime),
        }));
        let playlist_bindings = Arc::new(Mutex::new(HashMap::new()));
        let renderer_cache = Arc::new(Mutex::new(HashMap::new()));

        // SsdpClient
        let client = SsdpClient::new()?; // pmoupnp::ssdp::SsdpClient

        // Clone pour le thread de renouvellement périodique
        let client_for_renewal = client.clone();

        // Arc utilisé dans le thread
        let registry_for_thread = Arc::clone(&registry);
        let event_bus_for_discovery = event_bus.clone();
        let media_event_bus_for_discovery = media_event_bus.clone();

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
                        // Émettre les événements Online/Offline avant d'appliquer l'update
                        match &update {
                            DeviceUpdate::RendererOnline(info) => {
                                event_bus_for_discovery.broadcast(RendererEvent::Online {
                                    id: info.id.clone(),
                                    info: info.clone(),
                                });
                            }
                            DeviceUpdate::RendererOfflineById(id) => {
                                event_bus_for_discovery.broadcast(RendererEvent::Offline {
                                    id: id.clone(),
                                });
                            }
                            DeviceUpdate::RendererOfflineByUdn(udn) => {
                                // Trouver l'ID avant de marquer offline
                                if let Some(renderer) = reg.get_renderer_by_udn(udn) {
                                    event_bus_for_discovery.broadcast(RendererEvent::Offline {
                                        id: renderer.id.clone(),
                                    });
                                }
                            }
                            DeviceUpdate::ServerOnline(info) => {
                                media_event_bus_for_discovery.broadcast(MediaServerEvent::Online {
                                    server_id: info.id.clone(),
                                    info: info.clone(),
                                });
                            }
                            DeviceUpdate::ServerOfflineById(id) => {
                                media_event_bus_for_discovery.broadcast(MediaServerEvent::Offline {
                                    server_id: id.clone(),
                                });
                            }
                            DeviceUpdate::ServerOfflineByUdn(udn) => {
                                // Trouver l'ID avant de marquer offline
                                if let Some(server) = reg.get_server_by_udn(udn) {
                                    media_event_bus_for_discovery.broadcast(MediaServerEvent::Offline {
                                        server_id: server.id.clone(),
                                    });
                                }
                            }
                        }

                        reg.apply_update(update);
                    }
                }
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
                // Attendre 60 secondes avant le prochain cycle
                thread::sleep(Duration::from_secs(60));

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
        // Vérifie toutes les 60 secondes que les devices connus sont toujours accessibles
        let registry_for_presence = Arc::clone(&registry);
        let event_bus_for_presence = event_bus.clone();
        let media_event_bus_for_presence = media_event_bus.clone();
        thread::spawn(move || {
            use ureq::Agent;

            // HTTP client avec timeout court pour les vérifications de présence
            let config = Agent::config_builder()
                .timeout_global(Some(Duration::from_secs(5)))
                .build();
            let agent: Agent = config.into();

            loop {
                // Attendre 60 secondes avant le prochain cycle
                thread::sleep(Duration::from_secs(60));

                debug!("Starting periodic presence check for devices");

                let mut updates = Vec::new();

                // Lire la liste des devices
                if let Ok(reg) = registry_for_presence.read() {
                    // Vérifier les renderers
                    for renderer in reg.list_renderers() {
                        if !renderer.online {
                            continue; // Skip déjà offline
                        }

                        // Faire un HTTP HEAD pour vérifier la présence
                        match agent.head(&renderer.location).call() {
                            Ok(_) => {
                                // Device répond toujours
                                debug!("Renderer {} ({:?}) is still online",
                                       renderer.friendly_name, renderer.id);
                            }
                            Err(e) => {
                                // Device ne répond plus
                                warn!("Renderer {} ({:?}) is no longer responding: {} - marking offline",
                                      renderer.friendly_name, renderer.id, e);
                                updates.push(DeviceUpdate::RendererOfflineById(renderer.id));
                            }
                        }
                    }

                    // Vérifier les servers
                    for server in reg.list_servers() {
                        if !server.online {
                            continue; // Skip déjà offline
                        }

                        // Faire un HTTP HEAD pour vérifier la présence
                        match agent.head(&server.location).call() {
                            Ok(_) => {
                                // Device répond toujours
                                debug!("Server {} ({:?}) is still online",
                                       server.friendly_name, server.id);
                            }
                            Err(e) => {
                                // Device ne répond plus
                                warn!("Server {} ({:?}) is no longer responding: {} - marking offline",
                                      server.friendly_name, server.id, e);
                                updates.push(DeviceUpdate::ServerOfflineById(server.id));
                            }
                        }
                    }
                }

                // Appliquer les updates et émettre les événements
                if !updates.is_empty() {
                    if let Ok(mut reg) = registry_for_presence.write() {
                        for update in updates {
                            // Émettre les événements Offline
                            match &update {
                                DeviceUpdate::RendererOfflineById(id) => {
                                    event_bus_for_presence.broadcast(RendererEvent::Offline {
                                        id: id.clone(),
                                    });
                                }
                                DeviceUpdate::ServerOfflineById(id) => {
                                    media_event_bus_for_presence.broadcast(MediaServerEvent::Offline {
                                        server_id: id.clone(),
                                    });
                                }
                                _ => {}
                            }

                            reg.apply_update(update);
                        }
                    }
                }
            }
        });

        // Thread de découverte mDNS pour Chromecast
        let registry_for_mdns = Arc::clone(&registry);
        thread::spawn(move || {
            use crate::chromecast_discovery;
            use futures_util::StreamExt;

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
                                    debug!("Received mDNS response with {} records",
                                           response.records().count());

                                    // Process the mDNS response
                                    if let Some(update) = chromecast_discovery::process_mdns_response(response) {
                                        debug!("Processed Chromecast device update: {:?}", update);

                                        // Update the registry
                                        let mut registry = registry_for_mdns.write().unwrap();
                                        registry.apply_update(update);
                                    }
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

        let runtime_cp = ControlPoint {
            registry: Arc::clone(&registry),
            event_bus: event_bus.clone(),
            media_event_bus: media_event_bus.clone(),
            runtime: Arc::clone(&runtime),
            playlist_bindings: Arc::clone(&playlist_bindings),
            renderer_cache: Arc::clone(&renderer_cache),
        };

        thread::spawn(move || {
            let mut tick: u32 = 0;

            loop {
                let infos = {
                    let reg = runtime_cp.registry.read().unwrap();
                    reg.list_renderers()
                };

                // Build a map of current renderer IDs for cleanup
                let current_ids: HashSet<RendererId> = infos.iter().map(|i| i.id.clone()).collect();

                // Remove offline renderers from shared cache
                {
                    let mut cache = runtime_cp.renderer_cache.lock().unwrap();
                    cache.retain(|id, _| current_ids.contains(id));
                }

                // Get or create renderers from shared cache
                let renderers: Vec<MusicRenderer> = infos
                    .into_iter()
                    .filter_map(|info| {
                        let id = info.id.clone();

                        // Try to get from cache first
                        {
                            let cache = runtime_cp.renderer_cache.lock().unwrap();
                            if let Some(renderer) = cache.get(&id) {
                                return Some(renderer.clone());
                            }
                        }

                        // Create new renderer and add to cache
                        if let Some(renderer) = MusicRenderer::from_registry_info(info, &runtime_cp.registry) {
                            let mut cache = runtime_cp.renderer_cache.lock().unwrap();
                            cache.insert(id, renderer.clone());
                            Some(renderer)
                        } else {
                            None
                        }
                    })
                    .collect();

                for renderer in renderers {
                    let info = renderer.info();

                    if !info.online {
                        continue;
                    }

                    let backend = if info.capabilities.has_oh_playlist {
                        PlaylistBackend::OpenHome
                    } else {
                        PlaylistBackend::PMOQueue
                    };
                    let previous_backend = runtime_cp.runtime.playlist_backend(&info.id);
                    let runtime_entry_exists = runtime_cp.runtime.has_entry(&info.id);

                    // Initialize queue if: backend changed OR runtime entry doesn't exist yet
                    if previous_backend != backend || !runtime_entry_exists {
                        runtime_cp.runtime.set_playlist_backend(&info.id, backend);
                        match backend {
                            PlaylistBackend::OpenHome => {
                                if let Some(queue) = build_openhome_queue(info) {
                                    runtime_cp
                                        .runtime
                                        .set_music_queue(&info.id, MusicQueue::OpenHome(queue));
                                } else {
                                    runtime_cp.runtime.set_music_queue(
                                        &info.id,
                                        MusicQueue::Internal(InternalQueue::new()),
                                    );
                                }
                            }
                            PlaylistBackend::PMOQueue => {
                                runtime_cp.runtime.set_music_queue(
                                    &info.id,
                                    MusicQueue::Internal(InternalQueue::new()),
                                );
                            }
                        }
                        if matches!(backend, PlaylistBackend::OpenHome) {
                            if let Err(err) = sync_openhome_playlist(
                                &runtime_cp.registry,
                                &runtime_cp.runtime,
                                &runtime_cp.event_bus,
                                &info.id,
                            ) {
                                debug!(
                                    renderer = info.id.0.as_str(),
                                    error = %err,
                                    "Initial OpenHome playlist sync failed"
                                );
                            }
                        }
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
                                    runtime_cp.emit_renderer_event(
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

        let (oh_event_tx, oh_event_rx) = unbounded::<RendererEvent>();
        let event_forwarder_cp = ControlPoint {
            registry: Arc::clone(&registry),
            event_bus: event_bus.clone(),
            media_event_bus: media_event_bus.clone(),
            runtime: Arc::clone(&runtime),
            playlist_bindings: Arc::clone(&playlist_bindings),
            renderer_cache: Arc::clone(&renderer_cache),
        };

        thread::Builder::new()
            .name("cp-openhome-event-forwarder".into())
            .spawn(move || {
                while let Ok(event) = oh_event_rx.recv() {
                    event_forwarder_cp.emit_renderer_event(event);
                }
            })?;

        spawn_openhome_event_runtime(
            Arc::clone(&registry),
            Arc::clone(&runtime),
            event_bus.clone(),
            oh_event_tx,
            Arc::clone(&playlist_bindings),
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
            .spawn(move || loop {
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
                    MediaServerEvent::Online { server_id, info } => {
                        debug!(
                            server = server_id.0.as_str(),
                            friendly_name = info.friendly_name.as_str(),
                            "MediaServer came online"
                        );
                    }
                    MediaServerEvent::Offline { server_id } => {
                        debug!(
                            server = server_id.0.as_str(),
                            "MediaServer went offline"
                        );
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
            renderer_cache,
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

    /// Internal helper to get or create a renderer from the cache.
    /// This ensures that Chromecast renderers maintain their persistent connections.
    fn get_or_create_renderer(&self, info: RendererInfo) -> Option<MusicRenderer> {
        let id = info.id.clone();

        // Try to get from cache first
        {
            let cache = self.renderer_cache.lock().unwrap();
            if let Some(renderer) = cache.get(&id) {
                return Some(renderer.clone());
            }
        }

        // Not in cache, create new renderer
        if let Some(renderer) = MusicRenderer::from_registry_info(info, &self.registry) {
            // Add to cache
            let mut cache = self.renderer_cache.lock().unwrap();
            cache.insert(id, renderer.clone());
            Some(renderer)
        } else {
            None
        }
    }

    /// Snapshot list of music renderers (protocol-agnostic view).
    pub fn list_music_renderers(&self) -> Vec<MusicRenderer> {
        let infos = {
            let reg = self.registry.read().unwrap();
            reg.list_renderers()
        };

        // Clean up cache - remove renderers that are no longer in the registry
        {
            let current_ids: HashSet<RendererId> = infos.iter().map(|i| i.id.clone()).collect();
            let mut cache = self.renderer_cache.lock().unwrap();
            cache.retain(|id, _| current_ids.contains(id));
        }

        infos
            .into_iter()
            .filter_map(|info| self.get_or_create_renderer(info))
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
            .find_map(|info| self.get_or_create_renderer(info))
    }

    /// Lookup a music renderer by id.
    pub fn music_renderer_by_id(&self, id: &RendererId) -> Option<MusicRenderer> {
        let info = {
            let reg = self.registry.read().unwrap();
            reg.get_renderer(id)
        }?;

        self.get_or_create_renderer(info)
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

    /// Clears the renderer queue while preserving the playlist binding invariant.
    ///
    /// Invariant reminder: every user-driven queue mutation must call
    /// `detach_playlist_binding` beforehand so that any server-side playlist
    /// attachment stays consistent with the local `QueueBackend` snapshot.
    /// The actual structural change then goes through the backend helpers
    /// (`QueueBackend::clear_queue` via `RuntimeState::with_music_queue_mut`).
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
        self.detach_playlist_binding(renderer_id, "clear_queue");

        if self.runtime.uses_openhome_playlist(renderer_id) {
            let renderer = self.openhome_renderer(renderer_id)?;
            renderer.openhome_playlist_clear()?;
            self.sync_openhome_playlist_for(renderer_id)?;
            debug!(
                renderer = renderer_id.0.as_str(),
                "Cleared OpenHome playlist"
            );
            return Ok(());
        }

        let removed = self.runtime.with_music_queue_mut(renderer_id, |queue| {
            let removed = queue.upcoming_len()?;
            queue.clear_queue()?;
            Ok(removed)
        })?;

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

    /// Appends playback items to the renderer queue and enforces the playlist
    /// binding invariant for user-driven mutations.
    ///
    /// Each caller-triggered queue mutation must first detach any playlist binding
    /// to avoid diverging from the server container, then manipulate the queue
    /// strictly through the `QueueBackend` helpers (here `QueueBackend::enqueue_items`
    /// inside `RuntimeState::with_music_queue_mut`).
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
        self.detach_playlist_binding(renderer_id, "enqueue_items");

        if self.runtime.uses_openhome_playlist(renderer_id) {
            self.enqueue_items_openhome(renderer_id, items)?;
            return Ok(());
        }

        let item_count = items.len();
        let new_len = self.runtime.with_music_queue_mut(renderer_id, |queue| {
            queue.enqueue_items(items, EnqueueMode::AppendToEnd)?;
            queue.upcoming_len()
        })?;

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

    /// Read-only snapshot of the upcoming queue items for a renderer.
    ///
    /// This helper never mutates the runtime. It simply exposes the pending
    /// items as seen by the local `QueueBackend`. For a full `(items, index)`
    /// view, prefer [`get_full_queue_snapshot`].
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

    /// Read-only helper returning both queue items and the current index.
    ///
    /// This is the most detailed queue view exposed publicly and is meant
    /// for UI/REST layers that need an authoritative snapshot without
    /// mutating the runtime.
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

    /// Read-only accessor to the last known metadata for the renderer.
    ///
    /// Useful for UI layers that want to display the currently playing
    /// track even when the renderer is not returning metadata via UPnP.
    pub fn get_current_track_metadata(&self, renderer_id: &RendererId) -> Option<TrackMetadata> {
        self.runtime.current_track_metadata(renderer_id)
    }

    /// Force a resynchronization of the OpenHome playlist cache for a renderer.
    ///
    /// This is used by external APIs after mutating the native playlist so that
    /// the local queue mirrors the renderer state.
    pub fn refresh_openhome_playlist(&self, renderer_id: &RendererId) -> anyhow::Result<()> {
        self.sync_openhome_playlist_for(renderer_id)
    }

    pub fn get_openhome_playlist_snapshot(
        &self,
        renderer_id: &RendererId,
    ) -> anyhow::Result<OpenHomePlaylistSnapshot> {
        let renderer = self.openhome_renderer(renderer_id)?;
        renderer.openhome_playlist_snapshot()
    }

    pub fn get_cached_openhome_playlist_snapshot(
        &self,
        renderer_id: &RendererId,
        ttl: Duration,
    ) -> anyhow::Result<OpenHomePlaylistSnapshot> {
        let renderer = self.openhome_renderer(renderer_id)?;
        self.runtime.openhome_snapshot_cached(&renderer, ttl)
    }

    pub fn get_openhome_playlist_len(&self, renderer_id: &RendererId) -> anyhow::Result<usize> {
        let renderer = self.openhome_renderer(renderer_id)?;
        renderer.openhome_playlist_len()
    }

    /// Build a fully consistent snapshot for UI consumers (state + queue + binding).
    #[cfg(feature = "pmoserver")]
    pub fn renderer_full_snapshot(
        &self,
        renderer_id: &RendererId,
    ) -> anyhow::Result<FullRendererSnapshot> {
        let renderer = self
            .music_renderer_by_id(renderer_id)
            .ok_or_else(|| anyhow!("Renderer {} not found", renderer_id.0))?;
        let info = renderer.info();

        // MICRO-PATCH 5: Chemin OpenHome complètement découplé du miroir local
        if self.runtime.uses_openhome_playlist(renderer_id) {
            // Pour OpenHome: récupérer UNIQUEMENT runtime_snapshot pour volume/state/position
            // Ne PAS utiliser queue_items/current_index du runtime (miroir local)
            let (runtime_snapshot, _, _) = self.runtime.renderer_snapshot_bundle(renderer_id);

            // OpenHome est la source de vérité pour la queue - pas de fallback au runtime
            let snapshot = self.get_cached_openhome_playlist_snapshot(
                renderer_id,
                OPENHOME_SNAPSHOT_CACHE_TTL,
            )?;

            let queue_items: Vec<PlaybackItem> = snapshot
                .tracks
                .iter()
                .map(|track| playback_item_from_openhome_track(renderer_id, track))
                .collect();

            let queue_len = snapshot.tracks.len();

            // Pour OpenHome: current_index vient UNIQUEMENT d'OpenHome, pas d'heuristiques runtime
            let queue_current_index = snapshot.current_index.or_else(|| {
                snapshot.current_id.and_then(|id| {
                    snapshot.tracks.iter().position(|track| track.id == id)
                })
            });

            debug!(
                renderer = renderer_id.0.as_str(),
                current_id = ?snapshot.current_id,
                current_index = ?queue_current_index,
                track_count = snapshot.tracks.len(),
                "renderer_full_snapshot: OpenHome snapshot retrieved"
            );

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

            let (position_ms, duration_ms) =
                convert_runtime_position(runtime_snapshot.position.as_ref());
            let queue_current_metadata = queue_current_index
                .and_then(|idx| queue_items.get(idx))
                .map(current_track_from_playback_item);

            // MICRO-PATCH 5: Pour OpenHome, préférer les métadonnées depuis le snapshot OpenHome
            // car runtime_snapshot.last_metadata n'est jamais mis à jour pour OpenHome
            let current_track = queue_current_metadata.or_else(|| {
                runtime_snapshot
                    .last_metadata
                    .as_ref()
                    .map(|meta| CurrentTrackMetadata {
                        title: meta.title.clone(),
                        artist: meta.artist.clone(),
                        album: meta.album.clone(),
                        album_art_uri: meta.album_art_uri.clone(),
                    })
            });

            let state_view = RendererStateView {
                id: renderer_id.0.clone(),
                friendly_name: info.friendly_name.clone(),
                transport_state: runtime_snapshot
                    .state
                    .as_ref()
                    .map(|state| state.as_str().to_string())
                    .unwrap_or_else(|| "UNKNOWN".to_string()),
                position_ms,
                duration_ms,
                volume: runtime_snapshot
                    .last_volume
                    .and_then(|value| u8::try_from(value).ok()),
                mute: runtime_snapshot.last_mute,
                queue_len,
                attached_playlist: binding.clone(),
                current_track,
            };

            return Ok(FullRendererSnapshot {
                state: state_view,
                queue: queue_view,
                binding,
            });
        }

        // Chemin non-OpenHome (UPnP AV): continue d'utiliser le runtime
        let (runtime_snapshot, queue_items, mut queue_current_index) =
            self.runtime.renderer_snapshot_bundle(renderer_id);
        let playback_source = self.runtime.playback_source(renderer_id);
        let queue_len = queue_items.len();

        if queue_current_index.is_none() {
            if let Some(position) = runtime_snapshot.position.as_ref() {
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
        }

        if queue_current_index.is_none()
            && matches!(playback_source, PlaybackSource::FromQueue)
            && runtime_snapshot
                .state
                .as_ref()
                .map(|state| matches!(state, PlaybackState::Playing | PlaybackState::Paused))
                .unwrap_or(false)
            && !queue_items.is_empty()
        {
            queue_current_index = Some(0);
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

        let (position_ms, duration_ms) =
            convert_runtime_position(runtime_snapshot.position.as_ref());
        let queue_current_metadata = queue_current_index
            .and_then(|idx| queue_items.get(idx))
            .map(current_track_from_playback_item);

        let current_track = runtime_snapshot
            .last_metadata
            .as_ref()
            .map(|meta| CurrentTrackMetadata {
                title: meta.title.clone(),
                artist: meta.artist.clone(),
                album: meta.album.clone(),
                album_art_uri: meta.album_art_uri.clone(),
            })
            .or(queue_current_metadata);

        let state_view = RendererStateView {
            id: renderer_id.0.clone(),
            friendly_name: info.friendly_name.clone(),
            transport_state: runtime_snapshot
                .state
                .as_ref()
                .map(|state| state.as_str().to_string())
                .unwrap_or_else(|| "UNKNOWN".to_string()),
            position_ms,
            duration_ms,
            volume: runtime_snapshot
                .last_volume
                .and_then(|value| u8::try_from(value).ok()),
            mute: runtime_snapshot.last_mute,
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

    pub fn clear_openhome_playlist(&self, renderer_id: &RendererId) -> anyhow::Result<()> {
        let renderer = self.openhome_renderer(renderer_id)?;
        renderer.openhome_playlist_clear()?;
        self.sync_openhome_playlist_for(renderer_id)
    }

    pub fn add_openhome_track(
        &self,
        renderer_id: &RendererId,
        uri: &str,
        metadata: &str,
        after_id: Option<u32>,
        play: bool,
    ) -> anyhow::Result<()> {
        let renderer = self.openhome_renderer(renderer_id)?;
        renderer.openhome_playlist_add_track(uri, metadata, after_id, play)?;
        self.sync_openhome_playlist_for(renderer_id)
    }

    pub fn play_openhome_track_id(
        &self,
        renderer_id: &RendererId,
        track_id: u32,
    ) -> anyhow::Result<()> {
        let renderer = self.openhome_renderer(renderer_id)?;
        renderer.openhome_playlist_play_id(track_id)?;
        self.runtime
            .set_playback_source(renderer_id, PlaybackSource::FromQueue);
        self.sync_openhome_playlist_for(renderer_id)
    }

    /// Plays the current queue item without advancing the index.
    ///
    /// Useful after a Stop operation to resume playback from the same track.
    /// The method only reads queue content via the runtime helpers and
    /// delegates potential structural mutations to `QueueBackend` (when an item
    /// needs to be restored after a playback error).
    pub fn play_current_from_queue(&self, renderer_id: &RendererId) -> anyhow::Result<()> {
        if !self.runtime.has_entry(renderer_id) {
            let err = Self::runtime_entry_missing(renderer_id);
            warn!(
                renderer = renderer_id.0.as_str(),
                "Cannot play current: renderer not registered in runtime"
            );
            return Err(err);
        }

        if self.runtime.uses_openhome_playlist(renderer_id) {
            return self.play_current_openhome(renderer_id);
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

        let playback = (|| -> anyhow::Result<()> {
            let didl_metadata = playback_item_to_didl(&item);
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

                // Sauvegarder les métadonnées dans le snapshot pour que current_track soit disponible
                // même si le renderer UPnP ne retourne pas de métadonnées dans GetPositionInfo
                let metadata = playback_item_track_metadata(&item);
                self.runtime.update_snapshot_with(renderer_id, |snapshot| {
                    snapshot.last_metadata = Some(metadata);
                });

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

    /// Advances the queue by one item, starts playback and updates the snapshot.
    ///
    /// The structural mutation uses the `QueueBackend::dequeue_next` helper
    /// (through `RuntimeState`) so that all pointer updates are consistent.
    pub fn play_next_from_queue(&self, renderer_id: &RendererId) -> anyhow::Result<()> {
        if !self.runtime.has_entry(renderer_id) {
            let err = Self::runtime_entry_missing(renderer_id);
            warn!(
                renderer = renderer_id.0.as_str(),
                "Cannot advance queue: renderer not registered in runtime"
            );
            return Err(err);
        }

        if self.runtime.uses_openhome_playlist(renderer_id) {
            self.play_next_openhome(renderer_id)?;
            return Ok(());
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

        let playback = (|| -> anyhow::Result<()> {
            let didl_metadata = playback_item_to_didl(&item);
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
                .with_music_queue_mut(renderer_id, |queue| {
                    queue.enqueue_items(vec![item.clone()], EnqueueMode::InsertAfterCurrent)?;
                    Ok(())
                })
                .is_err()
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

        // Sauvegarder les métadonnées dans le snapshot pour que current_track soit disponible
        // même si le renderer UPnP ne retourne pas de métadonnées dans GetPositionInfo
        let metadata = playback_item_track_metadata(&item);
        self.runtime.update_snapshot_with(renderer_id, |snapshot| {
            snapshot.last_metadata = Some(metadata);
        });

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
                        let next_didl_metadata = playback_item_to_didl(next_item);
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
        let from_queue = self.runtime.is_playing_from_queue(renderer_id);

        debug!(
            renderer = renderer_id.0.as_str(),
            renderer_playing,
            from_queue,
            state = ?snapshot.state,
            "start_queue_playback_if_idle: checking if should start playback"
        );

        // Only skip if the renderer is actually playing
        // Don't skip just because playback_source is FromQueue - the renderer might have stopped
        if renderer_playing {
            debug!(
                renderer = renderer_id.0.as_str(),
                "start_queue_playback_if_idle: skipping because renderer is already playing"
            );
            return Ok(());
        }

        // Check if queue has ANY items (not just upcoming items after current)
        // This is important for newly attached playlists with current_index set
        let has_items = self
            .runtime
            .queue_full_snapshot(renderer_id)
            .map(|(items, _)| !items.is_empty())
            .unwrap_or(false);
        if !has_items {
            debug!(
                renderer = renderer_id.0.as_str(),
                "start_queue_playback_if_idle: queue is empty"
            );
            return Ok(());
        }

        if self.runtime.uses_openhome_playlist(renderer_id) {
            self.play_current_from_queue(renderer_id)
        } else {
            self.play_next_from_queue(renderer_id)
        }
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
        renderer_id: &RendererId,
        server_id: ServerId,
        container_id: String,
    ) -> anyhow::Result<()> {
        self.attach_queue_to_playlist_with_options(renderer_id, server_id, container_id, false)
    }

    /// Attach a renderer queue to a playlist with explicit `auto_play` behaviour.
    ///
    /// Same queue-mutation guarantees as [`attach_queue_to_playlist`].
    pub fn attach_queue_to_playlist_with_options(
        &self,
        renderer_id: &RendererId,
        server_id: ServerId,
        container_id: String,
        auto_play: bool,
    ) -> anyhow::Result<()> {
        self.attach_queue_to_playlist_internal(renderer_id, &server_id, &container_id, auto_play)
    }

    /// Internal implementation shared by every attach wrapper.
    fn attach_queue_to_playlist_internal(
        &self,
        renderer_id: &RendererId,
        server_id: &ServerId,
        container_id: &str,
        auto_play: bool,
    ) -> anyhow::Result<()> {
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

        // Clear the renderer's queue for OpenHome renderers
        // We also sync the local cache to reflect the empty state, which will trigger
        // refresh_attached_queue_for() to use replace_entire_playlist() instead of gentle sync
        if self.runtime.uses_openhome_playlist(renderer_id) {
            let renderer = self.openhome_renderer(renderer_id)?;
            renderer.openhome_playlist_clear()?;
            // Sync local cache to reflect the empty renderer state
            self.sync_openhome_playlist_for(renderer_id)?;
            debug!(
                renderer = renderer_id.0.as_str(),
                "Cleared OpenHome renderer playlist and synced local cache"
            );
        } else {
            // For non-OpenHome renderers, use the standard clear_queue
            self.clear_queue(renderer_id)?;
        }

        let binding = PlaylistBinding {
            server_id: server_id.clone(),
            container_id: container_id.to_string(),
            has_seen_update: false,
            pending_refresh: true,
            auto_play_on_refresh: auto_play,
        };

        {
            let mut bindings = self.playlist_bindings.lock().unwrap();
            bindings.insert(renderer_id.clone(), binding.clone());
            info!(
                renderer = renderer_id.0.as_str(),
                server = server_id.0.as_str(),
                container = container_id,
                auto_play,
                "Queue attached to playlist container"
            );
        }

        self.emit_renderer_event(RendererEvent::BindingChanged {
            id: renderer_id.clone(),
            binding: Some(binding),
        });

        let mut auto_start_cb = |rid: &RendererId| self.start_queue_playback_if_idle(rid);
        let callback: Option<&mut dyn FnMut(&RendererId) -> anyhow::Result<()>> = if auto_play {
            Some(&mut auto_start_cb)
        } else {
            None
        };

        refresh_attached_queue_for(
            &self.registry,
            &self.runtime,
            &self.playlist_bindings,
            renderer_id,
            &self.event_bus,
            callback,
        )
    }

    /// Detach a renderer's queue from its associated playlist container.
    ///
    /// Public mutation API paired with `attach_queue_to_playlist*`. After calling
    /// this, the queue will no longer be automatically refreshed from the server.
    /// If no binding existed, this is a no-op.
    pub fn detach_queue_playlist(&self, renderer_id: &RendererId) {
        self.detach_playlist_binding(renderer_id, "api_detach");
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

    /// Internal helper to detach any playlist binding and notify observers.
    ///
    /// Invariant: every user-driven queue mutation **must** call this method so
    /// that bindings never become out of sync with the local queue snapshot.
    fn detach_playlist_binding(&self, renderer_id: &RendererId, reason: &str) {
        let removed = {
            let mut bindings = self.playlist_bindings.lock().unwrap();
            bindings.remove(renderer_id)
        };

        if let Some(binding) = removed {
            info!(
                renderer = renderer_id.0.as_str(),
                server = binding.server_id.0.as_str(),
                container = binding.container_id.as_str(),
                reason = reason,
                "Playlist binding detached"
            );
            self.emit_renderer_event(RendererEvent::BindingChanged {
                id: renderer_id.clone(),
                binding: None,
            });
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

    fn openhome_renderer(&self, renderer_id: &RendererId) -> anyhow::Result<MusicRenderer> {
        let renderer = self
            .music_renderer_by_id(renderer_id)
            .ok_or_else(|| OpenHomeAccessError::RendererNotFound(renderer_id.0.clone()))?;
        if !renderer.info().capabilities.has_oh_playlist {
            return Err(OpenHomeAccessError::PlaylistNotSupported(renderer_id.0.clone()).into());
        }
        Ok(renderer)
    }

    fn sync_openhome_playlist_for(&self, renderer_id: &RendererId) -> anyhow::Result<()> {
        sync_openhome_playlist(&self.registry, &self.runtime, &self.event_bus, renderer_id)
    }

    fn enqueue_items_openhome(
        &self,
        renderer_id: &RendererId,
        items: Vec<PlaybackItem>,
    ) -> anyhow::Result<()> {
        if items.is_empty() {
            return Ok(());
        }

        let renderer = self.openhome_renderer(renderer_id)?;

        // Get the last track ID from the OpenHome native playlist
        // This ensures we append to the end of the actual playlist, not just our local queue
        let mut after_id = renderer
            .openhome_playlist_ids()
            .ok()
            .and_then(|ids| ids.last().copied());

        for item in items.iter() {
            let metadata = playback_item_to_didl(item);
            debug!(
                uri = item.uri.as_str(),
                protocol_info = item.protocol_info.as_str(),
                metadata_len = metadata.len(),
                "Inserting track to OpenHome playlist"
            );
            trace!(metadata = metadata.as_str(), "DIDL-Lite metadata");
            after_id =
                Some(renderer.openhome_playlist_add_track(&item.uri, &metadata, after_id, false)?);
        }

        self.sync_openhome_playlist_for(renderer_id)?;
        Ok(())
    }

    fn play_current_openhome(&self, renderer_id: &RendererId) -> anyhow::Result<()> {
        let renderer = self.openhome_renderer(renderer_id)?;

        // MICRO-PATCH 5: Pour OpenHome, récupérer les données directement depuis la playlist native
        // au lieu du miroir local (qui peut être vide ou obsolète)
        match self.get_cached_openhome_playlist_snapshot(
            renderer_id,
            OPENHOME_SNAPSHOT_CACHE_TTL,
        ) {
            Ok(snapshot) => {
                if snapshot.tracks.is_empty() {
                    debug!(
                        renderer = renderer_id.0.as_str(),
                        "OpenHome playlist is empty"
                    );
                    self.runtime
                        .set_playback_source(renderer_id, PlaybackSource::None);
                    return Ok(());
                }

                // Trouver le track_id courant depuis le snapshot OpenHome
                let target_track_id = if let Some(current_id) = snapshot.current_id {
                    Some(current_id)
                } else if let Some(current_idx) = snapshot.current_index {
                    snapshot.tracks.get(current_idx).map(|track| track.id)
                } else {
                    snapshot.tracks.first().map(|track| track.id)
                };

                if let Some(track_id) = target_track_id {
                    match renderer.openhome_playlist_play_id(track_id) {
                        Ok(()) => {
                            self.runtime
                                .set_playback_source(renderer_id, PlaybackSource::FromQueue);
                            self.sync_openhome_playlist_for(renderer_id)?;
                            info!(
                                renderer = renderer_id.0.as_str(),
                                track_id,
                                playlist_len = snapshot.tracks.len(),
                                "Started OpenHome playlist playback (current item)"
                            );
                            return Ok(());
                        }
                        Err(err) => {
                            warn!(
                                renderer = renderer_id.0.as_str(),
                                track_id,
                                error = %err,
                                "PlayId failed, falling back to Play()"
                            );
                        }
                    }
                }

                // Fallback: appeler Play() sans spécifier de track_id
                renderer.play()?;
                self.runtime
                    .set_playback_source(renderer_id, PlaybackSource::FromQueue);
                self.sync_openhome_playlist_for(renderer_id)?;
                info!(
                    renderer = renderer_id.0.as_str(),
                    playlist_len = snapshot.tracks.len(),
                    "Started OpenHome native playlist playback"
                );
                return Ok(());
            }
            Err(err) => {
                warn!(
                    renderer = renderer_id.0.as_str(),
                    error = %err,
                    "Failed to fetch OpenHome playlist snapshot, playlist might be empty"
                );
                debug!(
                    renderer = renderer_id.0.as_str(),
                    "OpenHome playlist is empty or unavailable"
                );
                self.runtime
                    .set_playback_source(renderer_id, PlaybackSource::None);
                return Ok(());
            }
        }
    }

    fn play_next_openhome(&self, renderer_id: &RendererId) -> anyhow::Result<()> {
        let renderer = self.openhome_renderer(renderer_id)?;

        // MICRO-PATCH 5: Récupérer les données directement depuis OpenHome au lieu du miroir local
        let snapshot = self.get_cached_openhome_playlist_snapshot(
            renderer_id,
            OPENHOME_SNAPSHOT_CACHE_TTL,
        )?;

        if snapshot.tracks.is_empty() {
            debug!(
                renderer = renderer_id.0.as_str(),
                "OpenHome playlist is empty, cannot play next"
            );
            self.runtime
                .set_playback_source(renderer_id, PlaybackSource::None);
            return Ok(());
        }

        // Déterminer le prochain track_id
        let next_track_id = match snapshot.current_index {
            Some(idx) => {
                // Prendre la piste suivante si elle existe
                snapshot
                    .tracks
                    .get(idx + 1)
                    .map(|track| track.id)
                    .or_else(|| snapshot.tracks.first().map(|track| track.id))
            }
            None => snapshot.tracks.first().map(|track| track.id),
        };

        let Some(track_id) = next_track_id else {
            debug!(
                renderer = renderer_id.0.as_str(),
                "No OpenHome track available to advance to"
            );
            self.runtime
                .set_playback_source(renderer_id, PlaybackSource::None);
            return Ok(());
        };

        renderer.openhome_playlist_play_id(track_id)?;
        self.runtime
            .set_playback_source(renderer_id, PlaybackSource::FromQueue);
        self.sync_openhome_playlist_for(renderer_id)?;
        info!(
            renderer = renderer_id.0.as_str(),
            track_id, "Advanced OpenHome playlist to next track"
        );
        Ok(())
    }
}

#[cfg(feature = "pmoserver")]
fn convert_runtime_position(position: Option<&PlaybackPositionInfo>) -> (Option<u64>, Option<u64>) {
    match position {
        Some(info) => (
            parse_hms_to_ms(info.rel_time.as_deref()),
            parse_hms_to_ms(info.track_duration.as_deref()),
        ),
        None => (None, None),
    }
}

fn build_openhome_queue(info: &RendererInfo) -> Option<OpenHomeQueue> {
    let playlist = build_playlist_client(info)?;
    let info_client = build_info_client(info);
    let product_client = build_product_client(info);
    Some(OpenHomeQueue::new(
        info.id.clone(),
        playlist,
        info_client,
        product_client,
    ))
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PlaylistBackend {
    PMOQueue,
    OpenHome,
}

struct RendererRuntimeEntry {
    snapshot: RendererRuntimeSnapshot,
    pub queue: crate::control_point::music_queue::MusicQueue,
    openhome_cache: OpenHomePlaylistCache,
    playback_source: PlaybackSource,
    user_stop_requested: bool,
    playlist_backend: PlaylistBackend,
}

impl Default for RendererRuntimeEntry {
    fn default() -> Self {
        Self {
            snapshot: RendererRuntimeSnapshot::default(),
            queue: MusicQueue::Internal(InternalQueue::new()),
            openhome_cache: OpenHomePlaylistCache::default(),
            playback_source: PlaybackSource::None,
            user_stop_requested: false,
            playlist_backend: PlaylistBackend::PMOQueue,
        }
    }
}

#[derive(Debug, Default, Clone)]
struct OpenHomePlaylistCache {
    ids: Option<Vec<u32>>,
    snapshot: Option<OpenHomePlaylistSnapshot>,
    last_refresh: Option<Instant>,
}

struct RuntimeState {
    entries: Mutex<HashMap<RendererId, RendererRuntimeEntry>>,
}

pub struct RendererRuntimeStateMut<'a> {
    pub queue: MusicQueueGuard<'a>,
    _guard: MutexGuard<'a, HashMap<RendererId, RendererRuntimeEntry>>,
}

impl<'a> RendererRuntimeStateMut<'a> {
    fn new(
        guard: MutexGuard<'a, HashMap<RendererId, RendererRuntimeEntry>>,
        queue_ptr: *mut MusicQueue,
    ) -> Self {
        Self {
            queue: MusicQueueGuard::new(queue_ptr),
            _guard: guard,
        }
    }
}

pub struct MusicQueueGuard<'a> {
    ptr: *mut MusicQueue,
    _marker: PhantomData<&'a mut MusicQueue>,
}

impl<'a> MusicQueueGuard<'a> {
    fn new(ptr: *mut MusicQueue) -> Self {
        Self {
            ptr,
            _marker: PhantomData,
        }
    }
}

impl<'a> Deref for MusicQueueGuard<'a> {
    type Target = MusicQueue;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.ptr }
    }
}

impl<'a> DerefMut for MusicQueueGuard<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.ptr }
    }
}

struct RuntimeOpenHomeQueueProvider {
    runtime: Arc<RuntimeState>,
}

impl OpenHomeQueueProvider for RuntimeOpenHomeQueueProvider {
    fn renderer_state(&self, renderer_id: &RendererId) -> anyhow::Result<RendererRuntimeState> {
        self.runtime.renderer_state(renderer_id)
    }

    fn renderer_state_mut<'a>(
        &'a self,
        renderer_id: &RendererId,
    ) -> anyhow::Result<RendererRuntimeStateMut<'a>> {
        self.runtime.renderer_state_mut(renderer_id)
    }

    fn invalidate_openhome_cache(&self, renderer_id: &RendererId) -> anyhow::Result<()> {
        self.runtime.invalidate_openhome_cache(renderer_id);
        Ok(())
    }
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

    fn update_snapshot_with<F>(&self, id: &RendererId, f: F)
    where
        F: FnOnce(&mut RendererRuntimeSnapshot),
    {
        self.with_entry(id, |entry| {
            f(&mut entry.snapshot);
        });
    }

    fn has_entry(&self, id: &RendererId) -> bool {
        let entries = self.entries.lock().unwrap();
        entries.contains_key(id)
    }

    fn with_music_queue_mut<F, R>(&self, id: &RendererId, f: F) -> anyhow::Result<R>
    where
        F: FnOnce(&mut MusicQueue) -> anyhow::Result<R>,
    {
        let mut entries = self.entries.lock().unwrap();
        let entry = entries
            .get_mut(id)
            .ok_or_else(|| anyhow!("Renderer {} not registered in runtime", id.0))?;
        f(&mut entry.queue)
    }

    fn queue_snapshot(&self, id: &RendererId) -> Option<Vec<PlaybackItem>> {
        let entries = self.entries.lock().unwrap();
        entries
            .get(id)
            .and_then(|entry| entry.queue.upcoming_items().ok())
    }

    fn queue_full_snapshot(&self, id: &RendererId) -> Option<(Vec<PlaybackItem>, Option<usize>)> {
        let entries = self.entries.lock().unwrap();
        entries.get(id).and_then(|entry| {
            entry
                .queue
                .queue_snapshot()
                .ok()
                .map(|snapshot| (snapshot.items, snapshot.current_index))
        })
    }

    fn renderer_state(&self, id: &RendererId) -> anyhow::Result<RendererRuntimeState> {
        let entries = self.entries.lock().unwrap();
        let entry = entries
            .get(id)
            .ok_or_else(|| anyhow!("Renderer {} not registered in runtime", id.0))?;
        Ok(RendererRuntimeState {
            queue: entry.queue.clone(),
        })
    }

    fn renderer_state_mut(&self, id: &RendererId) -> anyhow::Result<RendererRuntimeStateMut<'_>> {
        let mut entries = self.entries.lock().unwrap();
        let queue_ptr = {
            let entry = entries
                .get_mut(id)
                .ok_or_else(|| anyhow!("Renderer {} not registered in runtime", id.0))?;
            let entry_ptr: *mut RendererRuntimeEntry = entry;
            unsafe { &mut (*entry_ptr).queue as *mut MusicQueue }
        };
        Ok(RendererRuntimeStateMut::new(entries, queue_ptr))
    }

    fn current_track_metadata(&self, id: &RendererId) -> Option<TrackMetadata> {
        let entries = self.entries.lock().unwrap();
        entries
            .get(id)
            .and_then(|entry| entry.snapshot.last_metadata.clone())
    }

    #[cfg(feature = "pmoserver")]
    fn renderer_snapshot_bundle(
        &self,
        id: &RendererId,
    ) -> (RendererRuntimeSnapshot, Vec<PlaybackItem>, Option<usize>) {
        let entries = self.entries.lock().unwrap();
        if let Some(entry) = entries.get(id) {
            match entry.queue.queue_snapshot() {
                Ok(snapshot) => (
                    entry.snapshot.clone(),
                    snapshot.items,
                    snapshot.current_index,
                ),
                Err(err) => {
                    warn!(
                        renderer = id.0.as_str(),
                        error = %err,
                        "Failed to build queue snapshot for renderer"
                    );
                    (entry.snapshot.clone(), Vec::new(), None)
                }
            }
        } else {
            (RendererRuntimeSnapshot::default(), Vec::new(), None)
        }
    }

    fn dequeue_next(&self, id: &RendererId) -> Option<(PlaybackItem, usize)> {
        let mut entries = self.entries.lock().unwrap();
        let entry = entries.get_mut(id)?;
        entry.queue.dequeue_next().ok().flatten()
    }

    fn peek_current(&self, id: &RendererId) -> Option<(PlaybackItem, usize)> {
        let entries = self.entries.lock().unwrap();
        let entry = entries.get(id)?;
        entry.queue.peek_current().ok().flatten()
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

    fn set_music_queue(&self, id: &RendererId, queue: MusicQueue) {
        self.with_entry(id, |entry| {
            entry.queue = queue;
        });
    }

    fn invalidate_openhome_cache(&self, id: &RendererId) {
        self.with_entry(id, |entry| {
            entry.openhome_cache = OpenHomePlaylistCache::default();
        });
    }

    fn get_openhome_cache(&self, id: &RendererId) -> Option<OpenHomePlaylistCache> {
        let entries = self.entries.lock().unwrap();
        entries.get(id).map(|entry| entry.openhome_cache.clone())
    }

    fn set_openhome_cache(&self, id: &RendererId, cache: OpenHomePlaylistCache) {
        self.with_entry(id, |entry| {
            entry.openhome_cache = cache;
        });
    }

    fn openhome_snapshot_cached(
        &self,
        renderer: &MusicRenderer,
        ttl: Duration,
    ) -> anyhow::Result<OpenHomePlaylistSnapshot> {
        let ids = renderer.openhome_playlist_ids()?;
        let now = Instant::now();

        if let Some(cache) = self.get_openhome_cache(renderer.id()) {
            if let (Some(cached_ids), Some(snapshot), Some(last_refresh)) =
                (cache.ids, cache.snapshot, cache.last_refresh)
            {
                if cached_ids == ids && now.saturating_duration_since(last_refresh) <= ttl {
                    return Ok(snapshot);
                }
            }
        }

        let snapshot = renderer.fetch_openhome_playlist_snapshot()?;
        let cache = OpenHomePlaylistCache {
            ids: Some(ids),
            snapshot: Some(snapshot.clone()),
            last_refresh: Some(now),
        };
        self.set_openhome_cache(renderer.id(), cache);
        Ok(snapshot)
    }

    fn set_playlist_backend(&self, id: &RendererId, backend: PlaylistBackend) {
        self.with_entry(id, |entry| {
            entry.playlist_backend = backend;
        });
    }

    fn playlist_backend(&self, id: &RendererId) -> PlaylistBackend {
        let entries = self.entries.lock().unwrap();
        entries
            .get(id)
            .map(|entry| entry.playlist_backend)
            .unwrap_or(PlaylistBackend::PMOQueue)
    }

    fn uses_openhome_playlist(&self, id: &RendererId) -> bool {
        matches!(self.playlist_backend(id), PlaylistBackend::OpenHome)
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
        runtime.with_music_queue_mut(renderer_id, |queue| queue.clear_queue())?;
        runtime.invalidate_openhome_cache(renderer_id);

        // Emit QueueUpdated event
        event_bus.broadcast(RendererEvent::QueueUpdated {
            id: renderer_id.clone(),
            queue_length: 0,
        });

        return Ok(());
    }

    // Step 5: GENTLE SYNCHRONIZATION
    // Use incremental replace_queue() instead of replace_with_attached_playlist()
    // This uses LCS algorithm to minimize playlist operations and avoid interrupting playback

    // Get the full queue snapshot to access the item currently being played
    let (full_queue, current_idx) = runtime
        .queue_full_snapshot(renderer_id)
        .unwrap_or((vec![], None));

    // Get the item currently being played (at current_index), not the next one in queue
    let current_item = current_idx.and_then(|idx| full_queue.get(idx).cloned());

    // Check if renderer is playing - if so, we MUST preserve the current track
    // We use multiple signals to determine playback state:
    // 1. Direct playback_state() query (may fail on some renderers like upmpdcli)
    // 2. Presence of current_idx (if we have a current track, likely playing)
    let is_playing = {
        let renderer_info = {
            let reg = registry.read().unwrap();
            reg.get_renderer(renderer_id)
        };

        if let Some(info) = renderer_info {
            if let Some(renderer) = MusicRenderer::from_registry_info(info, registry) {
                // Try direct query first
                if matches!(renderer.playback_state(), Ok(PlaybackState::Playing)) {
                    true
                } else if current_idx.is_some() && !full_queue.is_empty() {
                    // Fallback: if we have a current index and non-empty queue,
                    // assume playback is happening (handles renderers where playback_state() fails)
                    debug!(
                        renderer = renderer_id.0.as_str(),
                        "playback_state() failed or not Playing, but current_idx is set - assuming playback"
                    );
                    true
                } else {
                    false
                }
            } else {
                false
            }
        } else {
            false
        }
    };

    let item_found_at = current_item.as_ref().and_then(|current| {
        let current_uid = current.unique_id();
        new_items
            .iter()
            .position(|new_item| new_item.unique_id() == current_uid)
            .or_else(|| {
                new_items
                    .iter()
                    .position(|new_item| new_item.uri == current.uri)
            })
    });

    let final_queue_len = runtime.with_music_queue_mut(renderer_id, |queue| {
        if let Some(idx) = item_found_at {
            // Current item is in new playlist - use gentle incremental update
            queue.replace_queue(new_items.clone(), Some(idx))?;
            info!(
                renderer = renderer_id.0.as_str(),
                server = server_id.0.as_str(),
                container = container_id.as_str(),
                total_items = new_items.len(),
                current_index = idx,
                upcoming = new_items.len().saturating_sub(idx + 1),
                current_preserved = true,
                is_playing,
                "Gentle refresh: current item found in new playlist"
            );
            Ok(new_items.len())
        } else if let Some(ref current) = current_item {
            // Current item NOT in new playlist
            if is_playing {
                // CRITICAL: Preserve current playing track by inserting it at the beginning
                let mut combined = Vec::with_capacity(new_items.len() + 1);
                combined.push(current.clone());
                combined.extend(new_items.clone());
                queue.replace_queue(combined, Some(0))?;
                info!(
                    renderer = renderer_id.0.as_str(),
                    server = server_id.0.as_str(),
                    container = container_id.as_str(),
                    total_items = new_items.len() + 1,
                    current_index = 0,
                    upcoming = new_items.len(),
                    current_preserved = true,
                    current_reinserted = true,
                    is_playing = true,
                    "Gentle refresh: preserved playing track not in new playlist"
                );
                Ok(new_items.len() + 1)
            } else {
                // Not playing, use new playlist as-is
                queue.replace_queue(new_items.clone(), None)?;
                info!(
                    renderer = renderer_id.0.as_str(),
                    server = server_id.0.as_str(),
                    container = container_id.as_str(),
                    total_items = new_items.len(),
                    current_preserved = false,
                    is_playing = false,
                    "Gentle refresh: replaced queue (not playing)"
                );
                Ok(new_items.len())
            }
        } else {
            // No current item, use new playlist as-is
            queue.replace_queue(new_items.clone(), None)?;
            info!(
                renderer = renderer_id.0.as_str(),
                server = server_id.0.as_str(),
                container = container_id.as_str(),
                total_items = new_items.len(),
                current_preserved = false,
                is_playing,
                "Gentle refresh: no current item"
            );
            Ok(new_items.len())
        }
    })?;
    runtime.invalidate_openhome_cache(renderer_id);

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
    let resource = entry.resources.iter().find(|res| res.is_audio())?;

    let metadata = TrackMetadata {
        title: Some(entry.title.clone()),
        artist: entry.artist.clone(),
        album: entry.album.clone(),
        genre: entry.genre.clone(),
        album_art_uri: entry.album_art_uri.clone(),
        date: entry.date.clone(),
        track_number: entry.track_number.clone(),
        creator: entry.creator.clone(),
    };

    Some(PlaybackItem {
        media_server_id: server.id().clone(),
        didl_id: entry.id.clone(),
        uri: resource.uri.clone(),
        protocol_info: resource.protocol_info.clone(),
        metadata: Some(metadata),
    })
}

const OPENHOME_TRACK_PREFIX: &str = "openhome:";

fn playback_item_from_openhome_track(
    renderer_id: &RendererId,
    track: &OpenHomePlaylistTrack,
) -> PlaybackItem {
    let metadata = TrackMetadata {
        title: track.title.clone(),
        artist: track.artist.clone(),
        album: track.album.clone(),
        genre: None,
        album_art_uri: track.album_art_uri.clone(),
        date: None,
        track_number: None,
        creator: None,
    };

    PlaybackItem {
        media_server_id: ServerId(format!("openhome:{}", renderer_id.0)),
        didl_id: format!("{}{}", OPENHOME_TRACK_PREFIX, track.id),
        uri: track.uri.clone(),
        // OpenHome tracks don't provide protocolInfo, use generic default
        protocol_info: "http-get:*:audio/*:*".to_string(),
        metadata: Some(metadata),
    }
}

fn openhome_track_id_from_item(item: &PlaybackItem) -> Option<u32> {
    let raw = item.didl_id.strip_prefix(OPENHOME_TRACK_PREFIX)?;
    raw.parse::<u32>().ok()
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

fn openhome_renderer_from_registry(
    registry: &Arc<RwLock<DeviceRegistry>>,
    renderer_id: &RendererId,
) -> anyhow::Result<MusicRenderer> {
    let info = {
        let reg = registry.read().unwrap();
        reg.get_renderer(renderer_id)
            .ok_or_else(|| OpenHomeAccessError::RendererNotFound(renderer_id.0.clone()))?
    };
    let renderer = MusicRenderer::from_registry_info(info, registry)
        .and_then(|r| match r {
            MusicRenderer::OpenHome(_) => Some(r),
            _ => None,
        })
        .ok_or_else(|| OpenHomeAccessError::PlaylistNotSupported(renderer_id.0.clone()))?;
    Ok(renderer)
}

fn sync_openhome_playlist(
    registry: &Arc<RwLock<DeviceRegistry>>,
    runtime: &Arc<RuntimeState>,
    event_bus: &RendererEventBus,
    renderer_id: &RendererId,
) -> anyhow::Result<()> {
    let renderer = openhome_renderer_from_registry(registry, renderer_id)?;

    let snapshot = renderer.fetch_openhome_playlist_snapshot()?;
    let playback_items: Vec<PlaybackItem> = snapshot
        .tracks
        .iter()
        .map(|track| playback_item_from_openhome_track(renderer_id, track))
        .collect();

    debug!(
        renderer = renderer_id.0.as_str(),
        fetched_items = playback_items.len(),
        current_id = ?snapshot.current_id,
        "sync_openhome_playlist: fetched snapshot from renderer"
    );

    let current_id = snapshot.current_id;

    let current_index = current_id.and_then(|id| {
        playback_items
            .iter()
            .position(|item| openhome_track_id_from_item(item) == Some(id))
    });

    let queue_items = playback_items;
    let queue_len = runtime
        .with_music_queue_mut(renderer_id, move |queue| {
            let len = queue_items.len();
            queue.replace_queue(queue_items, current_index)?;
            Ok(len)
        })
        .unwrap_or(0);

    debug!(
        renderer = renderer_id.0.as_str(),
        queue_len,
        "sync_openhome_playlist: updated local queue"
    );

    event_bus.broadcast(RendererEvent::QueueUpdated {
        id: renderer_id.clone(),
        queue_length: queue_len,
    });

    Ok(())
}

const OPENHOME_SUBSCRIPTION_TIMEOUT_SECS: u64 = 300;
const OPENHOME_RENEWAL_MARGIN_SECS: u64 = 60;

fn spawn_openhome_event_runtime(
    registry: Arc<RwLock<DeviceRegistry>>,
    runtime: Arc<RuntimeState>,
    event_bus: RendererEventBus,
    event_tx: Sender<RendererEvent>,
    playlist_bindings: Arc<Mutex<HashMap<RendererId, PlaylistBinding>>>,
) -> io::Result<()> {
    let listener = TcpListener::bind("0.0.0.0:0")?;
    let listener_addr = listener
        .local_addr()
        .map_err(|err| io::Error::new(io::ErrorKind::Other, err))?;

    info!("OpenHome event listener bound on {}", listener_addr);

    let (notify_tx, notify_rx) = unbounded::<OpenHomeIncomingNotify>();
    thread::Builder::new()
        .name("openhome-event-http".into())
        .spawn(move || run_openhome_http_listener(listener, notify_tx))?;

    let worker = OpenHomeEventRuntime::new(
        registry,
        runtime,
        event_bus,
        notify_rx,
        event_tx,
        listener_addr.port(),
        playlist_bindings,
    );

    thread::Builder::new()
        .name("openhome-event-worker".into())
        .spawn(move || worker.run())
        .map(|_| ())
}

struct OpenHomeEventRuntime {
    registry: Arc<RwLock<DeviceRegistry>>,
    runtime: Arc<RuntimeState>,
    event_bus: RendererEventBus,
    notify_rx: Receiver<OpenHomeIncomingNotify>,
    event_tx: Sender<RendererEvent>,
    listener_port: u16,
    http_timeout: Duration,
    subscriptions: HashMap<OpenHomeSubscriptionKey, OpenHomeSubscriptionState>,
    path_index: HashMap<String, OpenHomeSubscriptionKey>,
    playlist_bindings: Arc<Mutex<HashMap<RendererId, PlaylistBinding>>>,
}

impl OpenHomeEventRuntime {
    fn new(
        registry: Arc<RwLock<DeviceRegistry>>,
        runtime: Arc<RuntimeState>,
        event_bus: RendererEventBus,
        notify_rx: Receiver<OpenHomeIncomingNotify>,
        event_tx: Sender<RendererEvent>,
        listener_port: u16,
        playlist_bindings: Arc<Mutex<HashMap<RendererId, PlaylistBinding>>>,
    ) -> Self {
        Self {
            registry,
            runtime,
            event_bus,
            notify_rx,
            event_tx,
            listener_port,
            http_timeout: Duration::from_secs(5),
            subscriptions: HashMap::new(),
            path_index: HashMap::new(),
            playlist_bindings,
        }
    }

    fn run(mut self) {
        loop {
            self.drain_notifications();
            self.refresh_renderers();
            self.renew_expiring();
            thread::sleep(Duration::from_millis(250));
        }
    }

    fn drain_notifications(&mut self) {
        while let Ok(notify) = self.notify_rx.try_recv() {
            self.handle_notification(notify);
        }
    }

    fn refresh_renderers(&mut self) {
        let renderer_infos = {
            let reg = self.registry.read().unwrap();
            reg.list_renderers()
        };

        let mut active: HashSet<OpenHomeSubscriptionKey> = HashSet::new();

        for info in renderer_infos {
            if !info.online {
                continue;
            }

            if let Some(url) = info.oh_playlist_event_sub_url.clone() {
                let key = OpenHomeSubscriptionKey::new(&info.id, OhServiceKind::Playlist);
                active.insert(key.clone());
                self.ensure_subscription(key, info.clone(), url);
            }

            if let Some(url) = info.oh_info_event_sub_url.clone() {
                let key = OpenHomeSubscriptionKey::new(&info.id, OhServiceKind::Info);
                active.insert(key.clone());
                self.ensure_subscription(key, info.clone(), url);
            }

            if let Some(url) = info.oh_time_event_sub_url.clone() {
                let key = OpenHomeSubscriptionKey::new(&info.id, OhServiceKind::Time);
                active.insert(key.clone());
                self.ensure_subscription(key, info.clone(), url);
            }
        }

        let stale: Vec<OpenHomeSubscriptionKey> = self
            .subscriptions
            .keys()
            .filter(|key| !active.contains(*key))
            .cloned()
            .collect();

        for key in stale {
            if let Some(mut entry) = self.subscriptions.remove(&key) {
                self.path_index.remove(&entry.callback_path);
                if let Err(err) = Self::unsubscribe_entry(self.http_timeout, &mut entry) {
                    warn!(
                        renderer = entry.renderer.friendly_name.as_str(),
                        service = entry.service.as_str(),
                        error = %err,
                        "Failed to unsubscribe from OpenHome events"
                    );
                }
            }
        }
    }

    fn ensure_subscription(
        &mut self,
        key: OpenHomeSubscriptionKey,
        info: RendererInfo,
        event_url: String,
    ) {
        let entry = self.subscriptions.entry(key.clone()).or_insert_with(|| {
            OpenHomeSubscriptionState::new(info.clone(), key.service, event_url.clone())
        });

        entry.update(info, event_url);
        self.path_index
            .insert(entry.callback_path.clone(), key.clone());

        if entry.sid.is_none() && entry.should_retry() {
            if let Err(err) = Self::subscribe_entry(self.listener_port, self.http_timeout, entry) {
                warn!(
                    renderer = entry.renderer.friendly_name.as_str(),
                    service = entry.service.as_str(),
                    error = %err,
                    "OpenHome SUBSCRIBE failed"
                );
                entry.defer_retry();
            }
        }
    }

    fn renew_expiring(&mut self) {
        let now = Instant::now();
        let mut to_renew = Vec::new();
        for (key, entry) in self.subscriptions.iter() {
            if let Some(exp) = entry.expires_at {
                if exp <= now + Duration::from_secs(OPENHOME_RENEWAL_MARGIN_SECS) {
                    to_renew.push(key.clone());
                }
            }
        }

        for key in to_renew {
            if let Some(entry) = self.subscriptions.get_mut(&key) {
                if let Err(err) = Self::renew_entry(self.http_timeout, entry) {
                    warn!(
                        renderer = entry.renderer.friendly_name.as_str(),
                        service = entry.service.as_str(),
                        error = %err,
                        "Failed to renew OpenHome subscription"
                    );
                    entry.reset_subscription();
                }
            }
        }
    }

    fn handle_notification(&mut self, notify: OpenHomeIncomingNotify) {
        let Some(key) = self.path_index.get(&notify.path).cloned() else {
            debug!("Dropping OpenHome notify for unknown path {}", notify.path);
            return;
        };

        let Some(entry) = self.subscriptions.get(&key) else {
            return;
        };

        if let (Some(expected), Some(received)) = (&entry.sid, &notify.sid) {
            if !expected.eq_ignore_ascii_case(received) {
                debug!(
                    renderer = entry.renderer.friendly_name.as_str(),
                    service = entry.service.as_str(),
                    expected_sid = expected.as_str(),
                    received_sid = received.as_str(),
                    "Ignoring OpenHome notify with mismatched SID"
                );
                return;
            }
        }

        let properties =
            parse_openhome_propertyset(&entry.renderer.id, &entry.service, &notify.body);
        if properties.is_empty() {
            return;
        }

        match entry.service {
            OhServiceKind::Playlist => {
                if properties
                    .iter()
                    .any(|(name, _)| is_id_array_property(name))
                {
                    if self.runtime.uses_openhome_playlist(&entry.renderer.id) {
                        // Check if this renderer has an active playlist binding
                        // If it does, skip sync because refresh_attached_queue_for() handles it
                        let has_active_binding = {
                            let bindings = self.playlist_bindings.lock().unwrap();
                            bindings.contains_key(&entry.renderer.id)
                        };

                        if !has_active_binding {
                            // Synchronize the local queue with the renderer's playlist state
                            // This ensures our local mirror stays in sync when the renderer
                            // playlist changes from other control points or manual edits
                            if let Err(err) = sync_openhome_playlist(
                                &self.registry,
                                &self.runtime,
                                &self.event_bus,
                                &entry.renderer.id,
                            ) {
                                warn!(
                                    renderer = entry.renderer.friendly_name.as_str(),
                                    error = %err,
                                    "Failed to sync OpenHome playlist after IdArray event"
                                );
                            }
                        }
                    }
                }
            }
            OhServiceKind::Info => {
                self.handle_info_properties(&entry.renderer.id, properties);
            }
            OhServiceKind::Time => {
                self.handle_time_properties(&entry.renderer.id, properties);
            }
            OhServiceKind::Volume | OhServiceKind::Product => {}
        }
    }

    fn handle_info_properties(&self, renderer_id: &RendererId, properties: Vec<(String, String)>) {
        let mut metadata_xml: Option<String> = None;
        let mut transport_state: Option<String> = None;
        let mut track_id: Option<u32> = None;
        let mut track_uri: Option<String> = None;

        for (name, value) in properties {
            match name.as_str() {
                "Metadata" | "TrackMetadata" => {
                    if !value.trim().is_empty() {
                        metadata_xml = Some(value);
                    }
                }
                "TransportState" => {
                    transport_state = Some(value);
                }
                "Id" | "TrackId" => {
                    if let Ok(id) = value.trim().parse::<u32>() {
                        track_id = Some(id);
                    }
                }
                "Uri" | "TrackUri" => {
                    if !value.trim().is_empty() {
                        track_uri = Some(value);
                    }
                }
                _ => {}
            }
        }

        if let Some(xml) = metadata_xml {
            if let Some(metadata) = parse_track_metadata_from_didl(&xml) {
                self.runtime.update_snapshot_with(renderer_id, |snapshot| {
                    snapshot.last_metadata = Some(metadata.clone());
                    let mut position = snapshot
                        .position
                        .clone()
                        .unwrap_or_else(|| empty_playback_position());
                    position.track_metadata = Some(xml.clone());
                    snapshot.position = Some(position);
                });
                let _ = self.event_tx.send(RendererEvent::MetadataChanged {
                    id: renderer_id.clone(),
                    metadata,
                });
            }
        }

        if track_id.is_some() || track_uri.is_some() {
            self.runtime.update_snapshot_with(renderer_id, |snapshot| {
                let mut position = snapshot
                    .position
                    .clone()
                    .unwrap_or_else(|| empty_playback_position());
                if let Some(id) = track_id {
                    position.track = Some(id);
                }
                if let Some(uri) = track_uri.clone() {
                    position.track_uri = Some(uri);
                }
                snapshot.position = Some(position);
            });
        }

        if let Some(state_str) = transport_state {
            let playback_state = map_openhome_state(&state_str);
            let _ = self.event_tx.send(RendererEvent::StateChanged {
                id: renderer_id.clone(),
                state: playback_state,
            });
        }
    }

    fn handle_time_properties(&self, renderer_id: &RendererId, properties: Vec<(String, String)>) {
        let mut duration: Option<u32> = None;
        let mut seconds: Option<u32> = None;

        for (name, value) in properties {
            match name.as_str() {
                "Duration" => {
                    duration = value.trim().parse::<u32>().ok();
                }
                "Seconds" => {
                    seconds = value.trim().parse::<u32>().ok();
                }
                _ => {}
            }
        }

        if duration.is_none() && seconds.is_none() {
            return;
        }

        let position = self.runtime.snapshot_for(renderer_id).position;
        let mut new_position = position.unwrap_or_else(|| empty_playback_position());
        if let Some(d) = duration {
            new_position.track_duration = Some(format_seconds(d));
        }
        if let Some(s) = seconds {
            new_position.rel_time = Some(format_seconds(s));
        }

        self.runtime.update_snapshot_with(renderer_id, |snapshot| {
            snapshot.position = Some(new_position.clone());
        });

        let _ = self.event_tx.send(RendererEvent::PositionChanged {
            id: renderer_id.clone(),
            position: new_position,
        });
    }

    fn subscribe_entry(
        listener_port: u16,
        http_timeout: Duration,
        entry: &mut OpenHomeSubscriptionState,
    ) -> anyhow::Result<()> {
        let event_url = entry.event_sub_url.clone();
        let (remote_host, remote_port) =
            parse_host_port(&event_url).context("Cannot extract host for SUBSCRIBE")?;
        let local_ip = determine_local_ip(&remote_host, remote_port)
            .context("Cannot determine local IP for callback")?;

        let callback_url = format!(
            "http://{}:{}{}",
            format_ip(&local_ip),
            listener_port,
            entry.callback_path
        );

        debug!(
            renderer = entry.renderer.friendly_name.as_str(),
            service = entry.service.as_str(),
            callback = callback_url.as_str(),
            "Subscribing to OpenHome events"
        );

        let host_header = format!("{}:{}", remote_host, remote_port);
        let timeout_header = format!("Second-{}", OPENHOME_SUBSCRIPTION_TIMEOUT_SECS);
        let callback_header = format!("<{}>", callback_url);

        let request = http::Request::builder()
            .method("SUBSCRIBE")
            .uri(&event_url)
            .header("HOST", host_header)
            .header("CALLBACK", callback_header)
            .header("NT", "upnp:event")
            .header("TIMEOUT", timeout_header)
            .body(())
            .map_err(anyhow::Error::new)?;

        let response = build_agent(http_timeout).run(request)?;
        if !response.status().is_success() {
            anyhow::bail!("SUBSCRIBE returned HTTP {}", response.status());
        }

        let sid = response
            .headers()
            .get("SID")
            .and_then(|value| value.to_str().ok())
            .map(|s| s.to_string())
            .ok_or_else(|| anyhow::anyhow!("SUBSCRIBE response missing SID"))?;
        let timeout = parse_timeout(
            response
                .headers()
                .get("TIMEOUT")
                .and_then(|value| value.to_str().ok()),
        )
        .unwrap_or(Duration::from_secs(OPENHOME_SUBSCRIPTION_TIMEOUT_SECS));

        entry.sid = Some(sid);
        entry.expires_at = Some(Instant::now() + timeout);
        entry.retry_after = Instant::now() + Duration::from_secs(5);

        info!(
            renderer = entry.renderer.friendly_name.as_str(),
            service = entry.service.as_str(),
            "Subscribed to OpenHome events (timeout {}s)",
            timeout.as_secs()
        );

        Ok(())
    }

    fn renew_entry(
        http_timeout: Duration,
        entry: &mut OpenHomeSubscriptionState,
    ) -> anyhow::Result<()> {
        let sid = entry.sid.clone().context("Cannot renew without SID")?;
        let request = http::Request::builder()
            .method("SUBSCRIBE")
            .uri(&entry.event_sub_url)
            .header("SID", sid)
            .header(
                "TIMEOUT",
                format!("Second-{}", OPENHOME_SUBSCRIPTION_TIMEOUT_SECS),
            )
            .body(())
            .map_err(anyhow::Error::new)?;
        let response = build_agent(http_timeout).run(request)?;
        if !response.status().is_success() {
            anyhow::bail!("SUBSCRIBE renewal failed with {}", response.status());
        }
        let timeout = parse_timeout(
            response
                .headers()
                .get("TIMEOUT")
                .and_then(|value| value.to_str().ok()),
        )
        .unwrap_or(Duration::from_secs(OPENHOME_SUBSCRIPTION_TIMEOUT_SECS));
        entry.expires_at = Some(Instant::now() + timeout);
        info!(
            renderer = entry.renderer.friendly_name.as_str(),
            service = entry.service.as_str(),
            "Renewed OpenHome subscription (timeout {}s)",
            timeout.as_secs()
        );
        Ok(())
    }

    fn unsubscribe_entry(
        http_timeout: Duration,
        entry: &mut OpenHomeSubscriptionState,
    ) -> anyhow::Result<()> {
        let sid = match entry.sid.take() {
            Some(sid) => sid,
            None => return Ok(()),
        };

        let request = http::Request::builder()
            .method("UNSUBSCRIBE")
            .uri(&entry.event_sub_url)
            .header("SID", sid)
            .body(())
            .map_err(anyhow::Error::new)?;
        let response = build_agent(http_timeout).run(request)?;
        if !response.status().is_success() {
            warn!(
                renderer = entry.renderer.friendly_name.as_str(),
                service = entry.service.as_str(),
                status = response.status().as_u16(),
                "UNSUBSCRIBE returned non-success status"
            );
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct OpenHomeSubscriptionKey {
    renderer_id: RendererId,
    service: OhServiceKind,
}

impl OpenHomeSubscriptionKey {
    fn new(renderer_id: &RendererId, service: OhServiceKind) -> Self {
        Self {
            renderer_id: renderer_id.clone(),
            service,
        }
    }
}

struct OpenHomeSubscriptionState {
    renderer: RendererInfo,
    service: OhServiceKind,
    event_sub_url: String,
    callback_path: String,
    sid: Option<String>,
    expires_at: Option<Instant>,
    retry_after: Instant,
}

impl OpenHomeSubscriptionState {
    fn new(renderer: RendererInfo, service: OhServiceKind, event_sub_url: String) -> Self {
        Self {
            callback_path: build_openhome_callback_path(&renderer.id, service),
            renderer,
            service,
            event_sub_url,
            sid: None,
            expires_at: None,
            retry_after: Instant::now(),
        }
    }

    fn update(&mut self, renderer: RendererInfo, event_url: String) {
        if self.renderer.location != renderer.location || self.event_sub_url != event_url {
            self.event_sub_url = event_url;
            self.sid = None;
            self.expires_at = None;
            self.retry_after = Instant::now();
        }
        self.renderer = renderer;
    }

    fn should_retry(&self) -> bool {
        Instant::now() >= self.retry_after
    }

    fn defer_retry(&mut self) {
        self.retry_after = Instant::now() + Duration::from_secs(15);
    }

    fn reset_subscription(&mut self) {
        self.sid = None;
        self.expires_at = None;
        self.retry_after = Instant::now() + Duration::from_secs(5);
    }
}

struct OpenHomeIncomingNotify {
    path: String,
    sid: Option<String>,
    body: Vec<u8>,
}

fn run_openhome_http_listener(listener: TcpListener, notify_tx: Sender<OpenHomeIncomingNotify>) {
    for stream in listener.incoming() {
        match stream {
            Ok(mut stream) => {
                if let Err(err) = stream.set_read_timeout(Some(Duration::from_secs(5))) {
                    warn!(
                        "Failed to set read timeout on OpenHome notify connection: {}",
                        err
                    );
                }

                match read_openhome_http_request(&mut stream) {
                    Ok(request) => {
                        if request.method != "NOTIFY" {
                            let _ = write_openhome_http_response(
                                &mut stream,
                                405,
                                "Method Not Allowed",
                            );
                            continue;
                        }

                        let notify = OpenHomeIncomingNotify {
                            path: request.path,
                            sid: request.headers.get("sid").cloned(),
                            body: request.body,
                        };

                        if notify_tx.send(notify).is_err() {
                            warn!("Dropping OpenHome notify because worker channel is closed");
                        }
                        let _ = write_openhome_http_response(&mut stream, 200, "OK");
                    }
                    Err(err) => {
                        warn!("Failed to parse OpenHome notify request: {}", err);
                        let _ = write_openhome_http_response(&mut stream, 400, "Bad Request");
                    }
                }
            }
            Err(err) => {
                warn!("Incoming OpenHome notify connection failed: {}", err);
            }
        }
    }
}

struct OpenHomeHttpRequest {
    method: String,
    path: String,
    headers: HashMap<String, String>,
    body: Vec<u8>,
}

fn read_openhome_http_request(stream: &mut TcpStream) -> io::Result<OpenHomeHttpRequest> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut request_line = String::new();
    if reader.read_line(&mut request_line)? == 0 {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "missing request line",
        ));
    }

    let request_line = request_line.trim_end_matches(&['\r', '\n'][..]);
    let mut parts = request_line.split_whitespace();
    let method = parts
        .next()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing method"))?
        .to_ascii_uppercase();
    let path = parts
        .next()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing path"))?
        .to_string();

    let mut headers = HashMap::new();
    loop {
        let mut line = String::new();
        let len = reader.read_line(&mut line)?;
        if len == 0 {
            break;
        }
        let trimmed = line.trim_end_matches(&['\r', '\n'][..]);
        if trimmed.is_empty() {
            break;
        }
        if let Some((name, value)) = trimmed.split_once(':') {
            headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_string());
        }
    }

    let content_length: usize = headers
        .get("content-length")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);

    let mut body = vec![0u8; content_length];
    reader.read_exact(&mut body)?;

    Ok(OpenHomeHttpRequest {
        method,
        path,
        headers,
        body,
    })
}

fn write_openhome_http_response(
    stream: &mut TcpStream,
    status: u16,
    message: &str,
) -> io::Result<()> {
    let response = format!(
        "HTTP/1.1 {} {}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        status, message
    );
    stream.write_all(response.as_bytes())
}

fn build_openhome_callback_path(id: &RendererId, service: OhServiceKind) -> String {
    let mut sanitized = String::new();
    for ch in id.0.chars() {
        if ch.is_ascii_alphanumeric() {
            sanitized.push(ch);
        } else {
            sanitized.push('_');
        }
    }
    let mut hasher = DefaultHasher::new();
    id.hash(&mut hasher);
    service.hash(&mut hasher);
    let suffix = hasher.finish();
    format!("/openhome-events/{}/{:x}", service.as_str(), suffix)
}

fn parse_openhome_propertyset(
    renderer_id: &RendererId,
    service: &OhServiceKind,
    body: &[u8],
) -> Vec<(String, String)> {
    let mut properties = Vec::new();
    let reader = std::io::Cursor::new(body);
    let Ok(root) = Element::parse(reader) else {
        warn!(
            renderer = renderer_id.0.as_str(),
            service = service.as_str(),
            "Failed to parse OpenHome notify payload"
        );
        return properties;
    };

    for property in root.children.iter().filter_map(|node| match node {
        XMLNode::Element(elem) => Some(elem),
        _ => None,
    }) {
        for child in property.children.iter().filter_map(|node| match node {
            XMLNode::Element(elem) => Some(elem),
            _ => None,
        }) {
            if let Some(text) = child.get_text() {
                properties.push((child.name.clone(), text.into_owned()));
            }
        }
    }

    properties
}

fn is_id_array_property(name: &str) -> bool {
    name.trim().to_ascii_lowercase().ends_with("idarray")
}

fn empty_playback_position() -> PlaybackPositionInfo {
    PlaybackPositionInfo {
        track: None,
        rel_time: None,
        abs_time: None,
        track_duration: None,
        track_metadata: None,
        track_uri: None,
    }
}

fn parse_timeout(raw: Option<&str>) -> Option<Duration> {
    let value = raw?;
    let lower = value.trim().to_ascii_lowercase();
    if lower == "second-infinite" {
        return Some(Duration::from_secs(OPENHOME_SUBSCRIPTION_TIMEOUT_SECS));
    }
    if let Some(idx) = lower.find("second-") {
        let number = &lower[idx + 7..];
        if let Ok(seconds) = number.parse::<u64>() {
            return Some(Duration::from_secs(seconds));
        }
    }
    None
}

fn parse_host_port(url: &str) -> Option<(String, u16)> {
    let default_port = if url.to_ascii_lowercase().starts_with("https://") {
        443
    } else {
        80
    };
    let (_, rest) = url.split_once("://")?;
    let mut parts = rest.splitn(2, '/');
    let authority = parts.next()?.trim();
    if authority.starts_with('[') {
        let end = authority.find(']')?;
        let host = &authority[1..end];
        let remainder = authority.get(end + 1..).unwrap_or("");
        let port = if let Some(stripped) = remainder.strip_prefix(':') {
            stripped.parse().unwrap_or(default_port)
        } else {
            default_port
        };
        Some((host.to_string(), port))
    } else if let Some((host, port)) = authority.split_once(':') {
        Some((host.to_string(), port.parse().ok()?))
    } else {
        Some((authority.to_string(), default_port))
    }
}

fn determine_local_ip(remote_host: &str, remote_port: u16) -> io::Result<IpAddr> {
    let is_ipv6 = remote_host.contains(':') && !remote_host.contains('.');
    let target = if is_ipv6 {
        format!(
            "[{}]:{}",
            remote_host.trim_matches(|c| c == '[' || c == ']'),
            remote_port
        )
    } else {
        format!("{}:{}", remote_host, remote_port)
    };
    let bind_addr = if is_ipv6 { "[::]:0" } else { "0.0.0.0:0" };
    let socket = UdpSocket::bind(bind_addr)?;
    socket.connect(&target)?;
    Ok(socket.local_addr()?.ip())
}

fn format_ip(ip: &IpAddr) -> String {
    match ip {
        IpAddr::V4(v4) => v4.to_string(),
        IpAddr::V6(v6) => format!("[{}]", v6),
    }
}

fn build_agent(timeout: Duration) -> Agent {
    Agent::config_builder()
        .timeout_global(Some(timeout))
        .http_status_as_error(false)
        .allow_non_standard_methods(true)
        .build()
        .into()
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
