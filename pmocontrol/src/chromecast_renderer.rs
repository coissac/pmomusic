//! Chromecast backend implementation using the rust_cast library.
//!
//! This module provides a `ChromecastRenderer` that implements the standard
//! transport and volume control traits, allowing Chromecast devices to be
//! controlled through the same interface as UPnP, OpenHome, and other backends.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{anyhow, Result};
use rust_cast::channels::media::{Image, Media, Metadata, MusicTrackMediaMetadata, StreamType};
use rust_cast::channels::receiver::CastDeviceApp;
use rust_cast::CastDevice;
use tracing::debug;

use crate::capabilities::{
    PlaybackPosition, PlaybackPositionInfo, PlaybackState, PlaybackStatus, TransportControl,
    VolumeControl,
};
use crate::chromecast_discovery::{extract_host_from_location, extract_port_from_location};
use crate::model::{RendererInfo, RendererId, RendererProtocol};
use crate::openhome_client::parse_track_metadata_from_didl;

/// Default Chromecast port.
const DEFAULT_CHROMECAST_PORT: u16 = 8009;

/// Default timeout for Chromecast operations.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);

/// Session state for a Chromecast connection.
///
/// This tracks session IDs and cached status to enable efficient
/// communication with the Chromecast device.
/// Note: We don't store the connection itself to avoid lifetime issues.
#[derive(Debug)]
struct ChromecastSessionState {
    /// The receiver session ID obtained when launching an app.
    receiver_session_id: Option<String>,

    /// The media session ID obtained when loading media.
    media_session_id: Option<i32>,

    /// The destination transport ID (usually "web-0").
    destination_id: Option<String>,
}

impl ChromecastSessionState {
    fn new() -> Self {
        Self {
            receiver_session_id: None,
            media_session_id: None,
            destination_id: None,
        }
    }

    /// Clears all session state.
    fn clear(&mut self) {
        self.receiver_session_id = None;
        self.media_session_id = None;
        self.destination_id = None;
    }
}

/// Chromecast renderer backend.
///
/// Uses the rust_cast library to communicate with Chromecast devices
/// via the Cast protocol (Protocol Buffers over TLS).
#[derive(Clone, Debug)]
pub struct ChromecastRenderer {
    pub info: RendererInfo,
    host: String,
    port: u16,
    session_state: Arc<Mutex<ChromecastSessionState>>,
    timeout: Duration,
}

impl ChromecastRenderer {
    /// Creates a new ChromecastRenderer from RendererInfo.
    pub fn from_renderer_info(info: RendererInfo) -> Result<Self> {
        // Extract host and port from the location URL
        let host = extract_host_from_location(&info.location)
            .ok_or_else(|| anyhow!("Invalid Chromecast location: {}", info.location))?;

        let port = extract_port_from_location(&info.location)
            .unwrap_or(DEFAULT_CHROMECAST_PORT);

        debug!(
            "Creating ChromecastRenderer for {} at {}:{}",
            info.friendly_name, host, port
        );

        Ok(Self {
            info,
            host,
            port,
            session_state: Arc::new(Mutex::new(ChromecastSessionState::new())),
            timeout: DEFAULT_TIMEOUT,
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

    /// Creates a new connection to the Chromecast device.
    ///
    /// This creates a fresh connection each time to avoid lifetime issues.
    fn connect(&self) -> Result<CastDevice<'_>> {
        debug!("Connecting to Chromecast at {}:{}", self.host, self.port);

        let device = CastDevice::connect(&self.host, self.port)
            .map_err(|e| anyhow!("Failed to connect to Chromecast: {}", e))?;

        debug!("Successfully connected to Chromecast");
        Ok(device)
    }

    /// Ensures a receiver session exists by launching the Default Media Receiver app.
    ///
    /// This must be called before any media operations.
    /// Returns a new connection with the session already established.
    fn ensure_session(&self) -> Result<CastDevice<'_>> {
        let device = self.connect()?;

        let mut state = self.session_state.lock()
            .map_err(|e| anyhow!("Failed to acquire session state lock: {}", e))?;

        if state.receiver_session_id.is_none() {
            debug!("Launching Default Media Receiver app");

            let app = device.receiver.launch_app(&CastDeviceApp::DefaultMediaReceiver)
                .map_err(|e| anyhow!("Failed to launch app: {}", e))?;

            state.receiver_session_id = Some(app.session_id.clone());
            state.destination_id = Some(app.transport_id.clone());

            debug!(
                "Launched app with session_id: {}, transport_id: {}",
                app.session_id, app.transport_id
            );
        }

        Ok(device)
    }

    /// Converts DIDL-Lite metadata to rust_cast Media format.
    fn build_media_from_didl(&self, uri: &str, didl_xml: &str) -> Result<Media> {
        // Parse DIDL-Lite metadata
        let metadata = parse_track_metadata_from_didl(didl_xml)
            .unwrap_or_else(|| crate::model::TrackMetadata {
                title: None,
                artist: None,
                album: None,
                genre: None,
                album_art_uri: None,
                date: None,
                track_number: None,
                creator: None,
            });

        // Build music track metadata
        let images = metadata.album_art_uri
            .map(|uri| vec![Image { url: uri, dimensions: None }])
            .unwrap_or_default();

        let music_metadata = MusicTrackMediaMetadata {
            title: metadata.title,
            artist: metadata.artist,
            album_name: metadata.album,
            images,
            release_date: metadata.date,
            ..Default::default()
        };

        // Detect content type from URI
        let content_type = if uri.ends_with(".flac") {
            "audio/flac"
        } else if uri.ends_with(".mp3") {
            "audio/mpeg"
        } else if uri.ends_with(".ogg") || uri.ends_with(".oga") {
            "audio/ogg"
        } else if uri.ends_with(".m4a") || uri.ends_with(".aac") {
            "audio/mp4"
        } else {
            "audio/flac" // Default to FLAC
        }.to_string();

        Ok(Media {
            content_id: uri.to_string(),
            content_type,
            stream_type: StreamType::Buffered,
            metadata: Some(Metadata::MusicTrack(music_metadata)),
            duration: None, // Will be populated from status
        })
    }

    /// Gets the current media status from the Chromecast.
    fn get_media_status(&self) -> Result<rust_cast::channels::media::Status> {
        let device = self.connect()?;

        let state = self.session_state.lock()
            .map_err(|e| anyhow!("Failed to acquire session state lock: {}", e))?;

        let destination_id = state.destination_id.as_ref()
            .ok_or_else(|| anyhow!("No destination ID available"))?
            .clone();

        let media_session_id = state.media_session_id;

        drop(state);

        device.media.get_status(destination_id, media_session_id)
            .map_err(|e| anyhow!("Failed to get media status: {}", e))
    }

    /// Parses a HH:MM:SS time string to seconds.
    fn parse_hhmmss_to_seconds(hhmmss: &str) -> Result<f64> {
        let parts: Vec<&str> = hhmmss.split(':').collect();
        match parts.len() {
            3 => {
                let hours: f64 = parts[0].parse()
                    .map_err(|_| anyhow!("Invalid hours in time format"))?;
                let minutes: f64 = parts[1].parse()
                    .map_err(|_| anyhow!("Invalid minutes in time format"))?;
                let seconds: f64 = parts[2].parse()
                    .map_err(|_| anyhow!("Invalid seconds in time format"))?;
                Ok(hours * 3600.0 + minutes * 60.0 + seconds)
            }
            _ => Err(anyhow!("Invalid time format, expected HH:MM:SS")),
        }
    }

    /// Formats seconds to HH:MM:SS string.
    fn format_seconds_to_hhmmss(seconds: f64) -> String {
        let h = (seconds / 3600.0).floor() as u32;
        let m = ((seconds % 3600.0) / 60.0).floor() as u32;
        let s = (seconds % 60.0).floor() as u32;
        format!("{:02}:{:02}:{:02}", h, m, s)
    }
}

impl TransportControl for ChromecastRenderer {
    fn play_uri(&self, uri: &str, meta: &str) -> Result<()> {
        debug!("ChromecastRenderer: play_uri({})", uri);

        // Ensure we have a session and get a connection
        let device = self.ensure_session()?;

        // Build media from DIDL metadata
        let media = self.build_media_from_didl(uri, meta)?;

        // Get session IDs
        let state = self.session_state.lock()
            .map_err(|e| anyhow!("Failed to acquire session state lock: {}", e))?;

        let destination_id = state.destination_id.as_ref()
            .ok_or_else(|| anyhow!("No destination ID available"))?
            .clone();

        let session_id = state.receiver_session_id.as_ref()
            .ok_or_else(|| anyhow!("No receiver session ID available"))?
            .clone();

        // Drop the lock before calling device methods
        drop(state);

        let status = device.media.load(
            &destination_id,
            &session_id,
            &media,
        ).map_err(|e| anyhow!("Failed to load media: {}", e))?;

        // Cache media session ID
        let mut state = self.session_state.lock()
            .map_err(|e| anyhow!("Failed to acquire session state lock: {}", e))?;

        if let Some(entry) = status.entries.first() {
            state.media_session_id = Some(entry.media_session_id);
            debug!("Media loaded with session ID: {}", entry.media_session_id);
        }

        Ok(())
    }

    fn play(&self) -> Result<()> {
        debug!("ChromecastRenderer: play()");

        let device = self.connect()?;

        let state = self.session_state.lock()
            .map_err(|e| anyhow!("Failed to acquire session state lock: {}", e))?;

        let destination_id = state.destination_id.as_ref()
            .ok_or_else(|| anyhow!("No destination ID available"))?
            .clone();

        let media_session_id = state.media_session_id
            .ok_or_else(|| anyhow!("No media session ID available"))?;

        drop(state);

        device.media.play(&destination_id, media_session_id)
            .map_err(|e| anyhow!("Failed to play: {}", e))?;

        Ok(())
    }

    fn pause(&self) -> Result<()> {
        debug!("ChromecastRenderer: pause()");

        let device = self.connect()?;

        let state = self.session_state.lock()
            .map_err(|e| anyhow!("Failed to acquire session state lock: {}", e))?;

        let destination_id = state.destination_id.as_ref()
            .ok_or_else(|| anyhow!("No destination ID available"))?
            .clone();

        let media_session_id = state.media_session_id
            .ok_or_else(|| anyhow!("No media session ID available"))?;

        drop(state);

        device.media.pause(&destination_id, media_session_id)
            .map_err(|e| anyhow!("Failed to pause: {}", e))?;

        Ok(())
    }

    fn stop(&self) -> Result<()> {
        debug!("ChromecastRenderer: stop()");

        let device = self.connect()?;

        let state = self.session_state.lock()
            .map_err(|e| anyhow!("Failed to acquire session state lock: {}", e))?;

        let destination_id = state.destination_id.as_ref()
            .ok_or_else(|| anyhow!("No destination ID available"))?
            .clone();

        let media_session_id = state.media_session_id
            .ok_or_else(|| anyhow!("No media session ID available"))?;

        drop(state);

        device.media.stop(&destination_id, media_session_id)
            .map_err(|e| anyhow!("Failed to stop: {}", e))?;

        Ok(())
    }

    fn seek_rel_time(&self, hhmmss: &str) -> Result<()> {
        debug!("ChromecastRenderer: seek_rel_time({})", hhmmss);

        let seconds = Self::parse_hhmmss_to_seconds(hhmmss)? as f32;
        let device = self.connect()?;

        let state = self.session_state.lock()
            .map_err(|e| anyhow!("Failed to acquire session state lock: {}", e))?;

        let destination_id = state.destination_id.as_ref()
            .ok_or_else(|| anyhow!("No destination ID available"))?
            .clone();

        let media_session_id = state.media_session_id
            .ok_or_else(|| anyhow!("No media session ID available"))?;

        drop(state);

        device.media.seek(
            &destination_id,
            media_session_id,
            Some(seconds),
            Some(rust_cast::channels::media::ResumeState::PlaybackStart),
        ).map_err(|e| anyhow!("Failed to seek: {}", e))?;

        Ok(())
    }
}

impl VolumeControl for ChromecastRenderer {
    fn volume(&self) -> Result<u16> {
        let device = self.connect()?;

        let status = device.receiver.get_status()
            .map_err(|e| anyhow!("Failed to get receiver status: {}", e))?;

        // Convert f32 (0.0-1.0) to u16 (0-100)
        let volume = (status.volume.level.unwrap_or(0.0) * 100.0).round() as u16;
        Ok(volume.min(100))
    }

    fn set_volume(&self, v: u16) -> Result<()> {
        debug!("ChromecastRenderer: set_volume({})", v);

        let device = self.connect()?;
        let level = (v.min(100) as f32) / 100.0;

        device.receiver.set_volume(level)
            .map_err(|e| anyhow!("Failed to set volume: {}", e))?;

        Ok(())
    }

    fn mute(&self) -> Result<bool> {
        let device = self.connect()?;

        let status = device.receiver.get_status()
            .map_err(|e| anyhow!("Failed to get receiver status: {}", e))?;

        Ok(status.volume.muted.unwrap_or(false))
    }

    fn set_mute(&self, m: bool) -> Result<()> {
        debug!("ChromecastRenderer: set_mute({})", m);

        let device = self.connect()?;

        // Use set_volume with bool (Volume implements From<bool>)
        device.receiver.set_volume(m)
            .map_err(|e| anyhow!("Failed to set mute: {}", e))?;

        Ok(())
    }
}

impl PlaybackStatus for ChromecastRenderer {
    fn playback_state(&self) -> Result<PlaybackState> {
        let status = self.get_media_status()?;

        if let Some(entry) = status.entries.first() {
            use rust_cast::channels::media::PlayerState;

            let state = match entry.player_state {
                PlayerState::Playing => PlaybackState::Playing,
                PlayerState::Paused => PlaybackState::Paused,
                PlayerState::Idle => PlaybackState::Stopped,
                PlayerState::Buffering => PlaybackState::Transitioning,
            };

            Ok(state)
        } else {
            Ok(PlaybackState::NoMedia)
        }
    }
}

impl PlaybackPosition for ChromecastRenderer {
    fn playback_position(&self) -> Result<PlaybackPositionInfo> {
        let status = self.get_media_status()?;

        if let Some(entry) = status.entries.first() {
            let rel_time = entry.current_time.map(|t| Self::format_seconds_to_hhmmss(t as f64));

            let track_duration = entry.media.as_ref()
                .and_then(|m| m.duration)
                .map(|d| Self::format_seconds_to_hhmmss(d as f64));

            let track_uri = entry.media.as_ref()
                .map(|m| m.content_id.clone());

            Ok(PlaybackPositionInfo {
                track: None,
                rel_time,
                abs_time: None,
                track_duration,
                track_metadata: None,
                track_uri,
            })
        } else {
            Ok(PlaybackPositionInfo {
                track: None,
                rel_time: None,
                abs_time: None,
                track_duration: None,
                track_metadata: None,
                track_uri: None,
            })
        }
    }
}
