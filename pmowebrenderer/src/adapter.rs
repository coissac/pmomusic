use pmomediarenderer::{
    DeviceAdapter, DeviceCommand, DeviceStateReport, SharedState,
};

pub struct BrowserAdapter {
    pub state: SharedState,
}

impl BrowserAdapter {
    pub fn new(state: SharedState) -> Self {
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
