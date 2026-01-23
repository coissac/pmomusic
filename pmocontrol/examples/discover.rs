use std::sync::Arc;
use std::thread;
use std::time::Duration;

use pmocontrol::RendererProtocol;
use pmocontrol::{ControlPoint, DeviceRegistryRead, RendererInfo, UpnpMediaServer};

fn main() -> std::io::Result<()> {
    // Un tout petit logging optionnel
    tracing_subscriber::fmt::init();
    tracing::info!("Starting PMOMusic control point SSDP discovery example...");

    // Lance le control point (timeout HTTP pour les descriptions UPnP)
    let cp = ControlPoint::spawn(5)?;

    loop {
        thread::sleep(Duration::from_secs(5));

        // Accès thread-safe au DeviceRegistry
        let reg = cp.registry();
        let reg = reg.read().expect("registry poisoned");

        let renderers: Vec<RendererInfo> = reg.list_renderers();
        let servers: Vec<UpnpMediaServer> = reg.list_servers();

        println!("=====================");
        println!("Renderers detected : {}", renderers.len());
        for r in &renderers {
            let proto = match r.protocol {
                RendererProtocol::UpnpAvOnly => "UPnP AV",
                RendererProtocol::OpenHomeOnly => "OpenHome",
                RendererProtocol::OpenHomeHybrid => "Hybrid",
            };

            println!(
                "- [{}] {} ({}) [{}] online={}",
                r.id.0, r.friendly_name, r.model_name, proto, r.online
            );
        }

        println!();
        println!("Media servers detected : {}", servers.len());
        for s in &servers {
            println!(
                "- [{}] {} ({}) online={}",
                s.id.0, s.friendly_name, s.model_name, s.online
            );
        }

        println!("=====================");
    }
}
