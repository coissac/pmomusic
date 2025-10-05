//! # AVTransport Service - Service de contrôle de transport UPnP
//!
//! Ce module implémente le service AVTransport:1 selon la spécification UPnP AV.
//! Le service AVTransport permet de contrôler la lecture de médias **audio**
//! sur des rendus multimédias (MediaRenderer Audio).
//!
//! ## Fonctionnalités
//!
//! Le service AVTransport permet :
//! - **Contrôle de lecture** : Play, Pause, Stop
//! - **Navigation** : Next, Previous, Seek
//! - **Gestion des URIs** : SetAVTransportURI, SetNextAVTransportURI
//! - **Information d'état** : GetTransportInfo, GetPositionInfo, GetMediaInfo
//! - **Capacités** : GetDeviceCapabilities, GetCurrentTransportActions
//!
//! ## Conformité UPnP
//!
//! Cette implémentation suit la spécification **UPnP AVTransport:1 Service Template**.
//! Toutes les actions obligatoires (Required) sont implémentées :
//!
//! - ✅ SetAVTransportURI
//! - ✅ GetMediaInfo
//! - ✅ GetTransportInfo
//! - ✅ GetPositionInfo
//! - ✅ GetDeviceCapabilities
//! - ✅ GetTransportSettings
//! - ✅ GetCurrentTransportActions
//! - ✅ Stop, Play, Pause
//!
//! Et certaines actions optionnelles :
//! - ✅ SetNextAVTransportURI
//! - ✅ Seek, Next, Previous
//!
//! ## Variables d'état
//!
//! Le service expose 24 variables d'état conformes à la spécification :
//!
//! ### État du transport
//! - [`TRANSPORTSTATE`] : État actuel (PLAYING, STOPPED, PAUSED_PLAYBACK, etc.)
//! - [`TRANSPORTSTATUS`] : Status du transport (OK, ERROR_OCCURRED)
//! - [`TRANSPORTPLAYSPEED`] : Vitesse de lecture
//!
//! ### Information sur les pistes
//! - [`CURRENTTRACK`] : Numéro de la piste actuelle
//! - [`NUMBEROFTRACKS`] : Nombre total de pistes
//! - [`CURRENTTRACKDURATION`] : Durée de la piste actuelle
//! - [`CURRENTTRACKURI`] : URI de la piste actuelle
//! - [`CURRENTTRACKMETADATA`] : Métadonnées de la piste
//!
//! ### Positionnement
//! - [`RELATIVETIMEPOSITION`] : Position relative dans la piste
//! - [`ABSOLUTETIMEPOSITION`] : Position absolue
//!
//! ### URIs et métadonnées
//! - [`AVTRANSPORTURI`] : URI de la ressource en cours
//! - [`AVTRANSPORTURIMETADATA`] : Métadonnées associées
//! - [`AVTRANSPORTNEXTURI`] : URI de la ressource suivante
//! - [`AVTRANSPORTNEXTURIMETADATA`] : Métadonnées de la ressource suivante
//!
//! ### Modes et capacités
//! - [`CURRENTPLAYMODE`] : Mode de lecture (NORMAL, SHUFFLE, REPEAT_ONE, etc.)
//! - [`PLAYBACKSTORAGEMEDIUM`] : Support de lecture (NETWORK, HDD, CD-DA, etc.)
//! - [`POSSIBLEPLAYBACKSTORAGEMEDIA`] : Supports de lecture possibles
//!
//! ## Examples
//!
//! ```rust
//! use pmoupnp::mediarenderer::avtransport::AVTTRANSPORT;
//!
//! // Accéder au service
//! let service = &*AVTTRANSPORT;
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
//! - [UPnP AVTransport:1 Service Template](https://www.upnp.org/specs/av/UPnP-av-AVTransport-v1-Service.pdf)
//! - [UPnP AV Architecture](https://upnp.org/specs/av/)

use crate::define_service;

pub mod variables;
pub mod actions;

use actions::{
    GETCURRENTTRANSPORTACTIONS, GETDEVICECAPABILITIES, GETMEDIAINFO,
    GETPOSITIONINFO, GETTRANSPORTINFO, GETTRANSPORTSETTINGS, NEXT, PAUSE,
    PLAY, PREVIOUS, SEEK, SETNEXTAVTRANSPORTURI, SETAVTRANSPORTURI, STOP
};
use variables::{
    ABSOLUTETIMEPOSITION, AVTRANSPORTNEXTURI, AVTRANSPORTNEXTURIMETADATA,
    AVTRANSPORTURI, AVTRANSPORTURIMETADATA, A_ARG_TYPE_INSTANCE_ID,
    A_ARG_TYPE_PLAY_SPEED, A_ARG_TYPE_SEEKMODE, CURRENTMEDIADURATION,
    CURRENTPLAYMODE, CURRENTTRACK, CURRENTTRACKDURATION, CURRENTTRACKMETADATA,
    CURRENTTRACKURI, NUMBEROFTRACKS, PLAYBACKSTORAGEMEDIUM,
    POSSIBLEPLAYBACKSTORAGEMEDIA, RELATIVETIMEPOSITION, SEEKMODE,
    TRANSPORTPLAYSPEED, TRANSPORTSTATE, TRANSPORTSTATUS
};

/// Service AVTransport:1 conforme à la spécification UPnP AV.
///
/// Ce service permet de contrôler le transport de médias **audio** sur un MediaRenderer.
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
/// use pmoupnp::mediarenderer::avtransport::AVTTRANSPORT;
///
/// let service = &*AVTTRANSPORT;
/// assert_eq!(service.name(), "AVTransport");
/// assert_eq!(service.version(), 1);
/// ```
///
/// # Voir aussi
///
/// - Module [`actions`] : Définition de toutes les actions UPnP
/// - Module [`variables`] : Définition de toutes les variables d'état
define_service! {
    pub static AVTTRANSPORT = "AVTransport" {
        variables: [
            ABSOLUTETIMEPOSITION,
            A_ARG_TYPE_INSTANCE_ID,
            A_ARG_TYPE_PLAY_SPEED,
            A_ARG_TYPE_SEEKMODE,
            AVTRANSPORTNEXTURI,
            AVTRANSPORTNEXTURIMETADATA,
            AVTRANSPORTURI,
            AVTRANSPORTURIMETADATA,
            CURRENTMEDIADURATION,
            CURRENTPLAYMODE,
            CURRENTTRACK,
            CURRENTTRACKDURATION,
            CURRENTTRACKMETADATA,
            CURRENTTRACKURI,
            NUMBEROFTRACKS,
            PLAYBACKSTORAGEMEDIUM,
            POSSIBLEPLAYBACKSTORAGEMEDIA,
            RELATIVETIMEPOSITION,
            SEEKMODE,
            TRANSPORTPLAYSPEED,
            TRANSPORTSTATE,
            TRANSPORTSTATUS,
        ],
        actions: [
            GETCURRENTTRANSPORTACTIONS,
            GETDEVICECAPABILITIES,
            GETMEDIAINFO,
            GETPOSITIONINFO,
            GETTRANSPORTINFO,
            GETTRANSPORTSETTINGS,
            NEXT,
            PAUSE,
            PLAY,
            PREVIOUS,
            SEEK,
            SETNEXTAVTRANSPORTURI,
            SETAVTRANSPORTURI,
            STOP,
        ]
    }
}