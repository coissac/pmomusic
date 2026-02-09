//! État partagé du renderer (backend ↔ navigateur)

use parking_lot::RwLock;
use std::sync::Arc;

use crate::messages::PlaybackState;

/// État temps-réel du renderer (partagé backend ↔ navigateur)
#[derive(Debug, Clone)]
pub struct RendererState {
    pub playback_state: PlaybackState,
    pub current_uri: Option<String>,
    pub current_metadata: Option<String>,
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
            position: None,
            duration: None,
            volume: 100,
            mute: false,
        }
    }
}

/// Alias pour l'état partagé
pub type SharedState = Arc<RwLock<RendererState>>;
