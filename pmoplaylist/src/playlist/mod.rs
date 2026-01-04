//! Playlist interne (non exposée publiquement)

pub mod core;
pub mod record;

use self::core::{PlaylistConfig, PlaylistCore};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Weak};
use std::time::SystemTime;
use tokio::sync::RwLock;

/// État d'une playlist
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
enum PlaylistState {
    Active = 0,
    Deleted = 1,
}

impl From<u8> for PlaylistState {
    fn from(value: u8) -> Self {
        match value {
            1 => PlaylistState::Deleted,
            _ => PlaylistState::Active,
        }
    }
}

/// Playlist interne (gérée par le PlaylistManager)
pub struct Playlist {
    pub id: String,
    title: RwLock<String>,
    role: RwLock<PlaylistRole>,
    cover_pk: RwLock<Option<String>>,
    artist: RwLock<Option<String>>,
    state: Arc<AtomicU8>,
    pub core: Arc<RwLock<PlaylistCore>>,
    pub persistent: bool,
    last_change: RwLock<SystemTime>,
    writer_lock: RwLock<Option<Weak<()>>>,
}

impl Playlist {
    /// Crée une nouvelle playlist
    pub fn new(
        id: String,
        title: String,
        config: PlaylistConfig,
        persistent: bool,
        role: PlaylistRole,
        cover_pk: Option<String>,
    ) -> Self {
        Self {
            id,
            title: RwLock::new(title),
            role: RwLock::new(role),
            cover_pk: RwLock::new(cover_pk),
            artist: RwLock::new(None),
            state: Arc::new(AtomicU8::new(PlaylistState::Active as u8)),
            core: Arc::new(RwLock::new(PlaylistCore::new(config))),
            persistent,
            last_change: RwLock::new(SystemTime::now()),
            writer_lock: RwLock::new(None),
        }
    }

    /// Vérifie si la playlist est active
    pub fn is_alive(&self) -> bool {
        PlaylistState::from(self.state.load(Ordering::SeqCst)) == PlaylistState::Active
    }

    /// Marque la playlist comme supprimée
    pub fn mark_deleted(&self) {
        self.state
            .store(PlaylistState::Deleted as u8, Ordering::SeqCst);
    }

    /// Met à jour le timestamp de dernière modification
    pub async fn touch(&self) {
        *self.last_change.write().await = SystemTime::now();
    }

    /// Récupère le titre
    pub async fn title(&self) -> String {
        self.title.read().await.clone()
    }

    /// Change le titre
    pub async fn set_title(&self, title: String) {
        *self.title.write().await = title;
        self.touch().await;
    }

    /// Retourne le rôle
    pub async fn role(&self) -> PlaylistRole {
        self.role.read().await.clone()
    }

    /// Change le rôle
    pub async fn set_role(&self, role: PlaylistRole) {
        *self.role.write().await = role;
        self.touch().await;
    }

    /// Retourne la cover associée à la playlist.
    pub async fn cover_pk(&self) -> Option<String> {
        self.cover_pk.read().await.clone()
    }

    /// Modifie la cover (PK) de la playlist.
    pub async fn set_cover_pk(&self, value: Option<String>) {
        *self.cover_pk.write().await = value;
        self.touch().await;
    }

    /// Retourne l'artiste associé à la playlist.
    pub async fn artist(&self) -> Option<String> {
        self.artist.read().await.clone()
    }

    /// Modifie l'artiste de la playlist.
    pub async fn set_artist(&self, value: Option<String>) {
        *self.artist.write().await = value;
        self.touch().await;
    }

    /// Timestamp du dernier changement
    pub async fn last_change(&self) -> SystemTime {
        *self.last_change.read().await
    }

    /// Tente d'acquérir le write lock
    pub async fn acquire_write_lock(&self) -> Result<Arc<()>, ()> {
        let mut guard = self.writer_lock.write().await;

        // Vérifier si un writer existe déjà
        if let Some(weak) = guard.as_ref() {
            if weak.strong_count() > 0 {
                return Err(()); // Lock déjà pris
            }
        }

        // Créer un nouveau token
        let token = Arc::new(());
        *guard = Some(Arc::downgrade(&token));
        Ok(token)
    }
}

/// Rôle logique d'une playlist
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlaylistRole {
    User,
    Album,
    Radio,
    Source,
    Custom(String),
}

impl PlaylistRole {
    pub const fn user() -> Self {
        PlaylistRole::User
    }

    pub const fn album() -> Self {
        PlaylistRole::Album
    }

    pub const fn radio() -> Self {
        PlaylistRole::Radio
    }

    pub const fn source() -> Self {
        PlaylistRole::Source
    }

    pub fn custom<S: Into<String>>(value: S) -> Self {
        PlaylistRole::Custom(value.into())
    }

    pub fn as_str(&self) -> &str {
        match self {
            PlaylistRole::User => "user",
            PlaylistRole::Album => "album",
            PlaylistRole::Radio => "radio",
            PlaylistRole::Source => "source",
            PlaylistRole::Custom(value) => value.as_str(),
        }
    }
}

impl Default for PlaylistRole {
    fn default() -> Self {
        PlaylistRole::User
    }
}

impl fmt::Display for PlaylistRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Serialize for PlaylistRole {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for PlaylistRole {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Ok(PlaylistRole::from_str(&value).unwrap_or_else(|_| PlaylistRole::Custom(value)))
    }
}

impl FromStr for PlaylistRole {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "user" => Ok(PlaylistRole::User),
            "album" => Ok(PlaylistRole::Album),
            "radio" => Ok(PlaylistRole::Radio),
            "source" => Ok(PlaylistRole::Source),
            other => Ok(PlaylistRole::Custom(other.to_string())),
        }
    }
}
