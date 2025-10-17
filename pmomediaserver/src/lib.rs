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
//! # Utilisation
//!
//! ```ignore
//! use pmomediaserver::MEDIA_SERVER;
//!
//! // Le device est déjà configuré avec tous ses services
//! let server = MEDIA_SERVER.clone();
//! let instance = server.create_instance();
//! ```

pub mod contentdirectory;
pub mod connectionmanager;
pub mod device;

pub use device::MEDIA_SERVER;
