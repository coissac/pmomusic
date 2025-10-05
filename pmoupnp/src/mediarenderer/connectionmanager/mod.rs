//! # ConnectionManager Service - Service de gestion des connexions UPnP
//!
//! Ce module implémente le service ConnectionManager:1 selon la spécification UPnP AV.
//! Le service ConnectionManager gère les connexions entre MediaServer et MediaRenderer,
//! et expose les protocoles et formats supportés.
//!
//! ## Fonctionnalités
//!
//! Le service ConnectionManager permet :
//! - **Énumération des protocoles** : GetProtocolInfo
//! - **Gestion des connexions** : GetCurrentConnectionIDs, GetCurrentConnectionInfo
//! - Support des formats audio (MP3, FLAC, WAV, etc.)
//!
//! ## Conformité UPnP
//!
//! Cette implémentation suit la spécification **UPnP ConnectionManager:1 Service Template**.
//! Toutes les actions obligatoires (Required) sont implémentées :
//!
//! - ✅ GetProtocolInfo
//! - ✅ GetCurrentConnectionIDs
//! - ✅ GetCurrentConnectionInfo
//!
//! ## Variables d'état
//!
//! Le service expose les variables d'état conformes à la spécification :
//!
//! ### Informations de protocole
//! - [`SOURCEPROTOCOLINFO`] : Protocoles source (vide pour un renderer)
//! - [`SINKPROTOCOLINFO`] : Protocoles sink supportés (http-get:*:audio/mpeg:*, etc.)
//! - [`CURRENTCONNECTIONIDS`] : IDs des connexions actives
//!
//! ### Arguments
//! - [`A_ARG_TYPE_CONNECTIONID`] : ID de connexion
//! - [`A_ARG_TYPE_CONNECTIONSTATUS`] : Statut de connexion
//! - [`A_ARG_TYPE_DIRECTION`] : Direction (Input/Output)
//! - [`A_ARG_TYPE_PROTOCOLINFO`] : Information de protocole
//! - [`A_ARG_TYPE_RCSID`] : ID RenderingControl
//! - [`A_ARG_TYPE_AVTRANSPORTID`] : ID AVTransport
//!
//! ## Examples
//!
//! ```rust
//! use pmoupnp::mediarenderer::connectionmanager::CONNECTIONMANAGER;
//!
//! // Accéder au service
//! let service = &*CONNECTIONMANAGER;
//! println!("Service: {}", service.name());
//! println!("Type: {}", service.service_type());
//! ```
//!
//! ## Références
//!
//! - [UPnP ConnectionManager:1 Service Template](https://upnp.org/specs/av/UPnP-av-ConnectionManager-v1-Service.pdf)
//! - [UPnP AV Architecture](https://upnp.org/specs/av/)

use crate::define_service;

pub mod variables;
pub mod actions;

use actions::{GETCURRENTCONNECTIONIDS, GETCURRENTCONNECTIONINFO, GETPROTOCOLINFO};
use variables::{
    A_ARG_TYPE_AVTRANSPORTID, A_ARG_TYPE_CONNECTIONID, A_ARG_TYPE_CONNECTIONSTATUS,
    A_ARG_TYPE_DIRECTION, A_ARG_TYPE_PROTOCOLINFO, A_ARG_TYPE_RCSID,
    CURRENTCONNECTIONIDS, SINKPROTOCOLINFO, SOURCEPROTOCOLINFO
};

// Service ConnectionManager:1 conforme à la spécification UPnP AV pour MediaRenderer audio
// Voir la documentation du module pour plus de détails
define_service! {
    pub static CONNECTIONMANAGER = "ConnectionManager" {
        variables: [
            A_ARG_TYPE_AVTRANSPORTID,
            A_ARG_TYPE_CONNECTIONID,
            A_ARG_TYPE_CONNECTIONSTATUS,
            A_ARG_TYPE_DIRECTION,
            A_ARG_TYPE_PROTOCOLINFO,
            A_ARG_TYPE_RCSID,
            CURRENTCONNECTIONIDS,
            SINKPROTOCOLINFO,
            SOURCEPROTOCOLINFO,
        ],
        actions: [
            GETCURRENTCONNECTIONIDS,
            GETCURRENTCONNECTIONINFO,
            GETPROTOCOLINFO,
        ]
    }
}
