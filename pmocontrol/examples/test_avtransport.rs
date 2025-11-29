use std::thread;
use std::time::Duration;

use anyhow::{Context, Result};
use pmocontrol::{
    ControlPoint,
    DeviceRegistryRead,
    DiscoveredEndpoint,
    HttpXmlDescriptionProvider,
};

fn main() -> Result<()> {
    // Optionnel mais pratique si tu as déjà tracing dans la stack
    tracing_subscriber::fmt::init();

    // 1. Créer le control point et lancer la découverte SSDP
    //
    //    timeout_secs = 5 pour les HTTP GET des description.xml
    println!("Starting control point (HTTP timeout = 5s)...");
    let cp = ControlPoint::spawn(5).context("Failed to spawn ControlPoint")?;

    let registry = cp.registry();

    // 2. Laisser un peu de temps à la découverte
    println!("Waiting 5 seconds for UPnP discovery...");
    thread::sleep(Duration::from_secs(5));

    // 3. Lister les renderers connus
    let renderers = {
        let reg = registry.read().expect("DeviceRegistry RwLock poisoned");
        reg.list_renderers()
    };

    if renderers.is_empty() {
        println!("No UPnP renderers discovered.");
        return Ok(());
    }

    println!("Discovered renderers:");
    for (idx, r) in renderers.iter().enumerate() {
        println!(
            "  [{}] {} (model: {}, UDN: {})",
            idx, r.friendly_name, r.model_name, r.udn
        );
    }

    // 4. Choisir un renderer (ici: le premier)
    let renderer = renderers[0].clone();
    println!(
        "\nUsing renderer [0]: {} (model: {}, UDN: {})",
        renderer.friendly_name, renderer.model_name, renderer.udn
    );
    println!("Location: {}", renderer.location);

    // 5. Construire un DiscoveredEndpoint minimal pour ce renderer
    //
    //    On reconstruit un endpoint à partir des infos du registry.
    //    Ça permet de réutiliser HttpXmlDescriptionProvider::build_avtransport_client
    //    qui sait parser description.xml et trouver le service AVTransport.
    let endpoint = DiscoveredEndpoint::new(
        renderer.udn.clone(),
        renderer.location.clone(),
        renderer.server_header.clone(),
        renderer.max_age,
    );

    // 6. Construire un client AVTransport à partir de cet endpoint
    let provider = HttpXmlDescriptionProvider::new(5);
    let client_opt = provider
        .build_avtransport_client(&endpoint)
        .context("Failed to build AVTransport client from device description")?;

    let client = match client_opt {
        Some(c) => c,
        None => {
            println!(
                "Renderer {} has no AVTransport service (or no controlURL).",
                renderer.friendly_name
            );
            return Ok(());
        }
    };

    println!(
        "AVTransport endpoint:\n  service_type = {}\n  control_url  = {}",
        client.service_type, client.control_url
    );

    // 7. Appeler GetTransportInfo(InstanceID=0)
    println!("\nCalling GetTransportInfo (InstanceID = 0)...");
    let info = client
        .get_transport_info(0)
        .context("GetTransportInfo SOAP call failed")?;

    println!("GetTransportInfo result:");
    println!("  CurrentTransportState  = {}", info.current_transport_state);
    println!("  CurrentTransportStatus = {}", info.current_transport_status);
    println!("  CurrentSpeed           = {}", info.current_speed);

    Ok(())
}
