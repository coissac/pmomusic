//! PMO Web Renderer — Transforme un navigateur en MediaRenderer UPnP privé
//!
//! Architecture serveur-side streaming :
//! - Le serveur ouvre la source audio (fichier/HTTP) et l'encode en FLAC
//! - Le navigateur lit un flux FLAC via GET /api/webrenderer/{id}/stream
//! - Les commandes UPnP sont relayées vers le pipeline audio via PipelineControl

mod error;
mod handlers;
mod messages;
mod pipeline;
mod register;
mod registry;
mod renderer;
mod state;
mod stream;

#[cfg(feature = "pmoserver")]
mod config;

pub use error::WebRendererError;
pub use messages::PlaybackState;
pub use pipeline::{PipelineControl, PipelineHandle};
pub use registry::{RendererRegistry, WebRendererInstance};
pub use renderer::{FactoryError, WebRendererFactory};
pub use state::{RendererState, SharedState};

#[cfg(feature = "pmoserver")]
pub use config::WebRendererExt;
