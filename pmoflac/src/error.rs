use std::io;

#[derive(thiserror::Error, Debug)]
pub enum FlacError {
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("FLAC decode error: {0}")]
    Decode(String),
    #[error("FLAC encode error: {0}")]
    Encode(String),
    #[error("libFLAC initialization failed: {0}")]
    LibFlacInit(String),
    #[error("libFLAC write callback failed: {0}")]
    LibFlacWrite(String),
    #[error("internal channel closed unexpectedly")]
    ChannelClosed,
    #[error("unsupported configuration: {0}")]
    Unsupported(String),
    #[error("{role} task failed: {details}")]
    TaskJoin { role: &'static str, details: String },
}

impl From<claxon::Error> for FlacError {
    fn from(err: claxon::Error) -> Self {
        FlacError::Decode(err.to_string())
    }
}
