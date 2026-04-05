//! État partagé du renderer (backend ↔ pipeline)

use parking_lot::RwLock;
use std::collections::VecDeque;
use std::sync::Arc;

use crate::adapter::DeviceCommand;
use crate::messages::PlaybackState;

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
    pub pending_commands: VecDeque<DeviceCommand>,
}

impl RendererState {
    pub fn push_command(&mut self, cmd: DeviceCommand) {
        self.pending_commands.push_back(cmd);
    }

    pub fn pop_command(&mut self) -> Option<DeviceCommand> {
        self.pending_commands.pop_front()
    }
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
            pending_commands: VecDeque::new(),
        }
    }
}

pub type SharedState = Arc<RwLock<RendererState>>;
