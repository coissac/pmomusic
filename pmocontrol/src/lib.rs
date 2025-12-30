mod events;
mod media_server_events;

pub mod queue;
pub mod discovery;
pub mod upnp_clients;
pub mod arylic_client;
pub mod linkplay_client;
pub mod control_point;
pub mod errors;
pub mod media_server;
pub mod model;
pub mod music_renderer;
pub mod registry;
pub mod soap_client;
pub mod online;
pub mod identity;
pub mod linkplay_utils;



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


pub use control_point::{ControlPoint, PlaylistBinding};
pub use media_server::{
    MediaBrowser, MediaEntry, MediaResource, UpnpMediaServer,
};
pub use queue::{EnqueueMode, PlaybackItem, QueueSnapshot};

pub use model::{
    MediaServerEvent, RendererCapabilities, RendererEvent, RendererInfo, RendererProtocol,
};
pub use registry::{DeviceRegistry, DeviceUpdate};

pub use soap_client::invoke_upnp_action;
pub use online::DeviceOnline;

pub use identity::DeviceIdentity;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct DeviceId(pub String);

const DEFAULT_HTTP_TIMEOUT: Duration = Duration::from_secs(30);

