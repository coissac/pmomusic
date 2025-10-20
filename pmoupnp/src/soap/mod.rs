//! # Module SOAP - Simple Object Access Protocol
//!
//! Ce module implémente le support SOAP pour UPnP, permettant l'invocation d'actions
//! et la gestion des réponses/erreurs.
//!
//! ## Fonctionnalités
//!
//! - ✅ Parsing d'enveloppes SOAP
//! - ✅ Extraction d'actions UPnP avec arguments
//! - ✅ Construction de réponses SOAP
//! - ✅ Gestion des SOAP Faults
//! - ✅ Support des namespaces UPnP
//!
//! ## Architecture
//!
//! - [`SoapEnvelope`] : Enveloppe SOAP complète
//! - [`SoapAction`] : Action UPnP extraite
//! - [`SoapResponse`] : Réponse UPnP
//! - [`SoapFault`] : Erreur SOAP
//!
//! ## Example
//!
//! ```ignore
//! use pmoupnp::soap::{parse_soap_action, build_soap_response};
//!
//! // Parser une action SOAP
//! let body = r#"<?xml version="1.0"?>
//! <s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/">
//!   <s:Body>
//!     <u:Play xmlns:u="urn:schemas-upnp-org:service:AVTransport:1">
//!       <InstanceID>0</InstanceID>
//!       <Speed>1</Speed>
//!     </u:Play>
//!   </s:Body>
//! </s:Envelope>"#;
//!
//! let action = parse_soap_action(body.as_bytes()).unwrap();
//! assert_eq!(action.name, "Play");
//! assert_eq!(action.args.get("InstanceID"), Some(&"0".to_string()));
//!
//! // Construire une réponse
//! let mut values = std::collections::HashMap::new();
//! values.insert("CurrentTrack".to_string(), "5".to_string());
//! let response = build_soap_response(
//!     "urn:schemas-upnp-org:service:AVTransport:1",
//!     "GetPositionInfo",
//!     values
//! ).unwrap();
//! ```

mod builder;
mod envelope;
mod fault;
mod parser;

pub use builder::build_soap_response;
pub use envelope::{SoapBody, SoapEnvelope, SoapHeader};
pub use fault::{SoapFault, build_soap_fault};
pub use parser::{SoapAction, parse_soap_action};

/// Codes d'erreur SOAP UPnP standards
pub mod error_codes {
    /// Action invalide
    pub const INVALID_ACTION: &str = "401";

    /// Arguments invalides
    pub const INVALID_ARGS: &str = "402";

    /// Action échouée
    pub const ACTION_FAILED: &str = "501";

    /// Argument manquant
    pub const ARGUMENT_VALUE_INVALID: &str = "600";

    /// Argument hors limites
    pub const ARGUMENT_VALUE_OUT_OF_RANGE: &str = "601";

    /// Action optionnelle non implémentée
    pub const OPTIONAL_ACTION_NOT_IMPLEMENTED: &str = "602";

    /// Mémoire insuffisante
    pub const OUT_OF_MEMORY: &str = "603";

    /// Erreur humaine lisible
    pub const HUMAN_INTERVENTION_REQUIRED: &str = "604";

    /// Argument sous forme de chaîne trop long
    pub const STRING_ARGUMENT_TOO_LONG: &str = "605";
}
