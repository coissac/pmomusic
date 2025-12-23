use crate::capabilities::{PlaybackPositionInfo, PlaybackState};
use crate::control_point::PlaylistBinding;
use crate::media_server::ServerId;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct RendererId(pub String);

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

#[derive(Clone, Debug)]
pub enum RendererProtocol {
    UpnpAvOnly,
    OpenHomeOnly,
    Hybrid,
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
    pub fn supports_set_next(&self) -> bool {
        self.has_avtransport && self.has_avtransport_set_next
    }
}

#[derive(Clone, Debug)]
pub struct RendererInfo {
    pub id: RendererId,
    pub udn: String,
    pub friendly_name: String,
    pub model_name: String,
    pub manufacturer: String,

    pub protocol: RendererProtocol,
    pub capabilities: RendererCapabilities,

    pub location: String,
    pub server_header: String,
    pub online: bool,
    pub last_seen: std::time::SystemTime,
    pub max_age: u32,

    pub avtransport_service_type: Option<String>,
    pub avtransport_control_url: Option<String>,
    pub rendering_control_service_type: Option<String>,
    pub rendering_control_control_url: Option<String>,
    pub connection_manager_service_type: Option<String>,
    pub connection_manager_control_url: Option<String>,
    pub oh_playlist_service_type: Option<String>,
    pub oh_playlist_control_url: Option<String>,
    pub oh_playlist_event_sub_url: Option<String>,
    pub oh_info_service_type: Option<String>,
    pub oh_info_control_url: Option<String>,
    pub oh_info_event_sub_url: Option<String>,
    pub oh_time_service_type: Option<String>,
    pub oh_time_control_url: Option<String>,
    pub oh_time_event_sub_url: Option<String>,
    pub oh_volume_service_type: Option<String>,
    pub oh_volume_control_url: Option<String>,
    pub oh_radio_service_type: Option<String>,
    pub oh_radio_control_url: Option<String>,
    pub oh_product_service_type: Option<String>,
    pub oh_product_control_url: Option<String>,
}

#[derive(Clone, Debug)]
pub enum RendererEvent {
    StateChanged {
        id: RendererId,
        state: PlaybackState,
    },
    PositionChanged {
        id: RendererId,
        position: PlaybackPositionInfo,
    },
    VolumeChanged {
        id: RendererId,
        volume: u16,
    },
    MuteChanged {
        id: RendererId,
        mute: bool,
    },
    MetadataChanged {
        id: RendererId,
        metadata: TrackMetadata,
    },
    QueueUpdated {
        id: RendererId,
        queue_length: usize,
    },
    BindingChanged {
        id: RendererId,
        binding: Option<PlaylistBinding>,
    },
}

#[derive(Clone, Debug)]
pub enum MediaServerEvent {
    GlobalUpdated {
        server_id: ServerId,
        system_update_id: Option<u32>,
    },
    ContainersUpdated {
        server_id: ServerId,
        container_ids: Vec<String>,
    },
}
