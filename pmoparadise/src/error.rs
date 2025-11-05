//! Error types for the Radio Paradise client

/// Result type alias for Radio Paradise operations
pub type Result<T> = std::result::Result<T, Error>;

/// Errors that can occur when using the Radio Paradise client
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// HTTP request failed
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),

    /// JSON parsing failed
    #[error("JSON parsing failed: {0}")]
    Json(#[from] serde_json::Error),

    /// Invalid URL
    #[error("Invalid URL: {0}")]
    InvalidUrl(#[from] url::ParseError),

    /// IO error
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Invalid track index
    #[error("Invalid track index: {0} (block has {1} tracks)")]
    InvalidIndex(usize, usize),

    /// Invalid bitrate
    #[error("Invalid bitrate value: {0} (must be 0-4)")]
    InvalidBitrate(u8),

    /// Invalid event ID
    #[error("Invalid event ID: {0}")]
    InvalidEvent(String),

    /// Track not found in block
    #[error("Track not found at index {0}")]
    TrackNotFound(usize),

    /// Invalid elapsed time
    #[error("Invalid elapsed time: {0}ms (exceeds block length)")]
    InvalidElapsed(u64),

    /// Timeout error
    #[error("Request timeout")]
    Timeout,

    /// Generic error
    #[error("{0}")]
    Other(String),
}

impl Error {
    /// Create a generic error from a string
    pub fn other(msg: impl Into<String>) -> Self {
        Self::Other(msg.into())
    }
}
