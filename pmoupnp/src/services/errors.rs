//! Erreurs du module services.

use thiserror::Error;

/// Erreurs liées aux services UPnP.
///
/// Cette énumération couvre toutes les erreurs possibles lors de la manipulation
/// de services UPnP, incluant les erreurs de validation, de configuration et d'exécution.
#[derive(Error, Debug)]
pub enum ServiceError {
    /// Erreur générale du service.
    #[error("Service error: {0}")]
    GeneralError(String),

    /// Erreur de validation (paramètres invalides).
    #[error("Validation error: {0}")]
    ValidationError(String),

    /// Erreur lors d'une opération sur un ensemble (Set).
    #[error("Set operation error: {0}")]
    SetError(String),

    /// Erreur liée à une action.
    #[error("Action error: {0}")]
    ActionError(String),

    /// Erreur liée à une variable d'état.
    #[error("State variable error: {0}")]
    StateVariableError(String),

    /// Erreur de configuration.
    #[error("Configuration error: {0}")]
    ConfigError(String),

    /// Erreur réseau ou HTTP.
    #[error("Network error: {0}")]
    NetworkError(String),

    /// Erreur de sérialisation XML.
    #[error("XML serialization error: {0}")]
    XmlError(String),

    /// Erreur lors du traitement SOAP.
    #[error("SOAP error: {0}")]
    SoapError(String),
}

impl From<std::io::Error> for ServiceError {
    fn from(err: std::io::Error) -> Self {
        ServiceError::GeneralError(format!("IO error: {}", err))
    }
}

impl From<crate::UpnpObjectSetError> for ServiceError {
    fn from(err: crate::UpnpObjectSetError) -> Self {
        match err {
            crate::UpnpObjectSetError::AlreadyExists(name) => {
                ServiceError::SetError(format!("Object already exists: {}", name))
            }
        }
    }
}
