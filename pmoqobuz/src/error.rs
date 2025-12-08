//! Gestion des erreurs pour le client Qobuz

use thiserror::Error;

/// Type Result personnalisé pour pmoqobuz
pub type Result<T> = std::result::Result<T, QobuzError>;

/// Erreurs possibles lors de l'utilisation du client Qobuz
#[derive(Error, Debug)]
pub enum QobuzError {
    /// Erreur d'authentification (credentials invalides)
    #[error("Authentication failed: {0}")]
    Unauthorized(String),

    /// Ressource non trouvée (album, track, etc.)
    #[error("Resource not found: {0}")]
    NotFound(String),

    /// Erreur HTTP
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    /// Erreur de parsing JSON
    #[error("JSON parsing error: {0}")]
    JsonParse(#[from] serde_json::Error),

    /// Erreur de configuration (anyhow)
    #[error("Configuration error: {0}")]
    Config(#[from] anyhow::Error),

    /// Erreur de configuration Qobuz (App ID, secret, etc.)
    #[error("Qobuz configuration error: {0}")]
    Configuration(String),

    /// Erreur de l'API Qobuz
    #[error("Qobuz API error (code {code}): {message}")]
    ApiError { code: u16, message: String },

    /// Quota dépassé (rate limiting)
    #[error("Rate limit exceeded, please try again later")]
    RateLimitExceeded,

    /// Contenu non disponible dans la région de l'utilisateur
    #[error("Content not available in your region")]
    NotAvailable,

    /// Abonnement insuffisant pour accéder au contenu
    #[error("Subscription level insufficient: {0}")]
    SubscriptionRequired(String),

    /// Erreur de cache
    #[error("Cache error: {0}")]
    Cache(String),

    /// Erreur d'export DIDL
    #[error("DIDL export error: {0}")]
    DidlExport(String),

    /// Erreur générique
    #[error("Qobuz error: {0}")]
    Other(String),
}

impl QobuzError {
    /// Crée une erreur API depuis un code de statut HTTP et un message
    pub fn from_status_code(code: u16, message: impl Into<String>) -> Self {
        match code {
            401 | 403 => Self::Unauthorized(message.into()),
            404 => Self::NotFound(message.into()),
            429 => Self::RateLimitExceeded,
            _ => Self::ApiError {
                code,
                message: message.into(),
            },
        }
    }

    /// Vérifie si l'erreur est une erreur de credentials (401/403)
    /// ou d'AppID invalide (400 avec "app_id")
    pub fn is_auth_error(&self) -> bool {
        match self {
            QobuzError::Unauthorized(_) => true,
            QobuzError::ApiError { code: 400, message }
                if message.contains("app_id") || message.contains("Invalid") => true,
            _ => false,
        }
    }

    /// Vérifie si l'erreur est une erreur de rate limiting
    pub fn is_rate_limit(&self) -> bool {
        matches!(self, QobuzError::RateLimitExceeded)
    }
}
