use crate::music_renderer::{PlaybackPositionInfo, PlaylistBinding};
use crate::{DeviceId, DeviceIdentity};

/// Basic device information for event notifications
/// Contains only the essential identification fields
#[derive(Clone, Debug)]
pub struct DeviceBasicInfo {
    pub id: DeviceId,
    pub friendly_name: String,
    pub model_name: String,
    pub manufacturer: String,
}

impl DeviceBasicInfo {
    /// Create from any type implementing DeviceIdentity
    pub fn from_device<D: DeviceIdentity>(device: &D) -> Self {
        Self {
            id: device.id(),
            friendly_name: device.friendly_name().to_string(),
            model_name: device.model_name().to_string(),
            manufacturer: device.manufacturer().to_string(),
        }
    }
}

/// High-level playback state across backends.
#[derive(Clone, Debug)]
pub enum PlaybackState {
    Stopped,
    Playing,
    Paused,
    Transitioning,
    NoMedia,
    /// Backend-specific or unknown state string.
    Unknown(String),
}

impl PlaybackState {
    /// Map a raw UPnP AVTransport CurrentTransportState string
    /// to a logical PlaybackState.
    pub fn from_upnp_state(raw: &str) -> Self {
        let s = raw.trim().to_ascii_uppercase();
        match s.as_str() {
            "STOPPED" => PlaybackState::Stopped,
            "PLAYING" => PlaybackState::Playing,
            "PAUSED_PLAYBACK" => PlaybackState::Paused,
            // States from the AVTransport spec that we normalize:
            "PAUSED_RECORDING" => PlaybackState::Paused,
            "RECORDING" => PlaybackState::Playing,
            // Common vendor-specific states:
            "TRANSITIONING" => PlaybackState::Transitioning,
            "BUFFERING" | "PREPARING" => PlaybackState::Transitioning,
            "NO_MEDIA_PRESENT" => PlaybackState::NoMedia,
            _ => PlaybackState::Unknown(raw.to_string()),
        }
    }

    /// Returns a human-readable label for the playback state.
    pub fn as_str(&self) -> &str {
        match self {
            PlaybackState::Stopped => "STOPPED",
            PlaybackState::Playing => "PLAYING",
            PlaybackState::Paused => "PAUSED",
            PlaybackState::Transitioning => "TRANSITIONING",
            PlaybackState::NoMedia => "NO_MEDIA",
            PlaybackState::Unknown(s) => s.as_str(),
        }
    }
}

/// Indicates the source of the current playback.
///
/// This helps the control point distinguish between playback initiated
/// from the queue vs external sources (e.g., user playing from another app).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PlaybackSource {
    /// No playback active or source unknown.
    #[default]
    None,
    /// Playback was started from the control point's queue.
    FromQueue,
    /// Playback was started externally (e.g., from another app).
    External,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TrackMetadata {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub genre: Option<String>,
    pub album_art_uri: Option<String>,
    pub date: Option<String>,
    pub track_number: Option<String>,
    pub creator: Option<String>,
}

#[derive(Clone, Debug, Copy)]
pub enum RendererProtocol {
    UpnpAvOnly,
    OpenHomeOnly,
    OpenHomeHybrid,
    ChromecastOnly,
}

#[derive(Clone, Debug, Default)]
pub struct RendererCapabilities {
    pub has_avtransport: bool,
    /// True if the renderer is known to support AVTransport.SetNextAVTransportURI.
    ///
    /// This is discovered lazily at runtime; default is false.
    pub has_avtransport_set_next: bool,
    pub has_rendering_control: bool,
    pub has_connection_manager: bool,
    pub has_linkplay_http: bool,
    pub has_arylic_tcp: bool,

    pub has_oh_playlist: bool,
    pub has_oh_volume: bool,
    pub has_oh_info: bool,
    pub has_oh_time: bool,
    pub has_oh_radio: bool,

    pub has_chromecast: bool,
}

impl RendererCapabilities {
    pub fn make(
        has_avtransport: bool,
        has_avtransport_set_next: bool,
        has_rendering_control: bool,
        has_connection_manager: bool,
        has_linkplay_http: bool,
        has_arylic_tcp: bool,
        has_oh_playlist: bool,
        has_oh_volume: bool,
        has_oh_info: bool,
        has_oh_time: bool,
        has_oh_radio: bool,
        has_chromecast: bool,
    ) -> Self {
        RendererCapabilities {
            has_avtransport,
            has_avtransport_set_next,
            has_rendering_control,
            has_connection_manager,
            has_linkplay_http,
            has_arylic_tcp,
            has_oh_playlist,
            has_oh_volume,
            has_oh_info,
            has_oh_time,
            has_oh_radio,
            has_chromecast,
        }
    }

    pub fn has_avtransport(&self) -> bool {
        self.has_avtransport
    }
    pub fn has_avtransport_set_next(&self) -> bool {
        self.has_avtransport_set_next
    }
    pub fn has_rendering_control(&self) -> bool {
        self.has_rendering_control
    }
    pub fn has_connection_manager(&self) -> bool {
        self.has_connection_manager
    }
    pub fn has_linkplay_http(&self) -> bool {
        self.has_linkplay_http
    }
    pub fn has_arylic_tcp(&self) -> bool {
        self.has_arylic_tcp
    }
    pub fn has_oh_playlist(&self) -> bool {
        self.has_oh_playlist
    }
    pub fn has_oh_volume(&self) -> bool {
        self.has_oh_volume
    }
    pub fn has_oh_info(&self) -> bool {
        self.has_oh_info
    }
    pub fn has_oh_time(&self) -> bool {
        self.has_oh_time
    }
    pub fn has_oh_radio(&self) -> bool {
        self.has_oh_radio
    }

    pub fn has_chromecast(&self) -> bool {
        self.has_chromecast
    }

    pub fn supports_set_next(&self) -> bool {
        self.has_avtransport && self.has_avtransport_set_next
    }
}

#[derive(Clone, Debug)]
pub struct RendererInfo {
    id: DeviceId,
    udn: String,
    friendly_name: String,
    model_name: String,
    manufacturer: String,

    protocol: RendererProtocol,
    capabilities: RendererCapabilities,

    location: String,

    server_header: String,
    avtransport_service_type: Option<String>,
    avtransport_control_url: Option<String>,
    rendering_control_service_type: Option<String>,
    rendering_control_control_url: Option<String>,
    connection_manager_service_type: Option<String>,
    connection_manager_control_url: Option<String>,
    oh_playlist_service_type: Option<String>,
    oh_playlist_control_url: Option<String>,
    oh_playlist_event_sub_url: Option<String>,
    oh_info_service_type: Option<String>,
    oh_info_control_url: Option<String>,
    oh_info_event_sub_url: Option<String>,
    oh_time_service_type: Option<String>,
    oh_time_control_url: Option<String>,
    oh_time_event_sub_url: Option<String>,
    oh_volume_service_type: Option<String>,
    oh_volume_control_url: Option<String>,
    oh_radio_service_type: Option<String>,
    oh_radio_control_url: Option<String>,
    oh_product_service_type: Option<String>,
    oh_product_control_url: Option<String>,
}

impl RendererInfo {
    pub fn make(
        id: DeviceId,
        udn: String,
        friendly_name: String,
        model_name: String,
        manufacturer: String,
        protocol: RendererProtocol,
        capabilities: RendererCapabilities,
        location: String,
        server_header: String,
        avtransport_service_type: Option<String>,
        avtransport_control_url: Option<String>,
        rendering_control_service_type: Option<String>,
        rendering_control_control_url: Option<String>,
        connection_manager_service_type: Option<String>,
        connection_manager_control_url: Option<String>,
        oh_playlist_service_type: Option<String>,
        oh_playlist_control_url: Option<String>,
        oh_playlist_event_sub_url: Option<String>,
        oh_info_service_type: Option<String>,
        oh_info_control_url: Option<String>,
        oh_info_event_sub_url: Option<String>,
        oh_time_service_type: Option<String>,
        oh_time_control_url: Option<String>,
        oh_time_event_sub_url: Option<String>,
        oh_volume_service_type: Option<String>,
        oh_volume_control_url: Option<String>,
        oh_radio_service_type: Option<String>,
        oh_radio_control_url: Option<String>,
        oh_product_service_type: Option<String>,
        oh_product_control_url: Option<String>,
    ) -> RendererInfo {
        RendererInfo {
            id,
            udn,
            friendly_name,
            model_name,
            manufacturer,
            protocol,
            capabilities,
            location,
            server_header,
            avtransport_service_type,
            avtransport_control_url,
            rendering_control_service_type,
            rendering_control_control_url,
            connection_manager_service_type,
            connection_manager_control_url,
            oh_playlist_service_type,
            oh_playlist_control_url,
            oh_playlist_event_sub_url,
            oh_info_service_type,
            oh_info_control_url,
            oh_info_event_sub_url,
            oh_time_service_type,
            oh_time_control_url,
            oh_time_event_sub_url,
            oh_volume_service_type,
            oh_volume_control_url,
            oh_radio_service_type,
            oh_radio_control_url,
            oh_product_service_type,
            oh_product_control_url,
        }
    }

    pub fn protocol(&self) -> RendererProtocol {
        self.protocol
    }

    pub fn capabilities(&self) -> &RendererCapabilities {
        &self.capabilities
    }

    pub fn avtransport_service_type(&self) -> Option<String> {
        self.avtransport_service_type.clone()
    }

    pub fn avtransport_control_url(&self) -> Option<String> {
        self.avtransport_control_url.clone()
    }
    pub fn rendering_control_service_type(&self) -> Option<String> {
        self.rendering_control_service_type.clone()
    }
    pub fn rendering_control_control_url(&self) -> Option<String> {
        self.rendering_control_control_url.clone()
    }
    pub fn connection_manager_service_type(&self) -> Option<String> {
        self.connection_manager_service_type.clone()
    }
    pub fn connection_manager_control_url(&self) -> Option<String> {
        self.connection_manager_control_url.clone()
    }
    pub fn oh_playlist_service_type(&self) -> Option<String> {
        self.oh_playlist_service_type.clone()
    }
    pub fn oh_playlist_control_url(&self) -> Option<String> {
        self.oh_playlist_control_url.clone()
    }
    pub fn oh_playlist_event_sub_url(&self) -> Option<String> {
        self.oh_playlist_event_sub_url.clone()
    }
    pub fn oh_info_service_type(&self) -> Option<String> {
        self.oh_info_service_type.clone()
    }
    pub fn oh_info_control_url(&self) -> Option<String> {
        self.oh_info_control_url.clone()
    }
    pub fn oh_info_event_sub_url(&self) -> Option<String> {
        self.oh_info_event_sub_url.clone()
    }
    pub fn oh_time_service_type(&self) -> Option<String> {
        self.oh_time_service_type.clone()
    }
    pub fn oh_time_control_url(&self) -> Option<String> {
        self.oh_time_control_url.clone()
    }
    pub fn oh_time_event_sub_url(&self) -> Option<String> {
        self.oh_time_event_sub_url.clone()
    }
    pub fn oh_volume_service_type(&self) -> Option<String> {
        self.oh_volume_service_type.clone()
    }
    pub fn oh_volume_control_url(&self) -> Option<String> {
        self.oh_volume_control_url.clone()
    }
    pub fn oh_radio_service_type(&self) -> Option<String> {
        self.oh_radio_service_type.clone()
    }
    pub fn oh_radio_control_url(&self) -> Option<String> {
        self.oh_radio_control_url.clone()
    }
    pub fn oh_product_service_type(&self) -> Option<String> {
        self.oh_product_service_type.clone()
    }
    pub fn oh_product_control_url(&self) -> Option<String> {
        self.oh_product_control_url.clone()
    }
}

impl DeviceIdentity for RendererInfo {
    fn id(&self) -> DeviceId {
        self.id.clone()
    }
    fn udn(&self) -> &str {
        &self.udn
    }
    fn friendly_name(&self) -> &str {
        &self.friendly_name
    }
    fn model_name(&self) -> &str {
        &self.model_name
    }
    fn manufacturer(&self) -> &str {
        &self.manufacturer
    }
    fn location(&self) -> &str {
        &self.location
    }
    fn server_header(&self) -> &str {
        &self.server_header
    }

    fn is_a_music_renderer(&self) -> bool {
        true
    }
}

#[derive(Clone, Debug)]
pub enum RendererEvent {
    StateChanged {
        id: DeviceId,
        state: PlaybackState,
    },
    PositionChanged {
        id: DeviceId,
        position: PlaybackPositionInfo,
    },
    VolumeChanged {
        id: DeviceId,
        volume: u16,
    },
    MuteChanged {
        id: DeviceId,
        mute: bool,
    },
    MetadataChanged {
        id: DeviceId,
        metadata: TrackMetadata,
    },
    QueueUpdated {
        id: DeviceId,
        queue_length: usize,
    },
    BindingChanged {
        id: DeviceId,
        binding: Option<PlaylistBinding>,
    },
    Online {
        id: DeviceId,
        info: DeviceBasicInfo,
    },
    Offline {
        id: DeviceId,
    },
}

#[derive(Clone, Debug)]
pub enum MediaServerEvent {
    GlobalUpdated {
        server_id: DeviceId,
        system_update_id: Option<u32>,
    },
    ContainersUpdated {
        server_id: DeviceId,
        container_ids: Vec<String>,
    },
    Online {
        server_id: DeviceId,
        info: DeviceBasicInfo,
    },
    Offline {
        server_id: DeviceId,
    },
}
