use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DeviceCommand {
    Stream { url: String },
    Play,
    Pause,
    Seek { position_sec: f64 },
    Flush,
    Stop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DevicePlaybackState {
    Playing,
    Paused,
    Stopped,
    Buffering,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceStateReport {
    pub position_sec: Option<f64>,
    pub duration_sec: Option<f64>,
    pub playback_state: Option<DevicePlaybackState>,
}

pub trait DeviceAdapter: Send + Sync + 'static {
    fn deliver(&self, command: DeviceCommand);
    fn poll_state(&self) -> Option<DeviceStateReport>;
}

pub struct BrowserAdapter {
    pub state: crate::state::SharedState,
}

impl BrowserAdapter {
    pub fn new(state: crate::state::SharedState) -> Self {
        Self { state }
    }
}

impl DeviceAdapter for BrowserAdapter {
    fn deliver(&self, command: DeviceCommand) {
        self.state.write().push_command(command);
    }

    fn poll_state(&self) -> Option<DeviceStateReport> {
        None
    }
}
