use std::thread;
use std::time::Duration;

use pmoupnp::ssdp::{SsdpClient, SsdpEvent};

fn main() -> std::io::Result<()> {
    tracing_subscriber::fmt::init();
    tracing::info!("Starting raw SSDP dump helper...");

    let client = SsdpClient::new()?;

    // Envoie quelques requêtes M-SEARCH ciblées pour accélérer les réponses.
    let search_targets = [
        "ssdp:all",
        "urn:schemas-upnp-org:device:MediaRenderer:1",
        "urn:av-openhome-org:device:MediaRenderer:1",
        "urn:schemas-upnp-org:device:MediaServer:1",
    ];
    for st in &search_targets {
        if let Err(err) = client.send_msearch(st, 3) {
            eprintln!("Failed to send M-SEARCH for {}: {}", st, err);
        }
        thread::sleep(Duration::from_millis(200));
    }

    println!("Listening for SSDP events. Press Ctrl+C to stop.");

    client.run_event_loop(|event| match event {
        SsdpEvent::Alive {
            usn,
            nt,
            location,
            server,
            max_age,
            from,
        } => {
            println!(
                "[ALIVE] from={} usn={} nt={} location={} server={} max_age={}",
                from, usn, nt, location, server, max_age
            );
        }
        SsdpEvent::SearchResponse {
            usn,
            st,
            location,
            server,
            max_age,
            from,
        } => {
            println!(
                "[SEARCH RESPONSE] from={} usn={} st={} location={} server={} max_age={}",
                from, usn, st, location, server, max_age
            );
        }
        SsdpEvent::ByeBye { usn, nt, from } => {
            println!("[BYEBYE] from={} usn={} nt={}", from, usn, nt);
        }
    })
}
