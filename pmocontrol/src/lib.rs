mod events;
mod media_server_events;

pub mod queue;
pub mod discovery;
pub mod arylic_tcp;
pub mod avtransport_client;
pub mod capabilities;
pub mod chromecast_renderer;
pub mod connection_manager_client;
pub mod control_point;
pub mod errors;
pub mod linkplay_renderer;
pub mod media_server;
pub mod model;
pub mod music_renderer;
pub mod openhome;
pub mod openhome_client;
pub mod openhome_playlist;
pub mod openhome_renderer;
pub mod provider;
pub mod registry;
pub mod rendering_control_client;
pub mod soap_client;
pub mod upnp_renderer;
pub mod online;
pub mod identity;


// pmoserver extension (optional)
#[cfg(feature = "pmoserver")]
pub mod openapi;
#[cfg(feature = "pmoserver")]
pub mod pmoserver_ext;
#[cfg(feature = "pmoserver")]
pub mod sse;

use std::time::Duration;

#[cfg(feature = "pmoserver")]
pub use pmoserver_ext::ControlPointExt;

pub use arylic_tcp::ArylicTcpRenderer;
pub use avtransport_client::{AvTransportClient, PositionInfo, TransportInfo};
pub use capabilities::{
    PlaybackPosition, PlaybackPositionInfo, PlaybackState, PlaybackStatus, TransportControl,
    VolumeControl,
};
pub use chromecast_renderer::ChromecastRenderer;
pub use connection_manager_client::{ConnectionInfo, ConnectionManagerClient, ProtocolInfo};
pub use control_point::{ControlPoint, PlaylistBinding};
pub use linkplay_renderer::LinkPlayRenderer;
pub use media_server::{
    MediaBrowser, MediaEntry, MediaResource, UpnpMediaServer,
};
pub use music_renderer::MusicRendererBackend;
pub use openhome_playlist::{OpenHomePlaylistSnapshot, OpenHomePlaylistTrack};
pub use openhome_renderer::OpenHomeRenderer;
pub use queue::{EnqueueMode, PlaybackItem, QueueSnapshot};
pub use rendering_control_client::RenderingControlClient;
pub use upnp_renderer::UpnpRenderer;

pub use model::{
    MediaServerEvent, RendererCapabilities, RendererEvent, RendererInfo, RendererProtocol,
};
pub use provider::HttpXmlDescriptionProvider;
pub use registry::{DeviceRegistry, DeviceUpdate};

pub use soap_client::invoke_upnp_action;
pub use online::DeviceOnline;

pub use identity::DeviceIdentity;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct DeviceId(pub String);

const DEFAULT_HTTP_TIMEOUT: Duration = Duration::from_secs(30);

