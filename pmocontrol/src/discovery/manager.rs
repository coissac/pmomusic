use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Instant, SystemTime},
};

struct UDNSeen {
    max_age: u64,
    last_seen: Instant,
}

pub struct UDNRegistry {
    seen: HashMap<String, UDNSeen>,
}

impl UDNRegistry {
    pub fn new() -> Self {
        UDNRegistry {
            seen: HashMap::new(),
        }
    }

    /// Returns `true` if the UDN has been seen for at least half of its lifetime
    pub fn should_fetch(registry: Arc<Mutex<UDNRegistry>>, udn: &str, max_age: u64) -> bool {
        let now = Instant::now();
        let mut r = registry.lock().expect("UDNRegistry mutex lock failed");
        if let Some(seen) = r.seen.get_mut(udn) {
            if now.duration_since(seen.last_seen).as_secs() > max_age / 2 {
                false
            } else {
                seen.last_seen = now;
                true
            }
        } else {
            r.seen.insert(
                udn.to_string(),
                UDNSeen {
                    max_age,
                    last_seen: now,
                },
            );
            true
        }
    }
}
