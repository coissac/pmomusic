pub mod discovery;
pub mod model;
pub mod registry;

pub use discovery::{DeviceDescriptionProvider, DiscoveredEndpoint, DiscoveryManager};
pub use model::{
    MediaServerCapabilities, MediaServerId, MediaServerInfo, RendererCapabilities, RendererId,
    RendererInfo, RendererProtocol,
};
pub use registry::{DeviceRegistry, DeviceRegistryRead, DeviceUpdate};
