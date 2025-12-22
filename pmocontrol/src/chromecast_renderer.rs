//! Chromecast backend implementation using the cast-sender library.
//!
//! This module provides a `ChromecastRenderer` that implements the standard
//! transport and volume control traits, allowing Chromecast devices to be
//! controlled through the same interface as UPnP, OpenHome, and other backends.
//!
//! ## Architecture
//!
//! Uses `cast-sender`, a fully asynchronous Chromecast library that handles
//! heartbeats and connection management automatically. The async operations
//! are wrapped in sync calls using smol::block_on for compatibility with
//! the existing sync trait interfaces.

use std::sync::{Arc, Mutex, Once};
use std::thread::JoinHandle;

use anyhow::{Result, anyhow};

use tracing::debug;

use crate::capabilities::{
    PlaybackPosition, PlaybackPositionInfo, PlaybackState, PlaybackStatus, TransportControl,
    VolumeControl,
};
use crate::chromecast_discovery::{extract_host_from_location, extract_port_from_location};
use crate::model::{RendererId, RendererInfo, RendererProtocol};

use rust_cast::{
    CastDevice, ChannelMessage,
    channels::{
        heartbeat::HeartbeatResponse,
        media::{Media, PlayerState as CastPlayerState, StreamType},
        receiver::CastDeviceApp,
    },
};

const DEFAULT_DESTINATION_ID: &str = "receiver-0";

/// Default Chromecast port.
const DEFAULT_CHROMECAST_PORT: u16 = 8009;

/// Chromecast renderer backend.
///
/// Uses the rust_cast library to communicate with Chromecast devices
/// via the Cast protocol. For play operations, a dedicated thread is
/// spawned to handle heartbeat responses from the device.
#[derive(Clone)]
pub struct ChromecastRenderer {
    pub info: RendererInfo,
    host: String,
    port: u16,
    stop_signal: Arc<Mutex<bool>>,
    /// Handle to the active heartbeat thread, if any.
    /// Wrapped in Arc<Mutex> to allow cloning and proper thread lifecycle management.
    thread_handle: Arc<Mutex<Option<JoinHandle<()>>>>,
}

impl std::fmt::Debug for ChromecastRenderer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChromecastRenderer")
            .field("info", &self.info)
            .field("host", &self.host)
            .field("port", &self.port)
            .finish()
    }
}

/// Ensures the Rustls CryptoProvider is initialized exactly once.
///
/// This is required by rust_cast which uses rustls for TLS connections.
/// Without this, rust_cast will panic with:
/// "Could not automatically determine the process-level CryptoProvider"
fn ensure_crypto_provider_initialized() {
    static INIT: Once = Once::new();

    INIT.call_once(|| {
        // Install the default CryptoProvider (aws-lc-rs or ring, depending on features)
        let _ = rustls::crypto::CryptoProvider::install_default(
            rustls::crypto::aws_lc_rs::default_provider()
        );
        tracing::debug!("Rustls CryptoProvider initialized for Chromecast connections");
    });
}

/// Helper function to connect to a Chromecast device.
fn connect_to_device<'a>(host: &'a str, port: u16) -> Result<CastDevice<'a>> {
    // Ensure rustls crypto provider is initialized before any TLS connection
    ensure_crypto_provider_initialized();

    let device = CastDevice::connect_without_host_verification(host, port)
        .map_err(|e| anyhow!("Failed to connect to Chromecast: {}", e))?;

    device.connection
        .connect(DEFAULT_DESTINATION_ID.to_string())
        .map_err(|e| anyhow!("Failed to connect channel: {}", e))?;

    Ok(device)
}

/// Maps Chromecast PlayerState to our PlaybackState.
fn map_player_state(player_state: &CastPlayerState) -> PlaybackState {
    match player_state {
        CastPlayerState::Idle => PlaybackState::Stopped,
        CastPlayerState::Playing => PlaybackState::Playing,
        CastPlayerState::Buffering => PlaybackState::Transitioning,
        CastPlayerState::Paused => PlaybackState::Paused,
    }
}

impl ChromecastRenderer {
    /// Creates a new ChromecastRenderer from RendererInfo.
    pub fn from_renderer_info(info: RendererInfo) -> Result<Self> {
        tracing::info!(
            "ChromecastRenderer::from_renderer_info location={} for {}",
            info.location,
            info.friendly_name
        );

        let host = extract_host_from_location(&info.location)
            .ok_or_else(|| anyhow!("Invalid Chromecast location: {}", info.location))?;

        let port = extract_port_from_location(&info.location)
            .unwrap_or(DEFAULT_CHROMECAST_PORT);

        let stop_signal = Arc::new(Mutex::new(false));
        let thread_handle = Arc::new(Mutex::new(None));

        tracing::info!(
            "ChromecastRenderer created for {} with host={} port={}",
            info.friendly_name,
            host,
            port
        );

        Ok(Self {
            info,
            host,
            port,
            stop_signal,
            thread_handle,
        })
    }
    /// Returns the renderer ID.
    pub fn id(&self) -> &RendererId {
        &self.info.id
    }

    /// Returns the friendly name.
    pub fn friendly_name(&self) -> &str {
        &self.info.friendly_name
    }

    /// Returns the protocol.
    pub fn protocol(&self) -> &RendererProtocol {
        &self.info.protocol
    }

    /// Returns the renderer info.
    pub fn info(&self) -> &RendererInfo {
        &self.info
    }
}

impl TransportControl for ChromecastRenderer {
    fn play_uri(&self, uri: &str, meta: &str) -> Result<()> {
        debug!("ChromecastRenderer: play_uri({})", uri);

        // Signal any existing play thread to stop
        if let Ok(mut stop) = self.stop_signal.lock() {
            *stop = true;
        }

        // Wait for the previous thread to finish (with timeout)
        if let Ok(mut handle_guard) = self.thread_handle.lock() {
            if let Some(handle) = handle_guard.take() {
                // Release the lock before joining to avoid deadlock
                drop(handle_guard);

                // Wait for thread to finish (it should see stop_signal and exit)
                // Note: device.receive() may block, so thread might take time to notice stop_signal
                let join_result = std::thread::spawn(move || handle.join())
                    .join();

                match join_result {
                    Ok(Ok(())) => {
                        tracing::debug!("Previous heartbeat thread stopped cleanly");
                    }
                    Ok(Err(_)) => {
                        tracing::warn!("Previous heartbeat thread panicked");
                    }
                    Err(_) => {
                        tracing::error!("Failed to join previous heartbeat thread");
                    }
                }
            }
        }

        // Reset stop signal
        if let Ok(mut stop) = self.stop_signal.lock() {
            *stop = false;
        }

        // Launch a new play thread
        let host = self.host.clone();
        let port = self.port;
        let uri = uri.to_string();
        let meta = meta.to_string();
        let stop_signal = self.stop_signal.clone();

        let handle = std::thread::spawn(move || {
            tracing::info!("Play thread starting for URI: {}", uri);

            let device = match connect_to_device(&host, port) {
                Ok(d) => d,
                Err(e) => {
                    tracing::error!("Failed to connect in play thread: {}", e);
                    return;
                }
            };

            // Launch DefaultMediaReceiver app
            let app = match device.receiver.launch_app(&CastDeviceApp::DefaultMediaReceiver) {
                Ok(app) => app,
                Err(e) => {
                    tracing::error!("Failed to launch DefaultMediaReceiver: {}", e);
                    return;
                }
            };

            // Connect to the app's transport
            if let Err(e) = device.connection.connect(app.transport_id.as_str()) {
                tracing::error!("Failed to connect to app transport: {}", e);
                return;
            }

            // Load the media
            let content_type = detect_content_type_from_meta(&uri, &meta);
            let media = Media {
                content_id: uri.clone(),
                content_type,
                stream_type: StreamType::Buffered,
                duration: None,
                metadata: None,
            };

            match device.media.load(
                app.transport_id.as_str(),
                app.session_id.as_str(),
                &media,
            ) {
                Ok(status) => {
                    tracing::info!("Media loaded successfully: {:?}", status);
                }
                Err(e) => {
                    tracing::error!("Failed to load media: {}", e);
                    return;
                }
            }

            // Main loop: receive messages and respond to heartbeats
            loop {
                // Check stop signal
                if let Ok(stop) = stop_signal.lock() {
                    if *stop {
                        tracing::info!("Play thread stopping (stop signal received)");
                        break;
                    }
                }

                match device.receive() {
                    Ok(ChannelMessage::Heartbeat(response)) => {
                        tracing::trace!("[Heartbeat] {:?}", response);
                        if let HeartbeatResponse::Ping = response {
                            if let Err(e) = device.heartbeat.pong() {
                                tracing::error!("Failed to send heartbeat pong: {:?}", e);
                                break;
                            }
                        }
                    }
                    Ok(ChannelMessage::Media(response)) => {
                        tracing::debug!("[Media] {:?}", response);
                        // TODO: Update state from media messages
                    }
                    Ok(ChannelMessage::Receiver(response)) => {
                        tracing::debug!("[Receiver] {:?}", response);
                    }
                    Ok(ChannelMessage::Connection(response)) => {
                        tracing::trace!("[Connection] {:?}", response);
                    }
                    Ok(ChannelMessage::Raw(response)) => {
                        tracing::trace!("[Raw] {:?}", response);
                    }
                    Err(e) => {
                        tracing::error!("Error receiving message: {:?}", e);
                        break;
                    }
                }
            }

            tracing::info!("Play thread stopped");
        });

        // Store the thread handle for proper cleanup
        if let Ok(mut handle_guard) = self.thread_handle.lock() {
            *handle_guard = Some(handle);
        }

        Ok(())
    }

    fn play(&self) -> Result<()> {
        debug!("ChromecastRenderer: play()");

        let device = connect_to_device(&self.host, self.port)?;

        // Get receiver status to find the active app
        let status = device.receiver.get_status()
            .map_err(|e| anyhow!("Failed to get receiver status: {}", e))?;

        let app = status.applications.first()
            .ok_or_else(|| anyhow!("No active app found"))?;

        // Connect to the app
        device.connection.connect(app.transport_id.as_str())
            .map_err(|e| anyhow!("Failed to connect to app: {}", e))?;

        // Get media status
        let media_status = device.media.get_status(app.transport_id.as_str(), None)
            .map_err(|e| anyhow!("Failed to get media status: {}", e))?;

        let media_entry = media_status.entries.first()
            .ok_or_else(|| anyhow!("No media session found"))?;

        // Send play command
        device.media.play(app.transport_id.as_str(), media_entry.media_session_id)
            .map_err(|e| anyhow!("Failed to play: {}", e))?;

        Ok(())
    }

    fn pause(&self) -> Result<()> {
        debug!("ChromecastRenderer: pause()");

        let device = connect_to_device(&self.host, self.port)?;

        let status = device.receiver.get_status()
            .map_err(|e| anyhow!("Failed to get receiver status: {}", e))?;

        let app = status.applications.first()
            .ok_or_else(|| anyhow!("No active app found"))?;

        device.connection.connect(app.transport_id.as_str())
            .map_err(|e| anyhow!("Failed to connect to app: {}", e))?;

        let media_status = device.media.get_status(app.transport_id.as_str(), None)
            .map_err(|e| anyhow!("Failed to get media status: {}", e))?;

        let media_entry = media_status.entries.first()
            .ok_or_else(|| anyhow!("No media session found"))?;

        device.media.pause(app.transport_id.as_str(), media_entry.media_session_id)
            .map_err(|e| anyhow!("Failed to pause: {}", e))?;

        Ok(())
    }

    fn stop(&self) -> Result<()> {
        debug!("ChromecastRenderer: stop()");

        // Signal the play thread to stop
        if let Ok(mut stop) = self.stop_signal.lock() {
            *stop = true;
        }

        // Note: We don't wait for the thread here as stop() should be quick.
        // The thread will terminate on its own when it checks stop_signal.
        // If a new play_uri() is called, it will properly wait for this thread.

        // Also send stop command to the device
        let device = connect_to_device(&self.host, self.port)?;

        let status = device.receiver.get_status()
            .map_err(|e| anyhow!("Failed to get receiver status: {}", e))?;

        let app = status.applications.first()
            .ok_or_else(|| anyhow!("No active app found"))?;

        device.connection.connect(app.transport_id.as_str())
            .map_err(|e| anyhow!("Failed to connect to app: {}", e))?;

        let media_status = device.media.get_status(app.transport_id.as_str(), None)
            .map_err(|e| anyhow!("Failed to get media status: {}", e))?;

        let media_entry = media_status.entries.first()
            .ok_or_else(|| anyhow!("No media session found"))?;

        device.media.stop(app.transport_id.as_str(), media_entry.media_session_id)
            .map_err(|e| anyhow!("Failed to stop: {}", e))?;

        Ok(())
    }

    fn seek_rel_time(&self, hhmmss: &str) -> Result<()> {
        debug!("ChromecastRenderer: seek_rel_time({})", hhmmss);

        // Parse HH:MM:SS to seconds
        let parts: Vec<&str> = hhmmss.split(':').collect();
        if parts.len() != 3 {
            return Err(anyhow!("Invalid time format, expected HH:MM:SS: {}", hhmmss));
        }

        let hours: u32 = parts[0].parse()
            .map_err(|_| anyhow!("Invalid hours in time: {}", hhmmss))?;
        let minutes: u32 = parts[1].parse()
            .map_err(|_| anyhow!("Invalid minutes in time: {}", hhmmss))?;
        let seconds: u32 = parts[2].parse()
            .map_err(|_| anyhow!("Invalid seconds in time: {}", hhmmss))?;

        let total_seconds = (hours * 3600 + minutes * 60 + seconds) as f32;

        let device = connect_to_device(&self.host, self.port)?;

        let status = device.receiver.get_status()
            .map_err(|e| anyhow!("Failed to get receiver status: {}", e))?;

        let app = status.applications.first()
            .ok_or_else(|| anyhow!("No active app found"))?;

        device.connection.connect(app.transport_id.as_str())
            .map_err(|e| anyhow!("Failed to connect to app: {}", e))?;

        let media_status = device.media.get_status(app.transport_id.as_str(), None)
            .map_err(|e| anyhow!("Failed to get media status: {}", e))?;

        let media_entry = media_status.entries.first()
            .ok_or_else(|| anyhow!("No media session found"))?;

        device.media.seek(
            app.transport_id.as_str(),
            media_entry.media_session_id,
            Some(total_seconds),
            None,
        )
        .map_err(|e| anyhow!("Failed to seek: {}", e))?;

        Ok(())
    }
}

impl PlaybackStatus for ChromecastRenderer {
    fn playback_state(&self) -> Result<PlaybackState> {
        let device = connect_to_device(&self.host, self.port)?;

        // Get receiver status to find the active app
        let status = device.receiver.get_status()
            .map_err(|e| anyhow!("Failed to get receiver status: {}", e))?;

        // If no app is running, return NoMedia
        let app = match status.applications.first() {
            Some(app) => app,
            None => return Ok(PlaybackState::NoMedia),
        };

        // Connect to the app
        device.connection.connect(app.transport_id.as_str())
            .map_err(|e| anyhow!("Failed to connect to app: {}", e))?;

        // Get media status
        let media_status = device.media.get_status(app.transport_id.as_str(), None)
            .map_err(|e| anyhow!("Failed to get media status: {}", e))?;

        // If no media entry, return NoMedia
        let media_entry = match media_status.entries.first() {
            Some(entry) => entry,
            None => return Ok(PlaybackState::NoMedia),
        };

        Ok(map_player_state(&media_entry.player_state))
    }
}

impl PlaybackPosition for ChromecastRenderer {
    fn playback_position(&self) -> Result<PlaybackPositionInfo> {
        let device = connect_to_device(&self.host, self.port)?;

        // Get receiver status to find the active app
        let status = device.receiver.get_status()
            .map_err(|e| anyhow!("Failed to get receiver status: {}", e))?;

        let app = status.applications.first()
            .ok_or_else(|| anyhow!("No active app found"))?;

        // Connect to the app
        device.connection.connect(app.transport_id.as_str())
            .map_err(|e| anyhow!("Failed to connect to app: {}", e))?;

        // Get media status
        let media_status = device.media.get_status(app.transport_id.as_str(), None)
            .map_err(|e| anyhow!("Failed to get media status: {}", e))?;

        let media_entry = media_status.entries.first()
            .ok_or_else(|| anyhow!("No media session found"))?;

        // Extract position information
        let rel_time = media_entry.current_time
            .map(|time| format_time_hhmmss(time as f64));

        let track_duration = media_entry.media.as_ref()
            .and_then(|m| m.duration)
            .map(|dur| format_time_hhmmss(dur as f64));

        let track_uri = media_entry.media.as_ref()
            .map(|m| m.content_id.clone());

        Ok(PlaybackPositionInfo {
            track: Some(1),
            rel_time,
            abs_time: None,
            track_duration,
            track_metadata: None, // Chromecast doesn't use DIDL-Lite
            track_uri,
        })
    }
}

/// Detects the MIME content type from DIDL-Lite metadata or URI.
///
/// The UPnP protocol_info format is: "protocol:*:contentFormat:*"
/// For example: "http-get:*:audio/flac:*"
///
/// This function:
/// 1. Tries to parse DIDL-Lite metadata and extract protocolInfo
/// 2. Falls back to detecting from URI file extension
/// 3. Returns "audio/*" as a last resort
fn detect_content_type_from_meta(uri: &str, meta: &str) -> String {
    use pmodidl::MediaMetadataParser;

    // Try to parse DIDL-Lite metadata
    if !meta.is_empty() {
        if let Ok(didl) = pmodidl::DIDLLite::parse(meta) {
            // Get the first audio resource
            if let Some(item) = didl.items.first() {
                if let Some(resource) = item.audio_resources().next() {
                    // Protocol info format: "protocol:*:contentFormat:*"
                    // Extract the third field (content format / MIME type)
                    let parts: Vec<&str> = resource.protocol_info.split(':').collect();
                    if parts.len() >= 3 {
                        let content_type = parts[2].trim();
                        if !content_type.is_empty() && content_type != "*" {
                            tracing::debug!(
                                "Detected content type '{}' from DIDL-Lite metadata",
                                content_type
                            );
                            return content_type.to_string();
                        }
                    }
                }
            }
        }
    }

    // Fallback: try to detect from URI file extension
    let path = uri.split('?').next().unwrap_or(uri);
    let extension = path.split('.').last().unwrap_or("").to_lowercase();

    let content_type = match extension.as_str() {
        "flac" => "audio/flac",
        "mp3" => "audio/mpeg",
        "m4a" | "mp4" | "aac" => "audio/mp4",
        "ogg" => "audio/ogg",
        "opus" => "audio/opus",
        "wav" => "audio/wav",
        "weba" | "webm" => "audio/webm",
        "oga" => "audio/ogg",
        _ => {
            // Default to generic audio type
            tracing::debug!(
                "Could not detect content type from metadata or URI extension, using audio/*"
            );
            "audio/*"
        }
    };

    content_type.to_string()
}

/// Converts seconds to HH:MM:SS format.
fn format_time_hhmmss(seconds: f64) -> String {
    let total_secs = seconds as u64;
    let hours = total_secs / 3600;
    let minutes = (total_secs % 3600) / 60;
    let secs = total_secs % 60;
    format!("{:02}:{:02}:{:02}", hours, minutes, secs)
}

impl VolumeControl for ChromecastRenderer {
    fn volume(&self) -> Result<u16> {
        let device = connect_to_device(&self.host, self.port)?;

        let status = device.receiver.get_status()
            .map_err(|e| anyhow!("Failed to get receiver status: {}", e))?;

        if let Some(level) = status.volume.level {
            Ok((level * 100.0) as u16)
        } else {
            Ok(50) // Default volume
        }
    }

    fn set_volume(&self, volume: u16) -> Result<()> {
        debug!("ChromecastRenderer: set_volume({})", volume);

        let device = connect_to_device(&self.host, self.port)?;

        let level = (volume as f32) / 100.0;
        device.receiver.set_volume(level)
            .map_err(|e| anyhow!("Failed to set volume: {}", e))?;

        Ok(())
    }

    fn mute(&self) -> Result<bool> {
        let device = connect_to_device(&self.host, self.port)?;

        let status = device.receiver.get_status()
            .map_err(|e| anyhow!("Failed to get receiver status: {}", e))?;

        Ok(status.volume.muted.unwrap_or(false))
    }

    fn set_mute(&self, mute: bool) -> Result<()> {
        debug!("ChromecastRenderer: set_mute({})", mute);

        let device = connect_to_device(&self.host, self.port)?;

        device.receiver.set_volume(mute)
            .map_err(|e| anyhow!("Failed to set mute: {}", e))?;

        Ok(())
    }
}
