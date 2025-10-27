//! Playlist interne (non exposée publiquement)

pub mod core;
pub mod record;

use self::core::{PlaylistConfig, PlaylistCore};
use self::record::Record;
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
    state: Arc<AtomicU8>,
    pub core: Arc<RwLock<PlaylistCore>>,
    pub persistent: bool,
    last_change: RwLock<SystemTime>,
    writer_lock: RwLock<Option<Weak<()>>>,
}

impl Playlist {
    /// Crée une nouvelle playlist
    pub fn new(id: String, title: String, config: PlaylistConfig, persistent: bool) -> Self {
        Self {
            id,
            title: RwLock::new(title),
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
        self.state.store(PlaylistState::Deleted as u8, Ordering::SeqCst);
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
