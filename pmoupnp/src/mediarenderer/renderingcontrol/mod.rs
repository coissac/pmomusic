//! # RenderingControl Service - Service de contrôle du rendu audio UPnP
//!
//! Ce module implémente le service RenderingControl:1 selon la spécification UPnP AV.
//! Le service RenderingControl permet de contrôler les paramètres de rendu audio
//! (volume, mute, etc.) sur un MediaRenderer.
//!
//! ## Fonctionnalités
//!
//! Le service RenderingControl permet :
//! - **Contrôle du volume** : GetVolume, SetVolume
//! - **Contrôle du mute** : GetMute, SetMute
//! - Support multi-canal (Master, LF, RF)
//!
//! ## Conformité UPnP
//!
//! Cette implémentation suit la spécification **UPnP RenderingControl:1 Service Template**.
//! Toutes les actions obligatoires (Required) sont implémentées :
//!
//! - ✅ GetVolume
//! - ✅ SetVolume
//! - ✅ GetMute
//! - ✅ SetMute
//!
//! ## Variables d'état
//!
//! Le service expose les variables d'état conformes à la spécification :
//!
//! ### Contrôle audio
//! - [`VOLUME`] : Niveau de volume (0-100)
//! - [`MUTE`] : État mute (true/false)
//!
//! ### Arguments
//! - [`A_ARG_TYPE_INSTANCE_ID`] : ID d'instance
//! - [`A_ARG_TYPE_CHANNEL`] : Canal audio (Master, LF, RF)
//!
//! ## Examples
//!
//! ```rust
//! use pmoupnp::mediarenderer::renderingcontrol::RENDERINGCONTROL;
//!
//! // Accéder au service
//! let service = &*RENDERINGCONTROL;
//! println!("Service: {}", service.name());
//! println!("Type: {}", service.service_type());
//! ```
//!
//! ## Références
//!
//! - [UPnP RenderingControl:1 Service Template](https://upnp.org/specs/av/UPnP-av-RenderingControl-v1-Service.pdf)
//! - [UPnP AV Architecture](https://upnp.org/specs/av/)

use crate::define_service;

pub mod variables;
pub mod actions;

use actions::{GETMUTE, GETVOLUME, SETMUTE, SETVOLUME};
use variables::{A_ARG_TYPE_CHANNEL, A_ARG_TYPE_INSTANCE_ID, MUTE, VOLUME};

/// Service RenderingControl:1 conforme à la spécification UPnP AV.
///
/// Ce service permet de contrôler les paramètres de rendu audio sur un MediaRenderer.
///
/// # Initialisation
///
/// Le service est initialisé paresseusement (lazy) au premier accès via la macro
/// [`define_service!`]. Toutes les variables d'état et actions sont automatiquement
/// enregistrées.
///
/// # Exemples
///
/// ```rust
/// use pmoupnp::mediarenderer::renderingcontrol::RENDERINGCONTROL;
///
/// let service = &*RENDERINGCONTROL;
/// assert_eq!(service.name(), "RenderingControl");
/// assert_eq!(service.version(), 1);
/// ```
///
/// # Voir aussi
///
/// - Module [`actions`] : Définition de toutes les actions UPnP
/// - Module [`variables`] : Définition de toutes les variables d'état
define_service! {
    pub static RENDERINGCONTROL = "RenderingControl" {
        variables: [
            A_ARG_TYPE_CHANNEL,
            A_ARG_TYPE_INSTANCE_ID,
            MUTE,
            VOLUME,
        ],
        actions: [
            GETMUTE,
            GETVOLUME,
            SETMUTE,
            SETVOLUME,
        ]
    }
}
