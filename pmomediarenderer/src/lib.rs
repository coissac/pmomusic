//! Module MediaRenderer UPnP.
//!
//! Ce module implémente un MediaRenderer UPnP audio-only conforme à la spécification
//! UPnP AV Architecture. Un MediaRenderer permet de recevoir et lire du contenu audio
//! depuis un serveur UPnP (MediaServer).
//!
//! # Architecture
//!
//! Le MediaRenderer est composé de trois services obligatoires :
//!
//! - **AVTransport** : Contrôle de la lecture (play, pause, stop, seek, etc.)
//! - **RenderingControl** : Contrôle du volume et du mute
//! - **ConnectionManager** : Gestion des connexions et des protocoles supportés

pub mod adapter;
pub mod avtransport;
pub mod connectionmanager;
pub mod error;
pub mod handlers;
pub mod messages;
pub mod pipeline;
pub mod registry;
pub mod renderingcontrol;
pub mod renderer;
pub mod state;

pub use error::MediaRendererError;
pub use handlers::*;
pub use messages::PlaybackState;
pub use pipeline::{PipelineControl, PipelineHandle, seconds_to_upnp_time, upnp_time_to_seconds, InstancePipeline};
pub use registry::{MediaRendererInstance, MediaRendererRegistry};
pub use state::{RendererState, SharedState};
pub use adapter::{DeviceAdapter, DeviceCommand, DevicePlaybackState, DeviceStateReport};