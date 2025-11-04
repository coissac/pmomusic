use thiserror::Error;

#[derive(Error, Debug)]
pub enum ActionError {
    #[error("Action error: {0}")]
    GeneralError(String),

    #[error("Argument error: {0}")]
    ArgumentError(String),

    #[error("Set operation error: {0}")]
    SetError(String),
}

impl From<std::io::Error> for ActionError {
    fn from(err: std::io::Error) -> Self {
        ActionError::GeneralError(format!("IO error: {}", err))
    }
}
