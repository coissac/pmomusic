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

