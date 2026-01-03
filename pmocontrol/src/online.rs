use std::{
    sync::{Arc, Mutex},
    time::SystemTime,
};

#[derive(Clone, Debug)]
pub struct DeviceConnectionState {
    online: bool,
    last_seen: SystemTime,
    max_age: u32,
}

pub trait DeviceOnline {
    fn is_online(&self) -> bool;
    fn last_seen(&self) -> SystemTime;
    fn has_been_seen_now(&self, max_age: u32);
    fn mark_as_offline(&self);
    fn max_age(&self) -> u32;
}

impl DeviceConnectionState {
    pub fn new() -> Self {
        DeviceConnectionState {
            online: true,
            last_seen: SystemTime::now(),
            max_age: 1800,
        }
    }

    pub fn make() -> Arc<Mutex<Self>> {
        Arc::new(Mutex::new(DeviceConnectionState::new()))
    }

    pub fn is_online(&self) -> bool {
        self.online
    }

    pub fn last_seen(&self) -> SystemTime {
        self.last_seen
    }

    pub fn max_age(&self) -> u32 {
        self.max_age
    }

    pub fn has_been_seen_now(&mut self, max_age: u32) {
        self.last_seen = SystemTime::now();
        self.max_age = max_age;
        self.online = true;
    }

    pub fn mark_as_offline(&mut self) {
        self.online = false;
    }
}
