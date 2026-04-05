//! Erreurs liées au MediaRenderer

use thiserror::Error;

#[derive(Error, Debug)]
pub enum MediaRendererError {
    #[error("Invalid argument: {0}")]
    InvalidArgument(String),

    #[error("Failed to create device: {0}")]
    DeviceCreationError(String),

    #[error("Failed to register device: {0}")]
    RegistrationError(String),

    #[error("Server not available")]
    ServerNotAvailable,
}
