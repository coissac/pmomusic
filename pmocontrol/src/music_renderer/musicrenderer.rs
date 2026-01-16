//! Backend-agnostic music renderer façade for PMOMusic.
//!
//! `MusicRenderer` wraps every supported backend (UPnP AV/DLNA, OpenHome,
//! LinkPlay HTTP, Arylic TCP, Chromecast, and the hybrid UPnP + Arylic pairing) behind a
//! single control surface. Higher layers in PMOMusic must only interact with
//! renderers through this type so that transport, volume, and state queries
//! stay backend-neutral.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::SystemTime;

use tracing::{debug, error};

use crate::errors::ControlPointError;
use crate::events::RendererEventBus;
use crate::model::RendererEvent;
use crate::model::{PlaybackSource, PlaybackState, RendererInfo, RendererProtocol, TrackMetadata};
use crate::music_renderer::RendererFromMediaRendererInfo;
use crate::music_renderer::arylic_tcp::ArylicTcpRenderer;
use crate::music_renderer::capabilities::{
    PlaybackPosition, PlaybackPositionInfo, PlaybackStatus, QueueTransportControl, RendererBackend,
    TransportControl, VolumeControl,
};
use crate::music_renderer::chromecast_renderer::ChromecastRenderer;
use crate::music_renderer::linkplay_renderer::LinkPlayRenderer;
use crate::music_renderer::openhome_renderer::OpenHomeRenderer;
use crate::music_renderer::sleep_timer::SleepTimer;
use crate::music_renderer::upnp_renderer::UpnpRenderer;
use crate::music_renderer::watcher::{
    WatchStrategy, WatchedState, compute_logical_playback_state, extract_track_metadata,
    playback_position_equal, playback_state_equal,
};
use crate::online::DeviceConnectionState;
use crate::queue::{
    EnqueueMode, MusicQueue, PlaybackItem, QueueBackend, QueueFromRendererInfo, QueueSnapshot,
};
use crate::{DeviceId, DeviceIdentity, DeviceOnline};

use tracing::warn;

/// Describes a renderer's attachment to a media server playlist container.
///
/// When a renderer is attached to a playlist, the ControlPoint monitors the container
/// for updates and automatically refreshes the queue when changes are detected.
#[derive(Clone, Debug)]
pub struct PlaylistBinding {
    /// MediaServer that owns the playlist container.
    pub server_id: DeviceId,
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

/// Backend-agnostic façade exposing transport, volume, and status contracts.
#[derive(Clone, Debug)]
pub enum MusicRendererBackend {
    /// Classic UPnP AV / DLNA renderer (AVTransport + RenderingControl).
    Upnp(UpnpRenderer),
    /// Renderer powered by OpenHome services.
    OpenHome(OpenHomeRenderer),
    /// Renderer controlled via the LinkPlay HTTP API.
    LinkPlay(LinkPlayRenderer),
    /// Renderer reachable through the Arylic TCP control protocol (port 8899).
    ArylicTcp(ArylicTcpRenderer),
    /// Renderer controlled via the Google Cast protocol (Chromecast).
    Chromecast(ChromecastRenderer),
    /// Combined backend using UPnP for transport + volume writes and Arylic TCP
    /// to read detailed playback information as well as live volume/mute state.
    HybridUpnpArylic {
        upnp: UpnpRenderer,
        arylic: ArylicTcpRenderer,
    },
}

/// Internal state for tracking playback and control flow.
#[derive(Debug, Clone, Default)]
struct MusicRendererState {
    /// Last known track metadata (cached to avoid repeated queries).
    last_metadata: Option<TrackMetadata>,
    /// Source of the current playback (queue vs external).
    playback_source: PlaybackSource,
    /// Flag to distinguish user-requested stop from automatic events.
    user_stop_requested: bool,
    /// Sleep timer for auto-stop functionality.
    sleep_timer: SleepTimer,
    /// Flag indicating that a PLAYING state has been observed since the last track start.
    /// This prevents auto-advance on transient STOPPED states during track initialization.
    /// Auto-advance is only allowed when this flag is true.
    has_played_since_track_start: bool,
}

#[derive(Clone)]
pub struct MusicRenderer {
    info: RendererInfo,
    connection: Arc<Mutex<DeviceConnectionState>>,
    backend: Arc<Mutex<MusicRendererBackend>>,
    playlist_binding: Arc<Mutex<Option<PlaylistBinding>>>,
    state: Arc<Mutex<MusicRendererState>>,
    /// Optional event bus for emitting queue change events.
    event_bus: Option<RendererEventBus>,
    /// Cached state from last poll, used for change detection by the watcher.
    watched_state: Arc<Mutex<WatchedState>>,
    /// Flag to signal the watcher thread to stop.
    watcher_stop_flag: Arc<AtomicBool>,
    /// Handle to the watcher thread, if running.
    watcher_handle: Arc<Mutex<Option<JoinHandle<()>>>>,
}

impl std::fmt::Debug for MusicRenderer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MusicRenderer")
            .field("info", &self.info)
            .field("connection", &self.connection)
            .field("backend", &self.backend)
            .field("playlist_binding", &self.playlist_binding)
            .field("state", &self.state)
            .field(
                "event_bus",
                &self.event_bus.as_ref().map(|_| "RendererEventBus"),
            )
            .field("is_watching", &self.is_watching())
            .finish()
    }
}

impl MusicRenderer {
    pub fn new(
        info: RendererInfo,
        backend: Arc<Mutex<MusicRendererBackend>>,
    ) -> Arc<MusicRenderer> {
        let connection = DeviceConnectionState::new();

        let renderer = MusicRenderer {
            info,
            connection: Arc::new(Mutex::new(connection)),
            backend,
            playlist_binding: Arc::new(Mutex::new(None)),
            state: Arc::new(Mutex::new(MusicRendererState::default())),
            event_bus: None,
            watched_state: Arc::new(Mutex::new(WatchedState::default())),
            watcher_stop_flag: Arc::new(AtomicBool::new(false)),
            watcher_handle: Arc::new(Mutex::new(None)),
        };

        Arc::new(renderer)
    }

    pub fn from_renderer_info(info: &RendererInfo) -> Result<MusicRenderer, ControlPointError> {
        Self::from_renderer_info_with_bus(info, None)
    }

    pub fn from_renderer_info_with_bus(
        info: &RendererInfo,
        event_bus: Option<RendererEventBus>,
    ) -> Result<MusicRenderer, ControlPointError> {
        let connection = Arc::new(Mutex::new(DeviceConnectionState::new()));
        let backend = MusicRendererBackend::make_from_renderer_info(info)?;

        let renderer = MusicRenderer {
            info: info.clone(),
            connection,
            backend,
            playlist_binding: Arc::new(Mutex::new(None)),
            state: Arc::new(Mutex::new(MusicRendererState::default())),
            event_bus,
            watched_state: Arc::new(Mutex::new(WatchedState::default())),
            watcher_stop_flag: Arc::new(AtomicBool::new(false)),
            watcher_handle: Arc::new(Mutex::new(None)),
        };

        // Start watching immediately since the renderer is created online
        renderer.start_watching();

        Ok(renderer)
    }

    /// Helper method to emit a QueueUpdated event if an event bus is available.
    fn emit_queue_updated(&self) {
        if let Some(ref bus) = self.event_bus {
            let queue_length = self.len().unwrap_or(0);
            bus.broadcast(RendererEvent::QueueUpdated {
                id: self.id(),
                queue_length,
            });
        }
    }

    /// Helper method to emit any RendererEvent if an event bus is available.
    fn emit_event(&self, event: RendererEvent) {
        if let Some(ref bus) = self.event_bus {
            bus.broadcast(event);
        }
    }

    // =========================================================================
    // Watcher Thread Management
    // =========================================================================

    /// Starts the watcher thread for this renderer.
    ///
    /// The watcher thread polls the backend at regular intervals and emits
    /// events when state changes are detected. This method is idempotent:
    /// calling it when already watching is a no-op.
    pub fn start_watching(&self) {
        let mut handle_guard = self
            .watcher_handle
            .lock()
            .expect("Watcher handle mutex poisoned");
        if handle_guard.is_some() {
            return; // Already watching
        }

        // Reset the stop flag before starting
        self.watcher_stop_flag.store(false, Ordering::SeqCst);

        // Determine the watch strategy based on backend type
        let strategy = {
            let backend = self.backend.lock().expect("Backend mutex poisoned");
            WatchStrategy::for_backend(&*backend)
        };

        debug!(
            renderer = self.info.friendly_name(),
            strategy = ?strategy,
            "Starting watcher thread"
        );

        let handle = self.spawn_watcher_thread(strategy);
        *handle_guard = Some(handle);
    }

    /// Stops the watcher thread gracefully.
    ///
    /// This method is idempotent: calling it when not watching is a no-op.
    /// The method will block until the watcher thread terminates.
    pub fn stop_watching(&self) {
        // Signal the watcher to stop
        self.watcher_stop_flag.store(true, Ordering::SeqCst);

        // Take the handle and wait for the thread to finish
        let mut handle_guard = self
            .watcher_handle
            .lock()
            .expect("Watcher handle mutex poisoned");
        if let Some(handle) = handle_guard.take() {
            debug!(
                renderer = self.info.friendly_name(),
                "Stopping watcher thread"
            );
            // Wait for the thread to finish (ignore join errors)
            let _ = handle.join();
        }
    }

    /// Returns true if the watcher thread is currently running.
    pub fn is_watching(&self) -> bool {
        self.watcher_handle
            .lock()
            .expect("Watcher handle mutex poisoned")
            .is_some()
    }

    /// Spawns the watcher thread with the given strategy.
    fn spawn_watcher_thread(&self, strategy: WatchStrategy) -> JoinHandle<()> {
        let renderer = self.clone();
        let stop_flag = Arc::clone(&self.watcher_stop_flag);

        thread::Builder::new()
            .name(format!("watcher-{}", self.info.friendly_name()))
            .spawn(move || {
                renderer.watcher_loop(strategy, stop_flag);
            })
            .expect("Failed to spawn watcher thread")
    }

    /// Main loop for the watcher thread.
    fn watcher_loop(&self, strategy: WatchStrategy, stop_flag: Arc<AtomicBool>) {
        let Some(interval) = strategy.polling_interval() else {
            // Pure push strategy - no polling needed (future implementation)
            return;
        };

        let mut tick: u32 = 0;

        while !stop_flag.load(Ordering::SeqCst) {
            if self.is_online() {
                self.poll_and_emit_changes(tick);
            }

            tick = tick.wrapping_add(1);
            thread::sleep(interval);
        }

        debug!(
            renderer = self.info.friendly_name(),
            "Watcher thread exiting"
        );
    }

    /// Polls the backend and emits events for any detected changes.
    fn poll_and_emit_changes(&self, tick: u32) {
        let mut watched = self
            .watched_state
            .lock()
            .expect("WatchedState mutex poisoned");
        let prev_position = watched.position.clone();

        // Poll position every tick
        if let Ok(position) = self.playback_position() {
            let changed = watched
                .position
                .as_ref()
                .map(|prev| !playback_position_equal(prev, &position))
                .unwrap_or(true);

            if changed {
                self.emit_event(RendererEvent::PositionChanged {
                    id: self.id(),
                    position: position.clone(),
                });
            }

            // Extract and emit metadata changes
            if let Some(metadata) = extract_track_metadata(&position) {
                let metadata_changed = watched
                    .metadata
                    .as_ref()
                    .map(|prev| prev != &metadata)
                    .unwrap_or(true);

                if metadata_changed {
                    debug!(
                        renderer = self.info.friendly_name(),
                        title = metadata.title.as_deref(),
                        artist = metadata.artist.as_deref(),
                        "Emitting metadata changed event"
                    );
                    self.emit_event(RendererEvent::MetadataChanged {
                        id: self.id(),
                        metadata: metadata.clone(),
                    });
                    watched.metadata = Some(metadata);
                }
            }

            watched.position = Some(position);
        }

        // Poll state every tick
        if let Ok(raw_state) = self.playback_state() {
            let logical_state = compute_logical_playback_state(
                &raw_state,
                prev_position.as_ref(),
                watched.position.as_ref(),
            );

            let changed = watched
                .state
                .as_ref()
                .map(|prev| !playback_state_equal(prev, &logical_state))
                .unwrap_or(true);

            // Emit event only for non-transient states to reduce noise
            if changed && !matches!(logical_state, PlaybackState::Transitioning) {
                self.emit_event(RendererEvent::StateChanged {
                    id: self.id(),
                    state: logical_state.clone(),
                });

                // Handle auto-advance logic internally
                // Release the lock before calling handle_state_change to avoid deadlock
                drop(watched);
                self.handle_state_change(&logical_state);
                watched = self
                    .watched_state
                    .lock()
                    .expect("WatchedState mutex poisoned");
            }

            watched.state = Some(logical_state);
        }

        // Poll volume and mute every other tick (1 second at 500ms interval)
        if tick % 2 == 0 {
            if let Ok(volume) = self.volume() {
                if watched.volume != Some(volume) {
                    self.emit_event(RendererEvent::VolumeChanged {
                        id: self.id(),
                        volume,
                    });
                    watched.volume = Some(volume);
                }
            }

            if let Ok(mute) = self.mute() {
                if watched.mute != Some(mute) {
                    self.emit_event(RendererEvent::MuteChanged {
                        id: self.id(),
                        mute,
                    });
                    watched.mute = Some(mute);
                }
            }
        }
    }

    /// Handles playback state changes internally (auto-advance logic).
    ///
    /// This method is called by the watcher when a state change is detected.
    /// It handles the auto-advance behavior when playback stops.
    fn handle_state_change(&self, state: &PlaybackState) {
        match state {
            PlaybackState::Stopped => {
                // Check if user requested stop (via Stop button in UI)
                if self.check_and_clear_user_stop_requested() {
                    debug!(
                        renderer = self.info.friendly_name(),
                        "Renderer stopped by user request; not auto-advancing"
                    );
                    self.set_playback_source(PlaybackSource::None);
                    self.clear_has_played_flag();
                } else if self.is_playing_from_queue() {
                    // Only auto-advance if we have actually seen a PLAYING state
                    // since the track was started. This prevents auto-advance on
                    // transient STOPPED states during track initialization.
                    if self.check_and_clear_has_played_flag() {
                        debug!(
                            renderer = self.info.friendly_name(),
                            "Renderer stopped after queue-driven playback; advancing"
                        );
                        if let Err(err) = self.play_next_from_queue() {
                            error!(
                                renderer = self.info.friendly_name(),
                                error = %err,
                                "Auto-advance failed; clearing queue playback state"
                            );
                            self.set_playback_source(PlaybackSource::None);
                        }
                    } else {
                        debug!(
                            renderer = self.info.friendly_name(),
                            "Renderer stopped but no PLAYING state seen yet; ignoring (likely track initialization)"
                        );
                    }
                } else {
                    self.set_playback_source(PlaybackSource::None);
                    self.clear_has_played_flag();
                }
            }
            PlaybackState::Playing => {
                self.mark_external_if_idle();
                // Mark that we have seen a PLAYING state - auto-advance is now allowed
                self.set_has_played_flag();
            }
            _ => {}
        }
    }

    pub fn info(&self) -> &RendererInfo {
        &self.info
    }

    /// Returns the protocol.
    pub fn protocol(&self) -> RendererProtocol {
        self.info.protocol()
    }

    pub fn is_upnp(&self) -> bool {
        match &*self.backend.lock().expect("Backend mutex poisoned") {
            MusicRendererBackend::Upnp(_) => true,
            _ => false,
        }
    }

    pub fn is_openhome(&self) -> bool {
        match &*self.backend.lock().expect("Backend mutex poisoned") {
            MusicRendererBackend::OpenHome(_) => true,
            _ => false,
        }
    }

    pub fn is_linkplay(&self) -> bool {
        match &*self.backend.lock().expect("Backend mutex poisoned") {
            MusicRendererBackend::LinkPlay(_) => true,
            _ => false,
        }
    }

    pub fn is_arylictcp(&self) -> bool {
        match &*self.backend.lock().expect("Backend mutex poisoned") {
            MusicRendererBackend::ArylicTcp(_) => true,
            _ => false,
        }
    }

    pub fn is_chromecast(&self) -> bool {
        match &*self.backend.lock().expect("Backend mutex poisoned") {
            MusicRendererBackend::Chromecast(_) => true,
            _ => false,
        }
    }

    pub fn is_hybridupnparylic(&self) -> bool {
        match &*self.backend.lock().expect("Backend mutex poisoned") {
            MusicRendererBackend::HybridUpnpArylic { .. } => true,
            _ => false,
        }
    }

    /// Returns true if this renderer is known to support SetNextAVTransportURI.
    pub fn supports_set_next(&self) -> bool {
        self.info.capabilities().supports_set_next()
    }

    /// Prepare the renderer for attaching a new playlist by clearing the queue and stopping playback.
    pub fn clear_for_playlist_attach(&self) -> Result<(), ControlPointError> {
        // Clear the queue first
        self.backend
            .lock()
            .expect("Backend mutex poisoned")
            .clear_queue()?;

        // Then stop playback (ignore errors if already stopped)
        self.backend
            .lock()
            .expect("Backend mutex poisoned")
            .stop()
            .or_else(|err| {
                warn!(
                    renderer = self.id().0.as_str(),
                    error = %err,
                    "Stop failed when preparing for playlist attach (continuing anyway)"
                );
                Ok(())
            })
    }

    /// Get the current queue snapshot.
    pub fn queue_snapshot(&self) -> Result<QueueSnapshot, ControlPointError> {
        let mut snapshot = self
            .backend
            .lock()
            .expect("Backend mutex poisoned")
            .queue_snapshot()?;

        // Enrich snapshot with playlist_id from binding if available
        if let Some(binding) = self
            .playlist_binding
            .lock()
            .expect("Binding mutex poisoned")
            .as_ref()
        {
            // Use container_id as the playlist identifier
            snapshot.playlist_id = Some(binding.container_id.clone());
        }

        Ok(snapshot)
    }

    /// Get the current queue item without advancing.
    /// Returns the item and count of remaining items after current.
    pub fn peek_current(&self) -> Result<Option<(PlaybackItem, usize)>, ControlPointError> {
        self.backend
            .lock()
            .expect("Backend mutex poisoned")
            .peek_current()
    }

    /// Get the count of items remaining after the current index.
    pub fn upcoming_len(&self) -> Result<usize, ControlPointError> {
        self.backend
            .lock()
            .expect("Backend mutex poisoned")
            .upcoming_len()
    }

    /// Play the current item from the queue.
    pub fn play_current_from_queue(&self) -> Result<(), ControlPointError> {
        // Reset the has_played flag before starting playback to prevent
        // auto-advance on transient STOPPED states during track initialization.
        // The flag will be set back to true when PLAYING state is detected.
        self.clear_has_played_flag();

        self.backend
            .lock()
            .expect("Backend mutex poisoned")
            .play_from_queue()
    }

    /// Advance to and play the next item from the queue.
    pub fn play_next_from_queue(&self) -> Result<(), ControlPointError> {
        // Reset the has_played flag before starting playback to prevent
        // auto-advance on transient STOPPED states during track initialization.
        // The flag will be set back to true when PLAYING state is detected.
        self.clear_has_played_flag();

        self.backend
            .lock()
            .expect("Backend mutex poisoned")
            .play_next()?;
        self.emit_queue_updated();
        Ok(())
    }

    /// Play from a specific index in the queue.
    pub fn play_from_index(&self, index: usize) -> Result<(), ControlPointError> {
        // Reset the has_played flag before starting playback to prevent
        // auto-advance on transient STOPPED states during track initialization.
        // The flag will be set back to true when PLAYING state is detected.
        self.clear_has_played_flag();

        self.backend
            .lock()
            .expect("Backend mutex poisoned")
            .play_from_index(index)?;
        self.emit_queue_updated();
        Ok(())
    }

    /// Transport control: play
    ///
    /// Démarre ou reprend la lecture. Si une queue non vide existe,
    /// joue le track courant de la queue automatiquement (comportement unifié pour tous les backends).
    pub fn play(&self) -> Result<(), ControlPointError> {
        // Vérifier si on a une queue non vide
        let backend = self.backend.lock().expect("Backend mutex poisoned");
        let queue_not_empty = backend.len().unwrap_or(0) > 0;

        if queue_not_empty {
            // Si on a des items dans la queue, jouer le track courant (ou le premier si aucun n'est sélectionné)
            // Cela fonctionne pour tous les backends (UPnP interne, OpenHome, etc.)
            backend.play_from_queue()
        } else {
            // Queue vide : déléguer au backend (reprend la lecture en cours, etc.)
            backend.play()
        }
    }

    /// Transport control: pause
    pub fn pause(&self) -> Result<(), ControlPointError> {
        self.backend.lock().expect("Backend mutex poisoned").pause()
    }

    /// Transport control: stop
    pub fn stop(&self) -> Result<(), ControlPointError> {
        // Reset the has_played flag when stopping playback.
        // This ensures that if we start a new track, the flag will be false
        // until PLAYING state is observed, preventing auto-advance on
        // transient STOPPED states during track initialization.
        self.clear_has_played_flag();

        self.backend.lock().expect("Backend mutex poisoned").stop()
    }

    /// Set the next URI for gapless playback (UPnP AVTransport only).
    ///
    /// Returns Ok if the backend supports this feature and it succeeded.
    /// Returns Err if not supported or if it failed.
    pub fn set_next_uri(&self, uri: &str, metadata: &str) -> Result<(), ControlPointError> {
        let backend = self.backend.lock().expect("Backend mutex poisoned");

        match &*backend {
            MusicRendererBackend::Upnp(upnp) => upnp.set_next_uri(uri, metadata),
            MusicRendererBackend::HybridUpnpArylic { upnp, .. } => upnp.set_next_uri(uri, metadata),
            _ => Err(ControlPointError::ControlPoint(
                "SetNextURI not supported by this backend".to_string(),
            )),
        }
    }

    /// Transport control: seek to relative time
    pub fn seek_rel_time(&self, hhmmss: &str) -> Result<(), ControlPointError> {
        self.backend
            .lock()
            .expect("Backend mutex poisoned")
            .seek_rel_time(hhmmss)
    }

    /// Seek to a specific position in seconds
    pub fn seek(&self, seconds: u32) -> Result<(), ControlPointError> {
        // Convert seconds to HH:MM:SS format
        let hours = seconds / 3600;
        let minutes = (seconds % 3600) / 60;
        let secs = seconds % 60;
        let hhmmss = format!("{:02}:{:02}:{:02}", hours, minutes, secs);
        self.seek_rel_time(&hhmmss)
    }

    /// Volume control: get current volume
    pub fn volume(&self) -> Result<u16, ControlPointError> {
        self.backend
            .lock()
            .expect("Backend mutex poisoned")
            .volume()
    }

    /// Volume control: set volume
    pub fn set_volume(&self, vol: u16) -> Result<(), ControlPointError> {
        self.backend
            .lock()
            .expect("Backend mutex poisoned")
            .set_volume(vol)
    }

    /// Volume control: get mute state
    pub fn mute(&self) -> Result<bool, ControlPointError> {
        self.backend.lock().expect("Backend mutex poisoned").mute()
    }

    /// Volume control: set mute state
    pub fn set_mute(&self, m: bool) -> Result<(), ControlPointError> {
        self.backend
            .lock()
            .expect("Backend mutex poisoned")
            .set_mute(m)
    }

    /// Get playback state
    pub fn playback_state(&self) -> Result<PlaybackState, ControlPointError> {
        self.backend
            .lock()
            .expect("Backend mutex poisoned")
            .playback_state()
    }

    /// Get playback position
    pub fn playback_position(&self) -> Result<PlaybackPositionInfo, ControlPointError> {
        self.backend
            .lock()
            .expect("Backend mutex poisoned")
            .playback_position()
    }

    /// Sets the playlist binding for this renderer.
    /// Emits a `BindingChanged` event only if the binding actually changes.
    pub fn set_playlist_binding(&self, binding: Option<PlaylistBinding>) {
        let mut guard = self
            .playlist_binding
            .lock()
            .expect("Playlist binding mutex poisoned");

        // Check if there's an actual change (both None, or different Some values)
        let old_is_some = guard.is_some();
        let new_is_some = binding.is_some();
        let has_changed = old_is_some != new_is_some || (old_is_some && new_is_some);

        *guard = binding.clone();
        drop(guard);

        if has_changed {
            self.emit_binding_changed(binding);
        }
    }

    /// Gets the current playlist binding, if any.
    pub fn get_playlist_binding(&self) -> Option<PlaylistBinding> {
        self.playlist_binding
            .lock()
            .expect("Playlist binding mutex poisoned")
            .clone()
    }

    /// Clears the playlist binding.
    /// Emits a `BindingChanged` event with `None` only if there was a binding to clear.
    pub fn clear_playlist_binding(&self) {
        let mut guard = self
            .playlist_binding
            .lock()
            .expect("Playlist binding mutex poisoned");

        let had_binding = guard.is_some();
        *guard = None;
        drop(guard);

        if had_binding {
            self.emit_binding_changed(None);
        }
    }

    /// Helper method to emit a BindingChanged event if an event bus is available.
    fn emit_binding_changed(&self, binding: Option<PlaylistBinding>) {
        if let Some(ref bus) = self.event_bus {
            bus.broadcast(RendererEvent::BindingChanged {
                id: self.id(),
                binding,
            });
        }
    }

    /// Marks the current playlist binding for refresh if it matches the given server and container.
    /// Returns true if a matching binding was found and marked.
    pub fn mark_binding_for_refresh(&self, server_id: &DeviceId, container_ids: &[String]) -> bool {
        let mut binding_guard = self
            .playlist_binding
            .lock()
            .expect("Playlist binding mutex poisoned");

        if let Some(binding) = binding_guard.as_mut() {
            if &binding.server_id == server_id && container_ids.contains(&binding.container_id) {
                binding.pending_refresh = true;
                binding.has_seen_update = true;
                return true;
            }
        }

        false
    }

    /// Checks if the binding has pending_refresh flag set.
    /// Returns false if no binding exists.
    pub fn has_pending_refresh(&self) -> bool {
        self.playlist_binding
            .lock()
            .expect("Playlist binding mutex poisoned")
            .as_ref()
            .map(|b| b.pending_refresh)
            .unwrap_or(false)
    }

    /// Resets the pending_refresh flag to false.
    /// Does nothing if no binding exists.
    pub fn reset_pending_refresh(&self) {
        if let Some(binding) = self
            .playlist_binding
            .lock()
            .expect("Playlist binding mutex poisoned")
            .as_mut()
        {
            binding.pending_refresh = false;
        }
    }

    /// Marks the binding as pending refresh.
    /// Returns true if a binding exists and was marked, false otherwise.
    pub fn mark_pending_refresh(&self) -> bool {
        self.playlist_binding
            .lock()
            .expect("Playlist binding mutex poisoned")
            .as_mut()
            .map(|binding| {
                binding.pending_refresh = true;
                true
            })
            .unwrap_or(false)
    }

    /// Consumes and returns the auto_play_on_refresh flag, resetting it to false.
    /// Returns false if no binding exists.
    pub fn consume_auto_play(&self) -> bool {
        self.playlist_binding
            .lock()
            .expect("Playlist binding mutex poisoned")
            .as_mut()
            .map(|binding| {
                let auto_play = binding.auto_play_on_refresh;
                binding.auto_play_on_refresh = false;
                auto_play
            })
            .unwrap_or(false)
    }

    /// Returns the number of items in the queue.
    pub fn len(&self) -> Result<usize, ControlPointError> {
        let backend = self.backend.lock().expect("Backend mutex poisoned");
        backend.len()
    }

    /// Add items to the queue using the specified enqueue mode.
    pub fn enqueue_items(
        &self,
        items: Vec<PlaybackItem>,
        mode: EnqueueMode,
    ) -> Result<(), ControlPointError> {
        let mut backend = self.backend.lock().expect("Backend mutex poisoned");
        backend.enqueue_items(items, mode)?;
        drop(backend);
        self.emit_queue_updated();
        Ok(())
    }

    /// Synchronize the queue with new items while preserving the current track.
    ///
    /// This method intelligently updates the queue:
    /// - If the current track is in the new items, it keeps playing at the new position
    /// - If the current track is NOT in the new items, it's preserved as the first item
    /// - If there's no current track, the queue is simply replaced
    pub fn sync_queue(&self, items: Vec<PlaybackItem>) -> Result<(), ControlPointError> {
        let mut backend = self.backend.lock().expect("Backend mutex poisoned");
        backend.sync_queue(items)?;
        drop(backend);
        self.emit_queue_updated();
        Ok(())
    }

    /// Set the current queue index (for advanced use).
    /// Note: This does NOT start playback. Use select_queue_track() to play.
    pub fn set_queue_index(&self, index: Option<usize>) -> Result<(), ControlPointError> {
        let mut backend = self.backend.lock().expect("Backend mutex poisoned");
        backend.set_index(index)
    }

    /// Clears the renderer's queue using the generic QueueBackend trait.
    pub fn clear_queue(&self) -> Result<(), ControlPointError> {
        let mut backend = self.backend.lock().expect("Backend mutex poisoned");
        backend.clear_queue()?;
        drop(backend);
        self.emit_queue_updated();
        Ok(())
    }

    /// Dequeues and returns the next item from the queue.
    pub fn dequeue_next(&self) -> Result<Option<(PlaybackItem, usize)>, ControlPointError> {
        let mut backend = self.backend.lock().expect("Backend mutex poisoned");
        backend.dequeue_next()
    }

    /// Sets the current index in the queue.
    pub fn set_index(&self, index: Option<usize>) -> Result<(), ControlPointError> {
        let mut backend = self.backend.lock().expect("Backend mutex poisoned");
        backend.set_index(index)
    }

    /// Replaces the entire queue with new items and sets the current index.
    /// This is a complete replacement, unlike sync_queue which tries to preserve the current track.
    pub fn replace_queue(
        &self,
        items: Vec<PlaybackItem>,
        current_index: Option<usize>,
    ) -> Result<(), ControlPointError> {
        let mut backend = self.backend.lock().expect("Backend mutex poisoned");
        backend.replace_queue(items, current_index)?;
        drop(backend);
        self.emit_queue_updated();
        Ok(())
    }

    /// Adds a track to the queue.
    ///
    /// This is primarily for backends with persistent queues (OpenHome).
    /// For backends without this capability, it returns an error.
    ///
    /// Returns the backend-specific track ID if applicable.
    pub fn add_track_to_queue(
        &self,
        _uri: &str,
        _metadata: &str,
        _after_id: Option<u32>,
        _play: bool,
    ) -> Result<Option<u32>, ControlPointError> {
        // This operation requires backend-specific APIs (especially for OpenHome)
        // that aren't exposed through the generic QueueBackend trait.
        // The generic implementation cannot support this without more context
        // (media_server_id, didl_id, protocol_info, etc.).
        Err(ControlPointError::QueueError(
            "add_track_to_queue requires backend-specific implementation".to_string(),
        ))
    }

    /// Selects and plays a specific track from the queue by ID.
    ///
    /// Converts the track ID to a position using the generic QueueBackend trait,
    /// then plays the track.
    pub fn select_queue_track(&self, track_id: u32) -> Result<(), ControlPointError> {
        // Convert track_id to index
        let index = {
            let backend = self.backend.lock().expect("Backend mutex poisoned");
            backend.id_to_position(track_id)?
        };

        // Play from that index
        self.play_from_index(index)
    }

    /// Synchronizes the queue state with the backend.
    ///
    /// For backends with persistent queues (OpenHome), this refreshes the local view.
    /// For others, this is essentially a no-op (just reads the current state).
    pub fn sync_queue_state(&self) -> Result<(), ControlPointError> {
        let backend = self.backend.lock().expect("Backend mutex poisoned");
        // Calling queue_snapshot() triggers a refresh for backends that need it
        let _ = backend.queue_snapshot()?;
        Ok(())
    }

    /// Plays the current item from the backend queue.
    ///
    /// This is primarily for backends with persistent queues (OpenHome).
    pub fn play_current_from_backend_queue(&self) -> Result<(), ControlPointError> {
        let backend = self.backend.lock().expect("Backend mutex poisoned");

        // Get current track ID using generic QueueBackend trait
        let track_id = backend
            .current_track()?
            .ok_or_else(|| ControlPointError::QueueError("No current track".to_string()))?;

        drop(backend);

        // Play it using select_queue_track
        self.select_queue_track(track_id)
    }

    /// Plays from the queue at the current position.
    ///
    /// Uses the backend's play_from_queue which preserves the queue for all backends.
    pub fn play_from_queue(&self) -> Result<(), ControlPointError> {
        // Reset the has_played flag before starting playback to prevent
        // auto-advance on transient STOPPED states during track initialization.
        // The flag will be set back to true when PLAYING state is detected.
        self.clear_has_played_flag();

        let backend = self.backend.lock().expect("Backend mutex poisoned");
        backend.play_from_queue()
    }

    // --- Playback State Management ---

    /// Gets the last known track metadata.
    pub fn last_metadata(&self) -> Option<TrackMetadata> {
        self.state.lock().unwrap().last_metadata.clone()
    }

    /// Sets the last known track metadata.
    pub fn set_last_metadata(&self, metadata: Option<TrackMetadata>) {
        self.state.lock().unwrap().last_metadata = metadata;
    }

    /// Gets the current playback source.
    pub fn playback_source(&self) -> PlaybackSource {
        self.state.lock().unwrap().playback_source
    }

    /// Sets the playback source.
    pub fn set_playback_source(&self, source: PlaybackSource) {
        self.state.lock().unwrap().playback_source = source;
    }

    /// Checks if currently playing from queue.
    pub fn is_playing_from_queue(&self) -> bool {
        matches!(
            self.state.lock().unwrap().playback_source,
            PlaybackSource::FromQueue
        )
    }

    /// Marks playback as external if currently idle (source is None).
    pub fn mark_external_if_idle(&self) {
        let mut state = self.state.lock().unwrap();
        if matches!(state.playback_source, PlaybackSource::None) {
            state.playback_source = PlaybackSource::External;
        }
    }

    /// Marks that the user requested a stop (to prevent auto-advance).
    pub fn mark_user_stop_requested(&self) {
        self.state.lock().unwrap().user_stop_requested = true;
    }

    /// Checks and clears the user stop requested flag.
    pub fn check_and_clear_user_stop_requested(&self) -> bool {
        let mut state = self.state.lock().unwrap();
        let was_requested = state.user_stop_requested;
        state.user_stop_requested = false;
        was_requested
    }

    // --- Has-Played Flag Management (for auto-advance protection) ---

    /// Sets the has_played_since_track_start flag to true.
    /// Called when PLAYING state is detected.
    fn set_has_played_flag(&self) {
        self.state.lock().unwrap().has_played_since_track_start = true;
    }

    /// Clears the has_played_since_track_start flag.
    /// Called when stopping playback or starting a new track.
    /// This is public so that ControlPoint can reset it when jumping to a new track.
    pub fn clear_has_played_flag(&self) {
        self.state.lock().unwrap().has_played_since_track_start = false;
    }

    /// Checks and clears the has_played_since_track_start flag.
    /// Returns true if PLAYING was seen since last track start, false otherwise.
    /// Used to determine if auto-advance should be allowed.
    fn check_and_clear_has_played_flag(&self) -> bool {
        let mut state = self.state.lock().unwrap();
        let has_played = state.has_played_since_track_start;
        state.has_played_since_track_start = false;
        has_played
    }

    // --- Sleep Timer Management ---

    /// Starts the sleep timer with the given duration in seconds.
    /// Maximum duration is 2 hours (7200 seconds).
    ///
    /// Returns the remaining seconds after starting.
    ///
    /// # Errors
    /// Returns an error if the duration is invalid (0 or > 7200 seconds).
    pub fn start_sleep_timer(&self, duration_seconds: u32) -> Result<u32, ControlPointError> {
        let mut state = self.state.lock().unwrap();
        state
            .sleep_timer
            .start(duration_seconds)
            .map_err(|e| ControlPointError::ControlPoint(e))?;

        Ok(state.sleep_timer.remaining_seconds().unwrap_or(0))
    }

    /// Updates the sleep timer duration. Resets the timer to the new duration from now.
    ///
    /// Returns the new remaining seconds.
    ///
    /// # Errors
    /// Returns an error if the duration is invalid (0 or > 7200 seconds).
    pub fn update_sleep_timer(&self, duration_seconds: u32) -> Result<u32, ControlPointError> {
        let mut state = self.state.lock().unwrap();
        state
            .sleep_timer
            .update(duration_seconds)
            .map_err(|e| ControlPointError::ControlPoint(e))?;

        Ok(state.sleep_timer.remaining_seconds().unwrap_or(0))
    }

    /// Cancels the sleep timer.
    pub fn cancel_sleep_timer(&self) {
        self.state.lock().unwrap().sleep_timer.cancel();
    }

    /// Returns the remaining seconds of the sleep timer, or None if no timer is active.
    pub fn sleep_timer_remaining(&self) -> Option<u32> {
        self.state.lock().unwrap().sleep_timer.remaining_seconds()
    }

    /// Returns the configured duration of the sleep timer in seconds.
    pub fn sleep_timer_duration(&self) -> u32 {
        self.state.lock().unwrap().sleep_timer.duration_seconds()
    }

    /// Returns true if the sleep timer is active.
    pub fn is_sleep_timer_active(&self) -> bool {
        self.state.lock().unwrap().sleep_timer.is_active()
    }

    /// Returns true if the sleep timer has expired.
    pub fn is_sleep_timer_expired(&self) -> bool {
        self.state.lock().unwrap().sleep_timer.is_expired()
    }

    /// Gets the sleep timer state as a tuple (is_active, duration_seconds, remaining_seconds).
    pub fn sleep_timer_state(&self) -> (bool, u32, Option<u32>) {
        let state = self.state.lock().unwrap();
        (
            state.sleep_timer.is_active(),
            state.sleep_timer.duration_seconds(),
            state.sleep_timer.remaining_seconds(),
        )
    }

    // --- Queue Shuffle ---

    /// Shuffles the current queue and restarts playback from the first track.
    ///
    /// This method:
    /// 1. Detaches the queue from any attached playlist
    /// 2. Stops playback
    /// 3. Takes a snapshot of the current queue
    /// 4. Randomizes the order of tracks
    /// 5. Replaces the queue with the shuffled items
    /// 6. Starts playback from the first track
    ///
    /// # Errors
    /// Returns an error if the queue is empty or if any backend operation fails.
    pub fn shuffle_queue(&self) -> Result<(), ControlPointError> {
        use rand::seq::SliceRandom;
        use rand::thread_rng;

        // 1. Clear the playlist binding (detach from playlist)
        self.clear_playlist_binding();

        // 2. Stop playback (ignore errors if already stopped)
        let _ = self.stop();

        // 3. Get a snapshot of the current queue
        let snapshot = self.queue_snapshot()?;

        if snapshot.items.is_empty() {
            return Err(ControlPointError::QueueError(
                "Cannot shuffle an empty queue".to_string(),
            ));
        }

        // 4. Shuffle the items
        let mut shuffled_items = snapshot.items;
        let mut rng = thread_rng();
        shuffled_items.shuffle(&mut rng);

        // 5. Replace the queue with shuffled items, starting at index 0
        self.replace_queue(shuffled_items, Some(0))?;

        // 6. Start playback from the first track
        self.play_from_index(0)?;

        Ok(())
    }
}

/// Helper function to build DIDL-Lite metadata XML from TrackMetadata
pub(crate) fn build_didl_lite_metadata(
    metadata: &TrackMetadata,
    uri: &str,
    protocol_info: &str,
) -> String {
    format!(
        r#"<DIDL-Lite xmlns="urn:schemas-upnp-org:metadata-1-0/DIDL-Lite/" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:upnp="urn:schemas-upnp-org:metadata-1-0/upnp/">
<item id="0" parentID="-1" restricted="1">
<dc:title>{}</dc:title>
<dc:creator>{}</dc:creator>
<upnp:artist>{}</upnp:artist>
<upnp:album>{}</upnp:album>
{}
<res protocolInfo="{}">{}</res>
</item>
</DIDL-Lite>"#,
        metadata.title.as_deref().unwrap_or("Unknown Title"),
        metadata
            .creator
            .as_deref()
            .or(metadata.artist.as_deref())
            .unwrap_or("Unknown Artist"),
        metadata.artist.as_deref().unwrap_or("Unknown Artist"),
        metadata.album.as_deref().unwrap_or("Unknown Album"),
        metadata
            .album_art_uri
            .as_ref()
            .map(|art_uri| format!("<upnp:albumArtURI>{}</upnp:albumArtURI>", art_uri))
            .unwrap_or_default(),
        protocol_info,
        uri
    )
}

impl DeviceIdentity for MusicRenderer {
    fn id(&self) -> DeviceId {
        self.info.id()
    }

    fn udn(&self) -> &str {
        &*self.info.udn()
    }

    fn friendly_name(&self) -> &str {
        &self.info.friendly_name()
    }

    fn model_name(&self) -> &str {
        &self.info.model_name()
    }
    fn manufacturer(&self) -> &str {
        &self.info.manufacturer()
    }
    fn location(&self) -> &str {
        &self.info.location()
    }
    fn server_header(&self) -> &str {
        &self.info.server_header()
    }
}

impl DeviceOnline for MusicRenderer {
    fn is_online(&self) -> bool {
        self.connection
            .lock()
            .expect("Connection mutex poisoned")
            .is_online()
    }

    fn last_seen(&self) -> SystemTime {
        self.connection
            .lock()
            .expect("Connection mutex poisoned")
            .last_seen()
    }

    fn has_been_seen_now(&self, max_age: u32) {
        let was_online = self.is_online();
        self.connection
            .lock()
            .expect("Connection mutex poisoned")
            .has_been_seen_now(max_age);

        // Start watching if transitioning from offline to online
        if !was_online {
            self.start_watching();
        }
    }

    fn mark_as_offline(&self) {
        // Stop watching before marking offline
        self.stop_watching();
        self.connection
            .lock()
            .expect("Connection mutex poisoned")
            .mark_as_offline()
    }

    fn max_age(&self) -> u32 {
        self.connection
            .lock()
            .expect("Connection mutex poisoned")
            .max_age()
    }
}

impl RendererFromMediaRendererInfo for MusicRendererBackend {
    fn from_renderer_info(info: &RendererInfo) -> Result<Self, ControlPointError> {
        // Try OpenHome first for OpenHome-capable renderers
        match info.protocol() {
            RendererProtocol::OpenHomeOnly | RendererProtocol::OpenHomeHybrid => {
                if let Ok(backend) = OpenHomeRenderer::build_from_renderer_info(info) {
                    return Ok(backend);
                }
                // If OpenHomeOnly failed, it's an error
                if matches!(info.protocol(), RendererProtocol::OpenHomeOnly) {
                    return Err(ControlPointError::MusicRendererBackendBuild(format!(
                        "OpenHomeOnly renderer {} has no usable OpenHome services",
                        info.friendly_name()
                    )));
                }
                // OpenHomeHybrid: fall through to try UPnP-based backends
            }
            RendererProtocol::ChromecastOnly => {
                return ChromecastRenderer::build_from_renderer_info(info);
            }
            RendererProtocol::UpnpAvOnly => {
                // Will be handled below
            }
        }

        // UPnP-based renderers (UpnpAvOnly or OpenHomeHybrid fallback)
        let has_arylic = info.capabilities().has_arylic_tcp;
        let has_avtransport = info.capabilities().has_avtransport();

        // Try Hybrid UPnP + Arylic (special case: combines two backends)
        if has_arylic && has_avtransport {
            let upnp_backend = UpnpRenderer::build_from_renderer_info(info)?;
            if let MusicRendererBackend::Upnp(upnp) = upnp_backend {
                match ArylicTcpRenderer::build_from_renderer_info(info) {
                    Ok(MusicRendererBackend::ArylicTcp(arylic)) => {
                        return Ok(MusicRendererBackend::HybridUpnpArylic { upnp, arylic });
                    }
                    Err(err) => {
                        warn!(
                            renderer = info.friendly_name(),
                            error = %err,
                            "Failed to build Arylic TCP backend, falling back to UPnP only"
                        );
                        return Ok(MusicRendererBackend::Upnp(upnp));
                    }
                    _ => unreachable!(
                        "ArylicTcpRenderer::build_from_renderer_info should return ArylicTcp variant"
                    ),
                }
            } else {
                unreachable!("UpnpRenderer::build_from_renderer_info should return Upnp variant");
            }
        }

        // Try LinkPlay
        if info.capabilities().has_linkplay_http() {
            if let Ok(backend) = LinkPlayRenderer::build_from_renderer_info(info) {
                return Ok(backend);
            }
        }

        // Fallback to UPnP
        if has_avtransport {
            return UpnpRenderer::build_from_renderer_info(info);
        }

        // No suitable backend found
        Err(ControlPointError::MusicRendererBackendBuild(format!(
            "No suitable backend for renderer {}",
            info.friendly_name()
        )))
    }

    fn to_backend(self) -> MusicRendererBackend {
        self
    }
}

/// Transport control façade that dispatches to whichever backend can fulfill
/// the request, returning a standardized error if the backend lacks support.
impl TransportControl for MusicRendererBackend {
    fn play_uri(&self, uri: &str, meta: &str) -> Result<(), ControlPointError> {
        match self {
            MusicRendererBackend::Upnp(upnp) => upnp.play_uri(uri, meta),
            MusicRendererBackend::OpenHome(oh) => oh.play_uri(uri, meta),
            MusicRendererBackend::LinkPlay(lp) => lp.play_uri(uri, meta),
            MusicRendererBackend::ArylicTcp(_) => Err(
                ControlPointError::upnp_operation_not_supported("play_uri", "ArylicTcp"),
            ),
            MusicRendererBackend::Chromecast(cc) => cc.play_uri(uri, meta),
            MusicRendererBackend::HybridUpnpArylic { upnp, .. } => upnp.play_uri(uri, meta),
        }
    }

    fn play(&self) -> Result<(), ControlPointError> {
        match self {
            MusicRendererBackend::Upnp(upnp) => upnp.play(),
            MusicRendererBackend::OpenHome(oh) => oh.play(),
            MusicRendererBackend::LinkPlay(lp) => lp.play(),
            MusicRendererBackend::ArylicTcp(ary) => ary.play(),
            MusicRendererBackend::Chromecast(cc) => cc.play(),
            MusicRendererBackend::HybridUpnpArylic { arylic, .. } => arylic.play(),
        }
    }

    fn pause(&self) -> Result<(), ControlPointError> {
        match self {
            MusicRendererBackend::Upnp(upnp) => upnp.pause(),
            MusicRendererBackend::OpenHome(oh) => oh.pause(),
            MusicRendererBackend::LinkPlay(lp) => lp.pause(),
            MusicRendererBackend::ArylicTcp(ary) => ary.pause(),
            MusicRendererBackend::Chromecast(cc) => cc.pause(),
            MusicRendererBackend::HybridUpnpArylic { arylic, .. } => arylic.pause(),
        }
    }

    fn stop(&self) -> Result<(), ControlPointError> {
        match self {
            MusicRendererBackend::Upnp(upnp) => upnp.stop(),
            MusicRendererBackend::OpenHome(oh) => oh.stop(),
            MusicRendererBackend::LinkPlay(lp) => lp.stop(),
            MusicRendererBackend::ArylicTcp(ary) => ary.stop(),
            MusicRendererBackend::Chromecast(cc) => cc.stop(),
            MusicRendererBackend::HybridUpnpArylic { arylic, .. } => arylic.stop(),
        }
    }

    fn seek_rel_time(&self, hhmmss: &str) -> Result<(), ControlPointError> {
        match self {
            MusicRendererBackend::Upnp(upnp) => upnp.seek_rel_time(hhmmss),
            MusicRendererBackend::OpenHome(oh) => oh.seek_rel_time(hhmmss),
            MusicRendererBackend::LinkPlay(lp) => lp.seek_rel_time(hhmmss),
            MusicRendererBackend::ArylicTcp(_) => Err(
                ControlPointError::upnp_operation_not_supported("seek_rel_time", "ArylicTcp"),
            ),
            MusicRendererBackend::Chromecast(cc) => cc.seek_rel_time(hhmmss),
            MusicRendererBackend::HybridUpnpArylic { upnp, .. } => upnp.seek_rel_time(hhmmss),
        }
    }
}

/// Volume and mute controls exposed via the façade.
///
/// Hybrid backends may read via Arylic TCP and write via UPnP, but callers
/// always depend on a single [`VolumeControl`] entry point.
impl VolumeControl for MusicRendererBackend {
    fn volume(&self) -> Result<u16, ControlPointError> {
        match self {
            MusicRendererBackend::HybridUpnpArylic { arylic, .. } => arylic.volume(),
            MusicRendererBackend::ArylicTcp(ary) => ary.volume(),
            MusicRendererBackend::OpenHome(oh) => oh.volume(),
            MusicRendererBackend::Upnp(upnp) => upnp.volume(),
            MusicRendererBackend::LinkPlay(lp) => lp.volume(),
            MusicRendererBackend::Chromecast(cc) => cc.volume(),
        }
    }

    fn set_volume(&self, vol: u16) -> Result<(), ControlPointError> {
        match self {
            MusicRendererBackend::HybridUpnpArylic { upnp, .. } => upnp.set_volume(vol),
            MusicRendererBackend::ArylicTcp(ary) => ary.set_volume(vol),
            MusicRendererBackend::OpenHome(oh) => oh.set_volume(vol),
            MusicRendererBackend::Upnp(upnp) => upnp.set_volume(vol),
            MusicRendererBackend::LinkPlay(lp) => lp.set_volume(vol),
            MusicRendererBackend::Chromecast(cc) => cc.set_volume(vol),
        }
    }

    fn mute(&self) -> Result<bool, ControlPointError> {
        match self {
            MusicRendererBackend::HybridUpnpArylic { arylic, .. } => arylic.mute(),
            MusicRendererBackend::OpenHome(r) => r.mute(),
            MusicRendererBackend::Upnp(r) => r.mute(),
            MusicRendererBackend::LinkPlay(r) => r.mute(),
            MusicRendererBackend::ArylicTcp(r) => r.mute(),
            MusicRendererBackend::Chromecast(cc) => cc.mute(),
        }
    }

    fn set_mute(&self, m: bool) -> Result<(), ControlPointError> {
        match self {
            MusicRendererBackend::HybridUpnpArylic { arylic, .. } => arylic.set_mute(m),
            MusicRendererBackend::OpenHome(r) => r.set_mute(m),
            MusicRendererBackend::Upnp(r) => r.set_mute(m),
            MusicRendererBackend::LinkPlay(r) => r.set_mute(m),
            MusicRendererBackend::ArylicTcp(r) => r.set_mute(m),
            MusicRendererBackend::Chromecast(cc) => cc.set_mute(m),
        }
    }
}

/// Playback-state queries sourced from the backend best suited for the job.
///
/// Each backend reports into [`PlaybackState`], ensuring consumers never have
/// to reason about protocol-specific state machines.
impl PlaybackStatus for MusicRendererBackend {
    fn playback_state(&self) -> Result<PlaybackState, ControlPointError> {
        match self {
            MusicRendererBackend::Upnp(r) => PlaybackStatus::playback_state(r),
            MusicRendererBackend::OpenHome(r) => PlaybackStatus::playback_state(r),
            MusicRendererBackend::LinkPlay(r) => r.playback_state(),
            MusicRendererBackend::ArylicTcp(r) => r.playback_state(),
            MusicRendererBackend::Chromecast(cc) => cc.playback_state(),
            MusicRendererBackend::HybridUpnpArylic { arylic, .. } => arylic.playback_state(),
        }
    }
}

/// Playback-position queries that always yield a [`PlaybackPositionInfo`]
/// regardless of the backend providing the raw transport data.
impl PlaybackPosition for MusicRendererBackend {
    fn playback_position(&self) -> Result<PlaybackPositionInfo, ControlPointError> {
        match self {
            MusicRendererBackend::Upnp(r) => r.playback_position(),
            MusicRendererBackend::OpenHome(r) => r.playback_position(),
            MusicRendererBackend::LinkPlay(r) => r.playback_position(),
            MusicRendererBackend::ArylicTcp(r) => r.playback_position(),
            MusicRendererBackend::Chromecast(cc) => cc.playback_position(),
            MusicRendererBackend::HybridUpnpArylic { arylic, .. } => arylic.playback_position(),
        }
    }
}

impl RendererBackend for MusicRendererBackend {
    fn queue(&self) -> &Arc<Mutex<MusicQueue>> {
        match self {
            MusicRendererBackend::Upnp(r) => r.queue(),
            MusicRendererBackend::OpenHome(r) => r.queue(),
            MusicRendererBackend::LinkPlay(r) => r.queue(),
            MusicRendererBackend::ArylicTcp(r) => r.queue(),
            MusicRendererBackend::Chromecast(cc) => cc.queue(),
            MusicRendererBackend::HybridUpnpArylic { upnp, .. } => upnp.queue(),
        }
    }
}

impl QueueTransportControl for MusicRendererBackend {
    fn play_from_queue(&self) -> Result<(), ControlPointError> {
        match self {
            MusicRendererBackend::Upnp(r) => r.play_from_queue(),
            MusicRendererBackend::OpenHome(r) => r.play_from_queue(),
            MusicRendererBackend::LinkPlay(r) => r.play_from_queue(),
            MusicRendererBackend::ArylicTcp(r) => r.play_from_queue(),
            MusicRendererBackend::Chromecast(cc) => cc.play_from_queue(),
            MusicRendererBackend::HybridUpnpArylic { upnp, .. } => upnp.play_from_queue(),
        }
    }

    fn play_next(&self) -> Result<(), ControlPointError> {
        match self {
            MusicRendererBackend::Upnp(r) => r.play_next(),
            MusicRendererBackend::OpenHome(r) => r.play_next(),
            MusicRendererBackend::LinkPlay(r) => r.play_next(),
            MusicRendererBackend::ArylicTcp(r) => r.play_next(),
            MusicRendererBackend::Chromecast(cc) => cc.play_next(),
            MusicRendererBackend::HybridUpnpArylic { upnp, .. } => upnp.play_next(),
        }
    }

    fn play_previous(&self) -> Result<(), ControlPointError> {
        match self {
            MusicRendererBackend::Upnp(r) => r.play_previous(),
            MusicRendererBackend::OpenHome(r) => r.play_previous(),
            MusicRendererBackend::LinkPlay(r) => r.play_previous(),
            MusicRendererBackend::ArylicTcp(r) => r.play_previous(),
            MusicRendererBackend::Chromecast(cc) => cc.play_previous(),
            MusicRendererBackend::HybridUpnpArylic { upnp, .. } => upnp.play_previous(),
        }
    }

    fn play_from_index(&self, index: usize) -> Result<(), ControlPointError> {
        match self {
            MusicRendererBackend::Upnp(r) => r.play_from_index(index),
            MusicRendererBackend::OpenHome(r) => r.play_from_index(index),
            MusicRendererBackend::LinkPlay(r) => r.play_from_index(index),
            MusicRendererBackend::ArylicTcp(r) => r.play_from_index(index),
            MusicRendererBackend::Chromecast(cc) => cc.play_from_index(index),
            MusicRendererBackend::HybridUpnpArylic { upnp, .. } => upnp.play_from_index(index),
        }
    }
}

impl QueueBackend for MusicRendererBackend {
    fn len(&self) -> Result<usize, ControlPointError> {
        match self {
            MusicRendererBackend::Upnp(r) => r.len(),
            MusicRendererBackend::OpenHome(r) => r.len(),
            MusicRendererBackend::LinkPlay(r) => r.len(),
            MusicRendererBackend::ArylicTcp(r) => r.len(),
            MusicRendererBackend::Chromecast(cc) => cc.len(),
            MusicRendererBackend::HybridUpnpArylic { upnp, .. } => upnp.len(),
        }
    }

    fn track_ids(&self) -> Result<Vec<u32>, ControlPointError> {
        match self {
            MusicRendererBackend::Upnp(r) => r.track_ids(),
            MusicRendererBackend::OpenHome(r) => r.track_ids(),
            MusicRendererBackend::LinkPlay(r) => r.track_ids(),
            MusicRendererBackend::ArylicTcp(r) => r.track_ids(),
            MusicRendererBackend::Chromecast(cc) => cc.track_ids(),
            MusicRendererBackend::HybridUpnpArylic { upnp, .. } => upnp.track_ids(),
        }
    }

    fn id_to_position(&self, id: u32) -> Result<usize, ControlPointError> {
        match self {
            MusicRendererBackend::Upnp(r) => r.id_to_position(id),
            MusicRendererBackend::OpenHome(r) => r.id_to_position(id),
            MusicRendererBackend::LinkPlay(r) => r.id_to_position(id),
            MusicRendererBackend::ArylicTcp(r) => r.id_to_position(id),
            MusicRendererBackend::Chromecast(cc) => cc.id_to_position(id),
            MusicRendererBackend::HybridUpnpArylic { upnp, .. } => upnp.id_to_position(id),
        }
    }

    fn position_to_id(&self, id: usize) -> Result<u32, ControlPointError> {
        match self {
            MusicRendererBackend::Upnp(r) => r.position_to_id(id),
            MusicRendererBackend::OpenHome(r) => r.position_to_id(id),
            MusicRendererBackend::LinkPlay(r) => r.position_to_id(id),
            MusicRendererBackend::ArylicTcp(r) => r.position_to_id(id),
            MusicRendererBackend::Chromecast(cc) => cc.position_to_id(id),
            MusicRendererBackend::HybridUpnpArylic { upnp, .. } => upnp.position_to_id(id),
        }
    }

    fn current_track(&self) -> Result<Option<u32>, ControlPointError> {
        match self {
            MusicRendererBackend::Upnp(r) => r.current_track(),
            MusicRendererBackend::OpenHome(r) => r.current_track(),
            MusicRendererBackend::LinkPlay(r) => r.current_track(),
            MusicRendererBackend::ArylicTcp(r) => r.current_track(),
            MusicRendererBackend::Chromecast(cc) => cc.current_track(),
            MusicRendererBackend::HybridUpnpArylic { upnp, .. } => upnp.current_track(),
        }
    }

    fn current_index(&self) -> Result<Option<usize>, ControlPointError> {
        match self {
            MusicRendererBackend::Upnp(r) => r.current_index(),
            MusicRendererBackend::OpenHome(r) => r.current_index(),
            MusicRendererBackend::LinkPlay(r) => r.current_index(),
            MusicRendererBackend::ArylicTcp(r) => r.current_index(),
            MusicRendererBackend::Chromecast(cc) => cc.current_index(),
            MusicRendererBackend::HybridUpnpArylic { upnp, .. } => upnp.current_index(),
        }
    }

    fn queue_snapshot(&self) -> Result<QueueSnapshot, ControlPointError> {
        match self {
            MusicRendererBackend::Upnp(r) => r.queue_snapshot(),
            MusicRendererBackend::OpenHome(r) => r.queue_snapshot(),
            MusicRendererBackend::LinkPlay(r) => r.queue_snapshot(),
            MusicRendererBackend::ArylicTcp(r) => r.queue_snapshot(),
            MusicRendererBackend::Chromecast(cc) => cc.queue_snapshot(),
            MusicRendererBackend::HybridUpnpArylic { upnp, .. } => upnp.queue_snapshot(),
        }
    }

    fn set_index(&mut self, index: Option<usize>) -> Result<(), ControlPointError> {
        match self {
            MusicRendererBackend::Upnp(r) => r.set_index(index),
            MusicRendererBackend::OpenHome(r) => r.set_index(index),
            MusicRendererBackend::LinkPlay(r) => r.set_index(index),
            MusicRendererBackend::ArylicTcp(r) => r.set_index(index),
            MusicRendererBackend::Chromecast(cc) => cc.set_index(index),
            MusicRendererBackend::HybridUpnpArylic { upnp, .. } => upnp.set_index(index),
        }
    }

    fn replace_queue(
        &mut self,
        items: Vec<PlaybackItem>,
        current_index: Option<usize>,
    ) -> Result<(), ControlPointError> {
        match self {
            MusicRendererBackend::Upnp(r) => r.replace_queue(items, current_index),
            MusicRendererBackend::OpenHome(r) => r.replace_queue(items, current_index),
            MusicRendererBackend::LinkPlay(r) => r.replace_queue(items, current_index),
            MusicRendererBackend::ArylicTcp(r) => r.replace_queue(items, current_index),
            MusicRendererBackend::Chromecast(cc) => cc.replace_queue(items, current_index),
            MusicRendererBackend::HybridUpnpArylic { upnp, .. } => {
                upnp.replace_queue(items, current_index)
            }
        }
    }

    fn sync_queue(&mut self, items: Vec<PlaybackItem>) -> Result<(), ControlPointError> {
        match self {
            MusicRendererBackend::Upnp(r) => r.sync_queue(items),
            MusicRendererBackend::OpenHome(r) => r.sync_queue(items),
            MusicRendererBackend::LinkPlay(r) => r.sync_queue(items),
            MusicRendererBackend::ArylicTcp(r) => r.sync_queue(items),
            MusicRendererBackend::Chromecast(cc) => cc.sync_queue(items),
            MusicRendererBackend::HybridUpnpArylic { upnp, .. } => upnp.sync_queue(items),
        }
    }

    fn get_item(&self, index: usize) -> Result<Option<PlaybackItem>, ControlPointError> {
        match self {
            MusicRendererBackend::Upnp(r) => r.get_item(index),
            MusicRendererBackend::OpenHome(r) => r.get_item(index),
            MusicRendererBackend::LinkPlay(r) => r.get_item(index),
            MusicRendererBackend::ArylicTcp(r) => r.get_item(index),
            MusicRendererBackend::Chromecast(cc) => cc.get_item(index),
            MusicRendererBackend::HybridUpnpArylic { upnp, .. } => upnp.get_item(index),
        }
    }

    fn replace_item(&mut self, index: usize, item: PlaybackItem) -> Result<(), ControlPointError> {
        match self {
            MusicRendererBackend::Upnp(r) => r.replace_item(index, item),
            MusicRendererBackend::OpenHome(r) => r.replace_item(index, item),
            MusicRendererBackend::LinkPlay(r) => r.replace_item(index, item),
            MusicRendererBackend::ArylicTcp(r) => r.replace_item(index, item),
            MusicRendererBackend::Chromecast(cc) => cc.replace_item(index, item),
            MusicRendererBackend::HybridUpnpArylic { upnp, .. } => upnp.replace_item(index, item),
        }
    }

    fn enqueue_items(
        &mut self,
        items: Vec<PlaybackItem>,
        mode: EnqueueMode,
    ) -> Result<(), ControlPointError> {
        match self {
            MusicRendererBackend::Upnp(r) => r.enqueue_items(items, mode),
            MusicRendererBackend::OpenHome(r) => r.enqueue_items(items, mode),
            MusicRendererBackend::LinkPlay(r) => r.enqueue_items(items, mode),
            MusicRendererBackend::ArylicTcp(r) => r.enqueue_items(items, mode),
            MusicRendererBackend::Chromecast(cc) => cc.enqueue_items(items, mode),
            MusicRendererBackend::HybridUpnpArylic { upnp, .. } => upnp.enqueue_items(items, mode),
        }
    }
}
