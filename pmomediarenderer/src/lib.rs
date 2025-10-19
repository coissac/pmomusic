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
//!
//! # Device UPnP
//!
//! - Type : `urn:schemas-upnp-org:device:MediaRenderer:1`
//! - Services : AVTransport:1, RenderingControl:1, ConnectionManager:1
//!
//! # Utilisation
//!
//! ```ignore
//! use pmomediarenderer::MEDIA_RENDERER;
//!
//! // Le device est déjà configuré avec tous ses services
//! let renderer = MEDIA_RENDERER.clone();
//! let instance = renderer.create_instance();
//! ```

pub mod avtransport;
pub mod connectionmanager;
pub mod device;
pub mod renderingcontrol;

pub use device::MEDIA_RENDERER;
