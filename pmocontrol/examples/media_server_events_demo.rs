use std::collections::HashMap;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::Result;
use crossbeam_channel::RecvTimeoutError;
use pmocontrol::{ControlPoint, MediaServerEvent, ServerId, UpnpMediaServer};

const DISCOVERY_WAIT_SECS: u64 = 5;
const MONITOR_DURATION_SECS: u64 = 90;

fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let cp = ControlPoint::spawn(5)?;
    println!(
        "ControlPoint started; waiting {}s for discovery...",
        DISCOVERY_WAIT_SECS
    );
    thread::sleep(Duration::from_secs(DISCOVERY_WAIT_SECS));

    let servers = cp.list_media_servers();
    if servers.is_empty() {
        println!("No media servers discovered.");
        return Ok(());
    }

    println!("Discovered media servers:");
    for info in &servers {
        println!(
            "  - {} | model={} | udn={} | location={}",
            info.friendly_name, info.model_name, info.udn, info.location
        );
    }

    let mut cache: HashMap<ServerId, UpnpMediaServer> = servers
        .into_iter()
        .map(|info| (info.id.clone(), info))
        .collect();

    let receiver = cp.media_server_events().subscribe();
    let deadline = Instant::now() + Duration::from_secs(MONITOR_DURATION_SECS);
    println!(
        "Listening for ContentDirectory events for {} seconds...",
        MONITOR_DURATION_SECS
    );

    while Instant::now() < deadline {
        match receiver.recv_timeout(Duration::from_millis(500)) {
            Ok(event) => {
                print_event(&cp, &mut cache, &event);
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                println!("Event channel disconnected.");
                break;
            }
        }
    }

    println!("Monitoring finished.");
    Ok(())
}

fn print_event(
    cp: &ControlPoint,
    cache: &mut HashMap<ServerId, UpnpMediaServer>,
    event: &MediaServerEvent,
) {
    match event {
        MediaServerEvent::GlobalUpdated {
            server_id,
            system_update_id,
        } => {
            let label = describe_server(cp, cache, server_id);
            println!(
                "[{}] Global content update (SystemUpdateID={})",
                label,
                system_update_id
                    .map(|id| id.to_string())
                    .unwrap_or_else(|| "<unknown>".into())
            );
        }
        MediaServerEvent::ContainersUpdated {
            server_id,
            container_ids,
        } => {
            let label = describe_server(cp, cache, server_id);
            println!(
                "[{}] Containers updated: {}",
                label,
                if container_ids.is_empty() {
                    "<none>".into()
                } else {
                    container_ids.join(", ")
                }
            );
        }
    }
}

fn describe_server(
    cp: &ControlPoint,
    cache: &mut HashMap<ServerId, UpnpMediaServer>,
    id: &ServerId,
) -> String {
    if let Some(info) = cache.get(id) {
        return format!("{} ({})", info.friendly_name, id.0);
    }
    if let Some(info) = cp.media_server(id) {
        cache.insert(id.clone(), info.clone());
        return format!("{} ({})", info.friendly_name, id.0);
    }
    format!("{} (unknown)", id.0)
}
