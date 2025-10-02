use thiserror::Error;

#[derive(Error, Debug)]
pub enum ServiceError {
    #[error("Action error: {0}")]
    GeneralError(String),
    
    #[error("Argument error: {0}")]
    ArgumentError(String),
    
    #[error("Set operation error: {0}")]
    SetError(String),
}

impl From<std::io::Error> for ServiceError {
    fn from(err: std::io::Error) -> Self {
        ServiceError::GeneralError(format!("IO error: {}", err))
    }
}