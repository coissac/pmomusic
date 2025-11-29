use std::io;
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::Duration;

use pmoupnp::ssdp::SsdpClient;

use crate::discovery::DiscoveryManager;
use crate::provider::HttpXmlDescriptionProvider;
use crate::registry::{DeviceRegistry, DeviceUpdate};

/// Control point minimal :
/// - lance un SsdpClient dans un thread,
/// - passe les SsdpEvent au DiscoveryManager,
/// - applique les DeviceUpdate dans le DeviceRegistry.
pub struct ControlPoint {
    registry: Arc<RwLock<DeviceRegistry>>,
}

impl ControlPoint {
    /// Crée un ControlPoint et lance le thread de découverte SSDP.
    ///
    /// `timeout_secs` : timeout HTTP pour la récupération des descriptions UPnP.
    pub fn spawn(timeout_secs: u64) -> io::Result<Self> {
        let registry = Arc::new(RwLock::new(DeviceRegistry::new()));

        // SsdpClient
        let client = SsdpClient::new()?; // pmoupnp::ssdp::SsdpClient

        // Arc utilisé dans le thread
        let registry_for_thread = Arc::clone(&registry);

        // Thread de découverte
        thread::spawn(move || {
            // Provider HTTP+XML et DiscoveryManager VIVENT dans le thread
            let provider = HttpXmlDescriptionProvider::new(timeout_secs);
            let mut discovery = DiscoveryManager::new(provider);

            // ACTIVE DISCOVERY : envoyer quelques M-SEARCH au démarrage
            // pour forcer les devices à répondre rapidement.
            let search_targets = [
                "ssdp:all",
                "urn:schemas-upnp-org:device:MediaRenderer:1",
                "urn:av-openhome-org:device:MediaRenderer:1",
                "urn:schemas-upnp-org:device:MediaServer:1",
            ];

            for st in &search_targets {
                if let Err(e) = client.send_msearch(st, 3) {
                    eprintln!("Failed to send M-SEARCH for {}: {}", st, e);
                }
                std::thread::sleep(Duration::from_millis(200));
            }

            // La closure passée à run_event_loop capture discovery par mutable borrow
            // => FnMut, ce que SsdpClient::run_event_loop accepte.
            client.run_event_loop(move |event| {
                let updates: Vec<DeviceUpdate> = discovery.handle_ssdp_event(event);

                if updates.is_empty() {
                    return;
                }

                if let Ok(mut reg) = registry_for_thread.write() {
                    for update in updates {
                        reg.apply_update(update);
                    }
                }
            });
        });

        Ok(Self { registry })
    }

    /// Accès au DeviceRegistry partagé.
    pub fn registry(&self) -> Arc<RwLock<DeviceRegistry>> {
        Arc::clone(&self.registry)
    }
}
