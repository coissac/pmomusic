//! Error types for the Radio France client

/// Result type alias for Radio France operations
pub type Result<T> = std::result::Result<T, Error>;

/// Errors that can occur when using the Radio France client
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

    /// API returned an error status
    #[error("API error: {0}")]
    ApiError(String),

    /// Station not found
    #[error("Station not found: {0}")]
    StationNotFound(String),

    /// No HiFi stream available for station
    #[error("No HiFi stream found for station: {0}")]
    NoHifiStream(String),

    /// Scraping failed (HTML parsing error)
    #[error("Scraping failed: {0}")]
    ScrapingError(String),

    /// Regex error
    #[error("Regex error: {0}")]
    RegexError(#[from] regex::Error),

    /// Invalid station slug format
    #[error("Invalid station slug: {0}")]
    InvalidSlug(String),

    /// Timeout error
    #[error("Request timeout")]
    Timeout,

    /// Configuration error (from pmoconfig/anyhow)
    #[error("Configuration error: {0}")]
    Config(#[from] anyhow::Error),

    /// Generic error
    #[error("{0}")]
    Other(String),
}

impl Error {
    /// Create a generic error from a string
    pub fn other(msg: impl Into<String>) -> Self {
        Self::Other(msg.into())
    }

    /// Create an API error
    pub fn api_error(msg: impl Into<String>) -> Self {
        Self::ApiError(msg.into())
    }

    /// Create a scraping error
    pub fn scraping_error(msg: impl Into<String>) -> Self {
        Self::ScrapingError(msg.into())
    }
}
