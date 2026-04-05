//! Types de messages pour le MediaRenderer

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PlaybackState {
    Stopped,
    Playing,
    Paused,
    Transitioning,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct PlayerStateReport {
    pub position_sec: Option<f64>,
    pub duration_sec: Option<f64>,
    pub state: Option<String>,
    pub ready_state: Option<String>,
}
