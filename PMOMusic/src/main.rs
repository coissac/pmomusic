use pmoupnp::{
    ssdp::SsdpServer,
    upnp_api::UpnpApiExt,
    UpnpServer,
};
use pmomediarenderer::MEDIA_RENDERER;
use pmoserver::{
    logs::LoggingOptions,
    ServerBuilder
};
use pmoapp::{Webapp, WebAppExt};
use pmocovers::CoverCacheExt;
use tracing::info;

#[tokio::main]
async fn main() {
    // Créer le serveur - le trait UpnpServer étend automatiquement Server
    let mut server = ServerBuilder::new_configured().build();

    // Initialiser le logging et enregistrer les routes de logs
    server.init_logging().await;


    info!("📡 Registering the cover cache...");
    let cache = server.init_cover_cache_configured()
    .await
    .expect("Cannot initialise the image cache");

    info!("✅ Cover cache ready at {}",
        cache.cache_dir(),
    );



    // Routes de base
    server
        .add_route("/info", || async {
            serde_json::json!({"version": "1.0.0"})
        })
        .await;


    // Ajouter la webapp via le trait WebAppExt
    info!("📡 Registering Web application...");
    server.add_webapp_with_redirect::<Webapp>("/app").await;

    // Enregistrer l'API d'introspection UPnP
    server.register_upnp_api().await;

    info!("📡 Registering MediaRenderer...");
    let renderer_instance = server.register_device(MEDIA_RENDERER.clone())
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

    // Créer et enregistrer le device SSDP pour le MediaRenderer
    let ssdp_device = renderer_instance
        .to_ssdp_device("PMOMusic", "1.0");
    ssdp_server.add_device(ssdp_device);
    info!("✅ SSDP announcements sent for MediaRenderer");

    server.start().await;
    server.wait().await;
}
