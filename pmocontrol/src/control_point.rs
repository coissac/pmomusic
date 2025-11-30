use std::collections::{HashMap, HashSet};
use std::io;
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::Duration;

use crossbeam_channel::Receiver;
use pmoupnp::ssdp::SsdpClient;

use crate::MusicRenderer;
use crate::capabilities::{PlaybackPosition, PlaybackPositionInfo, PlaybackState, PlaybackStatus};
use crate::discovery::DiscoveryManager;
use crate::events::RendererEventBus;
use crate::model::{RendererEvent, RendererId, RendererProtocol};
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
}

impl ControlPoint {
    /// Crée un ControlPoint et lance le thread de découverte SSDP.
    ///
    /// `timeout_secs` : timeout HTTP pour la récupération des descriptions UPnP.
    pub fn spawn(timeout_secs: u64) -> io::Result<Self> {
        let registry = Arc::new(RwLock::new(DeviceRegistry::new()));
        let event_bus = RendererEventBus::new();

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
        };

        thread::spawn(move || {
            let mut cache: HashMap<RendererId, RendererRuntimeSnapshot> = HashMap::new();

            loop {
                let renderers = {
                    let reg = runtime_cp.registry.read().unwrap();
                    reg.list_renderers()
                        .into_iter()
                        .filter_map(|info| MusicRenderer::from_registry_info(info, &reg))
                        .collect::<Vec<_>>()
                };

                let mut seen_ids = HashSet::new();

                for renderer in renderers {
                    let info = renderer.info();

                    // Ne pas poller les renderers offline
                    if !info.online {
                        continue;
                    }

                    match info.protocol {
                        RendererProtocol::UpnpAvOnly | RendererProtocol::Hybrid => {}
                        RendererProtocol::OpenHomeOnly => continue,
                    }

                    let renderer_id = info.id.clone();
                    seen_ids.insert(renderer_id.clone());

                    let entry = cache
                        .entry(renderer_id.clone())
                        .or_insert_with(RendererRuntimeSnapshot::default);

                    // Keep a snapshot of the previous position to compute logical
                    // state transitions based on time deltas.
                    let prev_position = entry.position.clone();

                    // 1) Poll position first, so that the state logic can use the
                    //    freshly updated position when available.
                    if let Ok(position) = renderer.playback_position() {
                        let has_changed = match entry.position.as_ref() {
                            Some(prev) => !playback_position_equal(prev, &position),
                            None => true,
                        };

                        if has_changed {
                            runtime_cp.emit_renderer_event(RendererEvent::PositionChanged {
                                id: renderer_id.clone(),
                                position: position.clone(),
                            });
                        }

                        entry.position = Some(position);
                    }

                    // 2) Poll raw playback state and compute a logical state that
                    //    compensates for buggy devices (Arylic / LinkPlay).
                    if let Ok(raw_state) = renderer.playback_state() {
                        let logical_state = compute_logical_playback_state(
                            &raw_state,
                            prev_position.as_ref(),
                            entry.position.as_ref(),
                        );

                        let has_changed = match entry.state.as_ref() {
                            Some(prev) => !playback_state_equal(prev, &logical_state),
                            None => true,
                        };

                        if has_changed {
                            runtime_cp.emit_renderer_event(RendererEvent::StateChanged {
                                id: renderer_id.clone(),
                                state: logical_state.clone(),
                            });
                            entry.state = Some(logical_state);
                        }
                    }
                }

                cache.retain(|id, _| seen_ids.contains(id));
                thread::sleep(Duration::from_secs(1));
            }
        });

        Ok(Self {
            registry,
            event_bus,
        })
    }

    /// Accès au DeviceRegistry partagé.
    pub fn registry(&self) -> Arc<RwLock<DeviceRegistry>> {
        Arc::clone(&self.registry)
    }

    /// Snapshot list of renderers currently known by the registry.
    pub fn list_upnp_renderers(&self) -> Vec<UpnpRenderer> {
        let reg = self.registry.read().unwrap();
        reg.list_renderers()
            .into_iter()
            .map(|info| UpnpRenderer::from_registry(info, &reg))
            .collect()
    }

    /// Return the first renderer in the registry, if any.
    pub fn default_upnp_renderer(&self) -> Option<UpnpRenderer> {
        let reg = self.registry.read().unwrap();
        reg.list_renderers()
            .into_iter()
            .next()
            .map(|info| UpnpRenderer::from_registry(info, &reg))
    }

    /// Lookup a renderer by id.
    pub fn upnp_renderer_by_id(&self, id: &RendererId) -> Option<UpnpRenderer> {
        let reg = self.registry.read().unwrap();
        reg.get_renderer(id)
            .map(|info| UpnpRenderer::from_registry(info, &reg))
    }

    /// Snapshot list of music renderers (protocol-agnostic view).
    ///
    /// For now, only UPnP AV / hybrid renderers are wrapped as
    /// [`MusicRenderer::Upnp`]. OpenHome-only devices will be
    /// ignored until an OpenHome backend is implemented.
    pub fn list_music_renderers(&self) -> Vec<MusicRenderer> {
        let reg = self.registry.read().unwrap();
        reg.list_renderers()
            .into_iter()
            .filter_map(|info| MusicRenderer::from_registry_info(info, &reg))
            .collect()
    }

    /// Return the first music renderer in the registry, if any.
    pub fn default_music_renderer(&self) -> Option<MusicRenderer> {
        let reg = self.registry.read().unwrap();
        reg.list_renderers()
            .into_iter()
            .find_map(|info| MusicRenderer::from_registry_info(info, &reg))
    }

    /// Lookup a music renderer by id.
    pub fn music_renderer_by_id(&self, id: &RendererId) -> Option<MusicRenderer> {
        let reg = self.registry.read().unwrap();
        reg.get_renderer(id)
            .and_then(|info| MusicRenderer::from_registry_info(info, &reg))
    }

    /// Subscribe to renderer events emitted by the control point runtime.
    ///
    /// Each subscriber receives all future events independently.
    pub fn subscribe_events(&self) -> Receiver<RendererEvent> {
        self.event_bus.subscribe()
    }

    #[allow(dead_code)]
    pub(crate) fn emit_renderer_event(&self, event: RendererEvent) {
        self.event_bus.broadcast(event);
    }
}

#[derive(Default)]
struct RendererRuntimeSnapshot {
    state: Option<PlaybackState>,
    position: Option<PlaybackPositionInfo>,
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
