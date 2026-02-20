use pmoapp::{WebAppExt, Webapp};
use pmocontrol::ControlPointExt;
use pmomediarenderer::MEDIA_RENDERER;
use pmomediaserver::{
    MEDIA_SERVER, MediaServerDeviceExt, ParadiseStreamingExt, sources::SourcesExt,
};
use pmoserver::Server;
use pmosource::MusicSourceExt;
use pmoupnp::UpnpServerExt;
use pmowebrenderer::WebRendererExt;
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

    // Enregistrer Qobuz pour activer les lazy providers (QOBUZ:PK)
    if let Err(e) = server.write().await.register_qobuz().await {
        tracing::warn!("⚠️ Failed to register Qobuz source: {}", e);
    }

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

    // Enregistrer la source Radio France (inclut l'initialisation des routes API)
    info!("📻 Registering Radio France source...");
    if let Err(e) = server.write().await.register_radiofrance().await {
        tracing::warn!("⚠️ Failed to register Radio France source: {}", e);
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

    let base_url = renderer_instance.base_url();
    let desc_route = renderer_instance.description_route();
    info!("✅ MediaRenderer ready at {}{}", base_url, desc_route);

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

    // Enregistrer le Control Point (découverte renderers/serveurs + API REST + SSE)
    info!("🎛️  Registering Control Point...");
    let control_point = server
        .write()
        .await
        .register_control_point(5)
        .await
        .expect("Failed to register Control Point");

    // Enregistrer le WebRenderer (endpoint WebSocket pour renderers navigateur)
    info!("🌐 Registering WebRenderer...");
    server
        .write()
        .await
        .register_web_renderer(control_point)
        .await
        .expect("Failed to register WebRenderer");

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

    // Extraire le join_handle AVANT de libérer le write lock,
    // pour pouvoir l'awaiter sans tenir le write lock du serveur global.
    // (Tenir le write lock pendant wait() bloquerait register_device() dynamique)
    let join_handle = server.write().await.take_join_handle();
    if let Some(h) = join_handle {
        let _ = h.await;
    }

    // Le serveur HTTP est arrêté, mais des threads (ControlPoint, etc.) peuvent encore tourner
    // Attendre 2 secondes pour laisser le temps aux threads de se terminer
    info!("Waiting for background threads to finish...");
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    // Forcer l'arrêt du processus (les threads du ControlPoint tournent en boucle infinie)
    info!("✅ PMOMusic stopped");
    std::process::exit(0);
}
