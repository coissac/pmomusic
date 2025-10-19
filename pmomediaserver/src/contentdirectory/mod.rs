//! # ContentDirectory Service - Service de gestion du contenu UPnP
//!
//! Ce module implémente le service ContentDirectory:1 selon la spécification UPnP AV.
//! Le service ContentDirectory permet de naviguer et rechercher dans une bibliothèque
//! de contenu musical.
//!
//! ## Fonctionnalités
//!
//! Le service ContentDirectory permet :
//! - **Navigation** : Browse pour parcourir la hiérarchie de contenu
//! - **Recherche** : Search pour rechercher du contenu selon des critères
//! - **Capacités** : GetSearchCapabilities, GetSortCapabilities
//! - **Synchronisation** : GetSystemUpdateID pour détecter les changements
//!
//! ## Conformité UPnP
//!
//! Cette implémentation suit la spécification **UPnP ContentDirectory:1 Service Template**.
//! Toutes les actions obligatoires (Required) sont implémentées :
//!
//! - ✅ Browse
//! - ✅ GetSearchCapabilities
//! - ✅ GetSortCapabilities
//! - ✅ GetSystemUpdateID
//!
//! Et certaines actions optionnelles :
//! - ✅ Search
//!
//! ## Variables d'état
//!
//! Le service expose les variables d'état conformes à la spécification :
//!
//! ### Variables principales
//! - [`SYSTEMUPDATEID`] : ID de mise à jour du système (évènementiel)
//! - [`SEARCHCAPABILITIES`] : Capacités de recherche supportées
//! - [`SORTCAPABILITIES`] : Capacités de tri supportées
//!
//! ### Arguments d'action
//! - [`A_ARG_TYPE_OBJECTID`] : ID d'un objet (container ou item)
//! - [`A_ARG_TYPE_BROWSEFLAG`] : Type de browsing (Metadata ou DirectChildren)
//! - [`A_ARG_TYPE_FILTER`] : Filtre de propriétés à retourner
//! - [`A_ARG_TYPE_SORTCRITERIA`] : Critères de tri
//! - [`A_ARG_TYPE_SEARCHCRITERIA`] : Critères de recherche
//! - [`A_ARG_TYPE_INDEX`] : Index de départ
//! - [`A_ARG_TYPE_COUNT`] : Nombre d'éléments
//! - [`A_ARG_TYPE_UPDATEID`] : ID de mise à jour
//! - [`A_ARG_TYPE_RESULT`] : Résultat au format DIDL-Lite
//!
//! ## Examples
//!
//! ```rust
//! use pmomediaserver::contentdirectory::CONTENTDIRECTORY;
//! use pmoupnp::UpnpTyped;
//!
//! // Accéder au service
//! let service = &*CONTENTDIRECTORY;
//! println!("Service: {}", service.name());
//! println!("Type: {}", service.service_type());
//!
//! // Lister les actions disponibles
//! for action in service.actions() {
//!     println!("  Action: {}", action.get_name());
//! }
//!
//! // Lister les variables d'état
//! for variable in service.variables() {
//!     println!("  Variable: {}", variable.get_name());
//! }
//! ```
//!
//! ## Références
//!
//! - [UPnP ContentDirectory:1 Service Template](https://upnp.org/specs/av/UPnP-av-ContentDirectory-v1-Service.pdf)
//! - [UPnP AV Architecture](https://upnp.org/specs/av/)

use pmoupnp::define_service;

pub mod actions;
pub mod handlers;
pub mod variables;

use actions::{BROWSE, GETSEARCHCAPABILITIES, GETSORTCAPABILITIES, GETSYSTEMUPDATEID, SEARCH};
use variables::{
    A_ARG_TYPE_BROWSEFLAG, A_ARG_TYPE_COUNT, A_ARG_TYPE_FILTER, A_ARG_TYPE_INDEX,
    A_ARG_TYPE_OBJECTID, A_ARG_TYPE_RESULT, A_ARG_TYPE_SEARCHCRITERIA, A_ARG_TYPE_SORTCRITERIA,
    A_ARG_TYPE_UPDATEID, SEARCHCAPABILITIES, SORTCAPABILITIES, SYSTEMUPDATEID,
};

// Service ContentDirectory:1 conforme à la spécification UPnP AV pour MediaServer
// Voir la documentation du module pour plus de détails
define_service! {
    pub static CONTENTDIRECTORY = "ContentDirectory" {
        variables: [
            A_ARG_TYPE_OBJECTID,
            A_ARG_TYPE_BROWSEFLAG,
            A_ARG_TYPE_FILTER,
            A_ARG_TYPE_SORTCRITERIA,
            A_ARG_TYPE_INDEX,
            A_ARG_TYPE_COUNT,
            A_ARG_TYPE_UPDATEID,
            A_ARG_TYPE_RESULT,
            A_ARG_TYPE_SEARCHCRITERIA,
            SEARCHCAPABILITIES,
            SORTCAPABILITIES,
            SYSTEMUPDATEID,
        ],
        actions: [
            BROWSE,
            SEARCH,
            GETSEARCHCAPABILITIES,
            GETSORTCAPABILITIES,
            GETSYSTEMUPDATEID,
        ]
    }
}
