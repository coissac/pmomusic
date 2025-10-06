//! Erreurs relatives aux devices UPnP.

use thiserror::Error;

/// Erreurs liées aux devices UPnP.
#[derive(Error, Debug)]
pub enum DeviceError {
    /// Service déjà existant
    #[error("Service '{0}' already exists in device")]
    ServiceAlreadyExists(String),

    /// Device déjà existant
    #[error("Device '{0}' already exists")]
    DeviceAlreadyExists(String),

    /// Version invalide
    #[error("Device version must be > 0")]
    InvalidVersion,

    /// Erreur d'enregistrement d'URL
    #[error("Failed to register URL: {0}")]
    UrlRegistrationError(String),
}
