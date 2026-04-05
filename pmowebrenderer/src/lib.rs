//! PMO Web Renderer — Transforme un navigateur en MediaRenderer UPnP privé
//!
//! Architecture serveur-side streaming :
//! - Le serveur ouvre la source audio (fichier/HTTP) et l'encode en FLAC
//! - Le navigateur lit un flux FLAC via GET /api/webrenderer/{id}/stream
//! - Les commandes UPnP sont relayées vers le pipeline audio via PipelineControl

mod adapter;
mod helpers;
mod register;
mod stream;

#[cfg(feature = "pmoserver")]
mod config;

pub use adapter::BrowserAdapter;
pub use helpers::extract_browser_name;

#[cfg(feature = "pmoserver")]
pub use config::WebRendererExt;