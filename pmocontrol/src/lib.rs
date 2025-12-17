mod events;
mod media_server_events;

pub mod arylic_tcp;
pub mod avtransport_client;
pub mod capabilities;
pub mod connection_manager_client;
pub mod control_point;
pub mod discovery;
pub mod linkplay;
pub mod media_server;
pub mod model;
pub mod music_renderer;
pub mod openhome;
pub mod openhome_client;
pub mod openhome_playlist;
pub mod openhome_renderer;
pub mod provider;
pub mod queue_backend;
pub mod queue_interne;
pub mod registry;
pub mod rendering_control_client;
pub mod soap_client;
pub mod upnp_renderer;

// pmoserver extension (optional)
#[cfg(feature = "pmoserver")]
pub mod openapi;
#[cfg(feature = "pmoserver")]
pub mod pmoserver_ext;
#[cfg(feature = "pmoserver")]
pub mod sse;

#[cfg(feature = "pmoserver")]
pub use pmoserver_ext::ControlPointExt;

pub use arylic_tcp::ArylicTcpRenderer;
pub use avtransport_client::{AvTransportClient, PositionInfo, TransportInfo};
pub use capabilities::{
    PlaybackPosition, PlaybackPositionInfo, PlaybackState, PlaybackStatus, TransportControl,
    VolumeControl,
};
pub use connection_manager_client::{ConnectionInfo, ConnectionManagerClient, ProtocolInfo};
pub use control_point::{ControlPoint, PlaylistBinding};
pub use linkplay::LinkPlayRenderer;
pub use media_server::{
    MediaBrowser, MediaEntry, MediaResource, MediaServerInfo, MusicServer, ServerId,
    UpnpMediaServer,
};
pub use music_renderer::MusicRenderer;
pub use openhome_playlist::{OpenHomePlaylistSnapshot, OpenHomePlaylistTrack};
pub use openhome_renderer::OpenHomeRenderer;
pub use queue_backend::{EnqueueMode, PlaybackItem, QueueSnapshot};
pub use rendering_control_client::RenderingControlClient;
pub use upnp_renderer::UpnpRenderer;

pub use discovery::{DeviceDescriptionProvider, DiscoveredEndpoint, DiscoveryManager};
pub use model::{
    MediaServerEvent, RendererCapabilities, RendererEvent, RendererId, RendererInfo,
    RendererProtocol,
};
pub use provider::HttpXmlDescriptionProvider;
pub use registry::{DeviceRegistry, DeviceRegistryRead, DeviceUpdate};

pub use soap_client::invoke_upnp_action;
