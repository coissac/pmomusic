mod events;

pub mod arylic_tcp;
pub mod avtransport_client;
pub mod capabilities;
pub mod connection_manager_client;
pub mod control_point;
pub mod discovery;
pub mod linkplay;
pub mod model;
pub mod music_renderer;
pub mod provider;
pub mod registry;
pub mod rendering_control_client;
pub mod soap_client;
pub mod upnp_renderer;

pub use arylic_tcp::ArylicTcpRenderer;
pub use avtransport_client::{AvTransportClient, PositionInfo, TransportInfo};
pub use capabilities::{
    PlaybackPosition, PlaybackPositionInfo, PlaybackState, PlaybackStatus, TransportControl,
    VolumeControl,
};
pub use connection_manager_client::{ConnectionInfo, ConnectionManagerClient, ProtocolInfo};
pub use control_point::ControlPoint;
pub use linkplay::LinkPlayRenderer;
pub use music_renderer::MusicRenderer;
pub use rendering_control_client::RenderingControlClient;
pub use upnp_renderer::UpnpRenderer;

pub use discovery::{DeviceDescriptionProvider, DiscoveredEndpoint, DiscoveryManager};
pub use model::{
    MediaServerCapabilities, MediaServerId, MediaServerInfo, RendererCapabilities, RendererEvent,
    RendererId, RendererInfo, RendererProtocol,
};
pub use provider::HttpXmlDescriptionProvider;
pub use registry::{DeviceRegistry, DeviceRegistryRead, DeviceUpdate};

pub use soap_client::invoke_upnp_action;
