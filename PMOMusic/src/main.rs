use pmoupnp::{
    mediarenderer::MEDIA_RENDERER,
    ssdp::{SsdpDevice, SsdpServer},
    UpnpModel,
};
use pmoserver::{
    logs::{log_dump, log_sse, LogState, SseLayer},
    ServerBuilder
};
use pmoapp::Webapp;
use tracing_subscriber::Registry;
use tracing_subscriber::prelude::*;
use tracing::info;

#[tokio::main]
async fn main() {
    // Initialiser le logging d'abord
    let log_state = LogState::new(1000);
    let subscriber = Registry::default()
        .with(
            tracing_subscriber::fmt::layer()
                .with_target(true)
                .with_level(true)
                .with_ansi(true),
        )
        .with(SseLayer::new(log_state.clone()));
    tracing::subscriber::set_global_default(subscriber).unwrap();

    // Créer le serveur
    let mut server = ServerBuilder::new_configured().build();

    // Routes de base
    server
        .add_route("/info", || async {
            serde_json::json!({"version": "1.0.0"})
        })
        .await;

    server.add_spa::<Webapp>("/app").await;

    // Routes de logging
    server
        .add_handler_with_state("/log-sse", log_sse, log_state.clone())
        .await;
    server
        .add_handler_with_state("/log-dump", log_dump, log_state.clone())
        .await;

    server.add_redirect("/", "/app").await;

    // Créer et enregistrer le MediaRenderer
    info!("🎵 Creating MediaRenderer instance...");
    let renderer_instance = MEDIA_RENDERER.create_instance();

    // Créer et ajouter les instances de services
    for service in MEDIA_RENDERER.services().iter() {
        let service_instance = service.create_instance();
        renderer_instance
            .add_service(service_instance)
            .expect("Failed to add service to renderer");
    }

    info!("📡 Registering MediaRenderer routes...");
    renderer_instance
        .register_urls(&mut server)
        .await
        .expect("Failed to register MediaRenderer routes");

    info!("✅ MediaRenderer ready at {}{}",
        renderer_instance.base_url(),
        renderer_instance.description_route()
    );

    // Créer et démarrer le serveur SSDP
    info!("📡 Starting SSDP discovery...");
    let mut ssdp_server = SsdpServer::new();
    ssdp_server.start().expect("Failed to start SSDP server");

    // Créer le device SSDP pour le MediaRenderer
    let location = format!("{}{}",
        renderer_instance.base_url(),
        renderer_instance.description_route()
    );

    let mut ssdp_device = SsdpDevice::new(
        renderer_instance.udn().to_string(),
        MEDIA_RENDERER.device_type(),
        location,
        format!("Linux/5.0 UPnP/1.1 PMOMusic/1.0"),
    );

    // Ajouter les types de notification pour chaque service
    for service in renderer_instance.services() {
        ssdp_device.add_notification_type(service.service_type());
    }

    // Enregistrer le device et envoyer les annonces SSDP
    ssdp_server.add_device(ssdp_device);
    info!("✅ SSDP announcements sent for MediaRenderer");

    server.start().await;
    server.wait().await;
}
