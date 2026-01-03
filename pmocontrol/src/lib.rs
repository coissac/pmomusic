mod events;
mod media_server_events;

pub mod arylic_client;
pub mod control_point;
pub mod discovery;
pub mod errors;
pub mod identity;
pub mod linkplay_client;
pub mod linkplay_utils;
pub mod media_server;
pub mod model;
pub mod music_renderer;
pub mod online;
pub mod queue;
pub mod registry;
pub mod soap_client;
pub mod upnp_clients;

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

pub use control_point::ControlPoint;
pub use media_server::{MediaBrowser, MediaEntry, MediaResource, UpnpMediaServer};
pub use queue::{EnqueueMode, PlaybackItem, QueueSnapshot};

pub use model::{
    MediaServerEvent, PlaybackSource, RendererCapabilities, RendererEvent, RendererInfo,
    RendererProtocol,
};
pub use registry::{DeviceRegistry, DeviceUpdate};

pub use online::DeviceOnline;
pub use soap_client::invoke_upnp_action;

pub use identity::DeviceIdentity;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct DeviceId(pub String);

const DEFAULT_HTTP_TIMEOUT: Duration = Duration::from_secs(30);
