use thiserror::Error;

#[derive(Error, Debug)]
pub enum StateValueError {
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
