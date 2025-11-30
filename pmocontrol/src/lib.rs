mod events;

pub mod avtransport_client;
pub mod capabilities;
pub mod connection_manager_client;
pub mod control_point;
pub mod discovery;
pub mod model;
pub mod provider;
pub mod upnp_renderer;
pub mod rendering_control_client;
pub mod registry;
pub mod soap_client;
pub mod music_renderer;

pub use avtransport_client::{AvTransportClient, TransportInfo, PositionInfo};
pub use capabilities::{TransportControl, VolumeControl, PlaybackState, PlaybackStatus, PlaybackPosition, PlaybackPositionInfo};
pub use connection_manager_client::{ConnectionInfo, ConnectionManagerClient, ProtocolInfo};
pub use control_point::ControlPoint;
pub use rendering_control_client::RenderingControlClient;
pub use upnp_renderer::UpnpRenderer;
pub use music_renderer::MusicRenderer;

pub use discovery::{DeviceDescriptionProvider, DiscoveredEndpoint, DiscoveryManager};
pub use model::{
    MediaServerCapabilities, MediaServerId, MediaServerInfo, RendererCapabilities, RendererId,
    RendererEvent, RendererInfo, RendererProtocol,
};
pub use provider::HttpXmlDescriptionProvider;
pub use registry::{DeviceRegistry, DeviceRegistryRead, DeviceUpdate};

pub use soap_client::invoke_upnp_action;
