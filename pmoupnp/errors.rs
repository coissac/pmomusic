use thiserror::Error;



#[derive(Error, Debug)]
pub enum StateVariableError {
    #[error("Conversion error: {0}")]
    ConversionError(String),
    
    #[error("Validation error: {0}")]
    ValidationError(String),
    
    #[error("Range error: {0}")]
    RangeError(String),
    
    #[error("Type error: {0}")]
    TypeError(String),
    
    #[error("Parse error: {0}")]
    ParseError(String),
    
    #[error("Event condition error: {0}")]
    EventConditionError(String),
    
    #[error("Arithmetic error: {0}")]
    ArithmeticError(String),
    
    #[error("Unknown error: {0}")]
    Unknown(String),
}

impl From<std::num::TryFromIntError> for StateVariableError {
    fn from(err: std::num::TryFromIntError) -> Self {
        StateVariableError::ConversionError(format!("Integer conversion error: {}", err))
    }
}

impl From<std::str::ParseBoolError> for StateVariableError {
    fn from(err: std::str::ParseBoolError) -> Self {
        StateVariableError::ConversionError(format!("Boolean conversion error: {}", err))
    }
}

impl From<uuid::Error> for StateVariableError {
    fn from(err: uuid::Error) -> Self {
        StateVariableError::ConversionError(format!("UUID conversion error: {}", err))
    }
}

impl From<chrono::ParseError> for StateVariableError {
    fn from(err: chrono::ParseError) -> Self {
        StateVariableError::ConversionError(format!("Time conversion error: {}", err))
    }
}

impl From<url::ParseError> for StateVariableError {
    fn from(err: url::ParseError) -> Self {
        StateVariableError::ConversionError(format!("URI conversion error: {}", err))
    }
}

impl From<base64::DecodeError> for StateVariableError {
    fn from(err: base64::DecodeError) -> Self {
        StateVariableError::ConversionError(format!("Base64 conversion error: {}", err))
    }
}

impl From<hex::FromHexError> for StateVariableError {
    fn from(err: hex::FromHexError) -> Self {
        StateVariableError::ConversionError(format!("Hex conversion error: {}", err))
    }
}
