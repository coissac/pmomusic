use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Instant,
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

    /// Returns `true` if we should fetch/process this UDN (either first time or more than half max_age elapsed)
    pub fn should_fetch(registry: Arc<Mutex<UDNRegistry>>, udn: &str, max_age: u64) -> bool {
        let now = Instant::now();
        let mut r = registry.lock().expect("UDNRegistry mutex lock failed");
        if let Some(seen) = r.seen.get_mut(udn) {
            // If more than half the max_age has elapsed, we should fetch/process again
            if now.duration_since(seen.last_seen).as_secs() > max_age / 2 {
                seen.last_seen = now;
                true
            } else {
                // Too soon, skip this SSDP event
                false
            }
        } else {
            // First time seeing this UDN, insert and fetch
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
