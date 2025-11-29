use pmoapp::{WebAppExt, Webapp};
use pmomediarenderer::MEDIA_RENDERER;
use pmomediaserver::{
    MEDIA_SERVER, MediaServerDeviceExt, ParadiseStreamingExt, sources::SourcesExt,
};
use pmoserver::Server;
use pmosource::MusicSourceExt;
use pmoupnp::UpnpServerExt;
use tracing::info;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // ========== PHASE 1 : Infrastructure UPnP ==========
    // #[cfg(tokio_unstable)]
    // console_subscriber::init();

    let server = Server::create_upnp_server().await?; // Routes personnalisées de l'application
    server
        .write()
        .await
        .add_route("/info", || async {
            serde_json::json!({"version": "1.0.0"})
        })
        .await;

    // Initialiser le système de gestion des sources musicales avec API REST
    info!("📡 Initializing music sources management system...");
    server
        .write()
        .await
        .init_music_sources()
        .await
        .expect("Failed to initialize music sources API");

    // ========== PHASE 2 : Configuration métier ==========

    // Enregistrer les sources musicales
    info!("🎵 Registering music sources...");

    // // Enregistrer Qobuz
    // if let Err(e) = server.write().await.register_qobuz().await {
    //     tracing::warn!("⚠️ Failed to register Qobuz: {}", e);
    // }

    // Initialiser les canaux de streaming Radio Paradise (pipelines + routes HTTP)
    info!("📻 Initializing Radio Paradise streaming channels...");
    if let Err(e) = server.write().await.init_paradise_streaming().await {
        tracing::warn!("⚠️ Failed to initialize Paradise streaming: {}", e);
    } else {
        // Enregistrer la source Radio Paradise UPnP (inclut l'initialisation de l'API)
        if let Err(e) = server.write().await.register_paradise().await {
            tracing::warn!("⚠️ Failed to register Radio Paradise source: {}", e);
        }
    }

    // Lister toutes les sources enregistrées
    let sources = server.read().await.list_music_sources().await;
    info!("✅ {} music source(s) registered", sources.len());
    for source in sources {
        info!("  - {} ({})", source.name(), source.id());
    }

    // Enregistrer les devices UPnP (HTTP + SSDP automatique)
    info!("📡 Registering UPnP devices...");

    let renderer_instance = server
        .write()
        .await
        .register_device(MEDIA_RENDERER.clone())
        .await
        .expect("Failed to register MediaRenderer");

    info!(
        "✅ MediaRenderer ready at {}{}",
        renderer_instance.base_url(),
        renderer_instance.description_route()
    );

    let server_instance = server
        .write()
        .await
        .register_device(MEDIA_SERVER.clone())
        .await
        .expect("Failed to register MediaServer");

    // Enregistrer l'instance ContentDirectory pour les notifications GENA
    if let Some(cd_service) = server_instance.get_service("ContentDirectory") {
        pmomediaserver::contentdirectory::state::register_instance(&cd_service);
    }

    // Initialiser les ProtocolInfo du MediaServer
    server_instance.init_protocol_info();

    info!(
        "✅ MediaServer ready at {}{}",
        server_instance.base_url(),
        server_instance.description_route()
    );

    // Ajouter la webapp via le trait WebAppExt
    info!("📡 Registering Web application...");
    server
        .write()
        .await
        .add_webapp_with_redirect::<Webapp>("/app")
        .await;

    // ========== PHASE 3 : Démarrage du serveur ==========

    info!("🌐 Starting HTTP server...");
    server.write().await.start().await;

    info!("✅ PMOMusic is ready!");
    info!("Press Ctrl+C to stop...");
    server.write().await.wait().await;

    Ok(())
}
