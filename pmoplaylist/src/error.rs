//! Types d'erreurs pour pmoplaylist

/// Erreurs de gestion de playlist
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Playlist not found: {0}")]
    PlaylistNotFound(String),

    #[error("Playlist deleted: {0}")]
    PlaylistDeleted(String),

    #[error("Playlist already exists: {0}")]
    PlaylistAlreadyExists(String),

    #[error("Playlist is not persistent: {0}")]
    PlaylistNotPersistent(String),

    #[error("Write lock already held for playlist: {0}")]
    WriteLockHeld(String),

    #[error("Cache entry not found: {0}")]
    CacheEntryNotFound(String),

    #[error("Cache error: {0}")]
    CacheError(String),

    #[error("Persistence error: {0}")]
    PersistenceError(String),

    #[error("PlaylistManager not initialized")]
    ManagerNotInitialized,

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

/// Type Result spécialisé pour pmoplaylist
pub type Result<T> = std::result::Result<T, Error>;
