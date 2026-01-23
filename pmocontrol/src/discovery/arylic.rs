use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
    time::Duration,
};

use tracing::debug;

use crate::{
    arylic_client::{ARYLIC_TCP_PORT, send_command_required},
    errors::ControlPointError,
    linkplay_client::extract_linkplay_host,
};

static DETECTION_CACHE: OnceLock<Mutex<HashMap<String, bool>>> = OnceLock::new();

fn detection_cache() -> &'static Mutex<HashMap<String, bool>> {
    DETECTION_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Probe whether the renderer at the given location exposes the Arylic TCP API.
pub(crate) fn detect_arylic_tcp(location: &str, timeout: Duration) -> bool {
    let Some(host) = extract_linkplay_host(location) else {
        return false;
    };

    if let Ok(cache) = detection_cache().lock() {
        if let Some(result) = cache.get(&host) {
            return *result;
        }
    }

    let detected = match try_detect_tcp(&host, timeout) {
        Ok(_) => true,
        Err(err) => {
            debug!(
                "Arylic TCP detection failed for {} (host={}): {}",
                location, host, err
            );
            false
        }
    };

    if let Ok(mut cache) = detection_cache().lock() {
        cache.insert(host.to_string(), detected);
    }

    detected
}

fn try_detect_tcp(host: &str, timeout: Duration) -> Result<(), ControlPointError> {
    let payload = send_command_required(
        host,
        ARYLIC_TCP_PORT,
        timeout,
        "MCU+INF+GET",
        &["AXX+INF+", "AXX+DEV+"],
    )?;

    if payload.starts_with("AXX+INF+") || payload.starts_with("AXX+DEV+") {
        Ok(())
    } else {
        Err(ControlPointError::ArilycTcpError(format!(
            "Unexpected INF response from {}: {}",
            host, payload
        )))
    }
}
