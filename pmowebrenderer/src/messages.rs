//! Messages WebSocket pour la communication Backend ↔ Navigateur

use serde::{Deserialize, Serialize};

/// Messages envoyés du Backend → Navigateur
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    SessionCreated {
        token: String,
        renderer_info: RendererInfo,
    },
    Command {
        action: TransportAction,
        #[serde(skip_serializing_if = "Option::is_none")]
        params: Option<CommandParams>,
    },
    SetVolume {
        volume: u16,
    },
    SetMute {
        mute: bool,
    },
    Ping,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransportAction {
    Play,
    Pause,
    Stop,
    Seek,
    SetUri,
    SetNextUri,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandParams {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<String>,
}

/// Messages envoyés du Navigateur → Backend
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    Init { capabilities: BrowserCapabilities },
    StateUpdate { state: PlaybackState },
    PositionUpdate { position: String, duration: String },
    MetadataUpdate { metadata: TrackMetadata },
    VolumeUpdate { volume: u16, mute: bool },
    /// Envoyé quand la piste courante se termine naturellement (gapless).
    /// Le backend fait avancer current → next dans l'état partagé.
    TrackEnded,
    Pong,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserCapabilities {
    pub user_agent: String,
    pub supported_formats: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PlaybackState {
    Stopped,
    Playing,
    Paused,
    Transitioning,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackMetadata {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub duration: Option<String>,
    pub album_art_uri: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RendererInfo {
    pub udn: String,
    pub friendly_name: String,
    pub model_name: String,
    pub description_url: String,
}
