//! PMO Web Renderer - Transforme un navigateur en MediaRenderer UPnP privé

mod error;
mod handlers;
mod messages;
mod renderer;
mod session;
mod state;
mod websocket;

#[cfg(feature = "pmoserver")]
mod config;

pub use error::WebRendererError;
pub use messages::{
    BrowserCapabilities, ClientMessage, CommandParams, PlaybackState, RendererInfo, ServerMessage,
    TrackMetadata, TransportAction,
};
pub use renderer::{FactoryError, WebRendererFactory};
pub use session::{SessionManager, WebRendererSession};
pub use state::{RendererState, SharedState};
pub use websocket::{websocket_handler, WebSocketState};

#[cfg(feature = "pmoserver")]
pub use config::WebRendererExt;
