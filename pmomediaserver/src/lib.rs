//! Module MediaServer UPnP.
//!
//! Ce module implémente un MediaServer UPnP conforme à la spécification
//! UPnP AV Architecture. Un MediaServer permet d'exposer et de servir du contenu audio
//! à des clients UPnP (MediaRenderer).
//!
//! # Architecture
//!
//! Le MediaServer est composé de deux services obligatoires :
//!
//! - **ContentDirectory** : Gestion du contenu et navigation dans la bibliothèque musicale
//! - **ConnectionManager** : Gestion des connexions et des protocoles supportés
//!
//! # Device UPnP
//!
//! - Type : `urn:schemas-upnp-org:device:MediaServer:1`
//! - Services : ContentDirectory:1, ConnectionManager:1
//!
//! # Utilisation de base
//!
//! ```ignore
//! use pmomediaserver::MEDIA_SERVER;
//!
//! // Le device est déjà configuré avec tous ses services
//! let server = MEDIA_SERVER.clone();
//! let instance = server.create_instance();
//! ```
//!
//! # Gestion des sources musicales
//!
//! Le MediaServer peut diffuser plusieurs sources musicales (Qobuz, Radio Paradise, etc.)
//! via le trait `MediaServerExt` :
//!
//! ```ignore
//! use pmomediaserver::server_ext::MediaServerExt;
//! use pmoserver::ServerBuilder;
//! use std::sync::Arc;
//!
//! let mut server = ServerBuilder::new_configured().build();
//!
//! // Enregistrer une source musicale
//! let qobuz = Arc::new(QobuzSource::new(credentials));
//! server.register_music_source(qobuz).await;
//!
//! // Lister toutes les sources
//! let sources = server.list_music_sources().await;
//! for source in sources {
//!     println!("Source: {} ({})", source.name(), source.id());
//! }
//! ```
//!
//! # Enregistrement simplifié avec features
//!
//! Avec les features activées, vous pouvez enregistrer des sources préconfigurées :
//!
//! ```ignore
//! use pmomediaserver::sources::SourcesExt;
//! use pmoserver::ServerBuilder;
//!
//! let mut server = ServerBuilder::new_configured().build();
//!
//! // Enregistrer Qobuz depuis la config (feature "qobuz" requise)
//! server.register_qobuz_from_config().await?;
//! ```

pub mod connectionmanager;
pub mod content_handler;
pub mod contentdirectory;
pub mod device;
pub mod device_ext;
pub mod server_ext;
pub mod source_registry;
pub mod sources;

// API REST pour l'enregistrement des sources (requires features qobuz/paradise)
#[cfg(any(feature = "qobuz", feature = "paradise"))]
pub mod sources_api;

// Extension pour le streaming Paradise (requires feature paradise)
#[cfg(feature = "paradise")]
pub mod paradise_streaming;

pub use content_handler::ContentHandler;
pub use device::MEDIA_SERVER;
pub use device_ext::MediaServerDeviceExt;
pub use server_ext::{MediaServerExt, MusicSourceExt, get_source_registry};
pub use source_registry::SourceRegistry;
pub use sources::{SourceInitError, SourcesExt};

#[cfg(feature = "paradise")]
pub use paradise_streaming::ParadiseStreamingExt;

// Re-export sources when features are enabled
#[cfg(feature = "qobuz")]
pub use pmoqobuz;
