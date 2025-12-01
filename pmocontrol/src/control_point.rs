use std::collections::HashMap;
use std::io;
use std::sync::{Arc, Mutex, RwLock};
use std::thread;
use std::time::Duration;

use anyhow::anyhow;
use crossbeam_channel::Receiver;
use pmoupnp::ssdp::SsdpClient;
use tracing::{debug, error, warn};

use crate::MusicRenderer;
use crate::capabilities::{
    PlaybackPosition, PlaybackPositionInfo, PlaybackState, PlaybackStatus, TransportControl,
    VolumeControl,
};
use crate::discovery::DiscoveryManager;
use crate::events::RendererEventBus;
use crate::media_server::{MediaServerInfo, ServerId};
use crate::model::{RendererEvent, RendererId, RendererProtocol};
use crate::music_renderer::op_not_supported;
use crate::playback_queue::{PlaybackItem, PlaybackQueue};
use crate::provider::HttpXmlDescriptionProvider;
use crate::registry::{DeviceRegistry, DeviceRegistryRead, DeviceUpdate};
use crate::upnp_renderer::UpnpRenderer;

/// Control point minimal :
/// - lance un SsdpClient dans un thread,
/// - passe les SsdpEvent au DiscoveryManager,
/// - applique les DeviceUpdate dans le DeviceRegistry.
pub struct ControlPoint {
    registry: Arc<RwLock<DeviceRegistry>>,
    event_bus: RendererEventBus,
    runtime: Arc<RuntimeState>,
}

impl ControlPoint {
    /// Crée un ControlPoint et lance le thread de découverte SSDP.
    ///
    /// `timeout_secs` : timeout HTTP pour la récupération des descriptions UPnP.
    pub fn spawn(timeout_secs: u64) -> io::Result<Self> {
        let registry = Arc::new(RwLock::new(DeviceRegistry::new()));
        let event_bus = RendererEventBus::new();
        let runtime = Arc::new(RuntimeState::new());

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
            runtime: Arc::clone(&runtime),
        };

        thread::spawn(move || {
            loop {
                let infos = {
                    let reg = runtime_cp.registry.read().unwrap();
                    reg.list_renderers()
                };
                let renderers = infos
                    .into_iter()
                    .filter_map(|info| MusicRenderer::from_registry_info(info, &runtime_cp.registry))
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

                        new_snapshot.position = Some(position);
                    }

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

                        if has_changed {
                            runtime_cp.emit_renderer_event(RendererEvent::StateChanged {
                                id: renderer_id.clone(),
                                state: logical_state.clone(),
                            });
                        }

                        new_snapshot.state = Some(logical_state);
                    }

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

                    runtime_cp
                        .runtime
                        .update_snapshot(&renderer_id, new_snapshot);
                }

                thread::sleep(Duration::from_secs(1));
            }
        });

        Ok(Self {
            registry,
            event_bus,
            runtime,
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

        let removed = self
            .runtime
            .with_queue_mut(renderer_id, |queue| {
                let removed = queue.len();
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

        let item_count = items.len();
        let new_len = self
            .runtime
            .with_queue_mut(renderer_id, |queue| {
                queue.enqueue_many(items);
                queue.len()
            })
            .ok_or_else(|| Self::runtime_entry_missing(renderer_id))?;

        debug!(
            renderer = renderer_id.0.as_str(),
            added = item_count,
            queue_len = new_len,
            "Enqueued playback items"
        );
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
            renderer.play_uri(&item.uri, "")?;
            renderer.play()?;
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
                        match upnp.set_next_uri(&next_item.uri, "") {
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

        Ok(())
    }

    /// Subscribe to renderer events emitted by the control point runtime.
    ///
    /// Each subscriber receives all future events independently.
    pub fn subscribe_events(&self) -> Receiver<RendererEvent> {
        self.event_bus.subscribe()
    }

    pub(crate) fn emit_renderer_event(&self, event: RendererEvent) {
        self.handle_renderer_event(&event);
        self.event_bus.broadcast(event);
    }

    fn handle_renderer_event(&self, event: &RendererEvent) {
        if let RendererEvent::StateChanged { id, state } = event {
            match state {
                PlaybackState::Stopped => {
                    if self.runtime.is_playing_from_queue(id) {
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

    fn dequeue_next(&self, id: &RendererId) -> Option<(PlaybackItem, usize)> {
        let mut entries = self.entries.lock().unwrap();
        let entry = entries.get_mut(id)?;
        let item = entry.queue.dequeue()?;
        let remaining = entry.queue.len();
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
}
