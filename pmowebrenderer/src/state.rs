//! État partagé du renderer (backend ↔ navigateur)

use parking_lot::RwLock;
use std::sync::Arc;
use tokio::sync::mpsc;

use crate::messages::{PlaybackState, ServerMessage};

/// État temps-réel du renderer (partagé backend ↔ navigateur)
#[derive(Debug, Clone)]
pub struct RendererState {
    pub playback_state: PlaybackState,
    pub current_uri: Option<String>,
    pub current_metadata: Option<String>,
    pub next_uri: Option<String>,
    pub next_metadata: Option<String>,
    pub position: Option<String>,
    pub duration: Option<String>,
    pub volume: u16,
    pub mute: bool,
}

impl Default for RendererState {
    fn default() -> Self {
        Self {
            playback_state: PlaybackState::Stopped,
            current_uri: None,
            current_metadata: None,
            next_uri: None,
            next_metadata: None,
            position: None,
            duration: None,
            volume: 100,
            mute: false,
        }
    }
}

/// Alias pour l'état partagé
pub type SharedState = Arc<RwLock<RendererState>>;

/// Sender WebSocket partagé et remplaçable entre les reconnexions.
///
/// Les handlers UPnP capturent ce `Arc` à la création du device. À chaque
/// reconnexion WebSocket (reload de page), on remplace le sender interne via
/// `set()`, sans avoir à recréer le device ni ses handlers.
#[derive(Clone)]
pub struct SharedSender(Arc<RwLock<Option<mpsc::UnboundedSender<ServerMessage>>>>);

impl SharedSender {
    pub fn new(sender: mpsc::UnboundedSender<ServerMessage>) -> Self {
        Self(Arc::new(RwLock::new(Some(sender))))
    }

    /// Envoie un message au navigateur. Ignore silencieusement si déconnecté.
    pub fn send(&self, msg: ServerMessage) {
        if let Some(tx) = self.0.read().as_ref() {
            let _ = tx.send(msg);
        }
    }

    /// Remplace le sender (appelé à la reconnexion WebSocket).
    pub fn set(&self, sender: mpsc::UnboundedSender<ServerMessage>) {
        *self.0.write() = Some(sender);
    }

    /// Retire le sender (appelé à la déconnexion).
    pub fn clear(&self) {
        *self.0.write() = None;
    }
}
