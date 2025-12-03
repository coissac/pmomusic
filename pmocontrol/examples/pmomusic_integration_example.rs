//! Exemple d'intégration du Control Point dans PMOMusic
//!
//! Cet exemple montre comment enregistrer le Control Point dans une application
//! PMOMusic complète, en suivant le même pattern que les autres composants.

#[cfg(not(feature = "pmoserver"))]
fn main() {
    eprintln!(
        "This example requires the 'pmoserver' feature. Re-run with `--features pmoserver`."
    );
}

#[cfg(feature = "pmoserver")]
use pmocontrol::ControlPointExt;
#[cfg(feature = "pmoserver")]
use pmoserver::Server;
#[cfg(feature = "pmoserver")]
use tracing::info;

#[cfg(feature = "pmoserver")]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialiser le logging
    tracing_subscriber::fmt::init();

    // ========== PHASE 1 : Infrastructure ==========

    let server = Server::create_upnp_server().await?;

    // ========== PHASE 2 : Enregistrement des composants ==========

    // Enregistrer les devices UPnP, sources musicales, etc.
    // (code existant de PMOMusic...)

    // ========== Enregistrer le Control Point ==========
    //
    // Cette ligne unique :
    // 1. Lance le runtime SSDP et la découverte des devices
    // 2. Démarre le polling des renderers (état, position, volume, etc.)
    // 3. S'abonne aux événements UPnP des serveurs de médias
    // 4. Enregistre toutes les routes REST (/api/control/*)
    // 5. Enregistre tous les endpoints SSE (/api/control/events/*)
    // 6. Génère la documentation OpenAPI

    info!("🎛️  Registering Control Point...");
    let control_point = server
        .write()
        .await
        .register_control_point(5) // timeout de 5 secondes pour les requêtes HTTP
        .await?;

    // Le Control Point est maintenant actif !
    // On peut l'utiliser directement si besoin
    info!("✅ Control Point ready!");

    // Exemple : lister les renderers découverts (optionnel)
    tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
    let renderers = control_point.list_music_renderers();
    info!("📻 Discovered {} renderer(s)", renderers.len());
    for renderer in renderers {
        info!("  - {} ({})", renderer.friendly_name, renderer.id.0);
    }

    // ========== PHASE 3 : Démarrage du serveur ==========

    info!("🌐 Starting HTTP server...");
    server.write().await.start().await;

    info!("✅ PMOMusic is ready!");
    info!("📡 Control Point API available at:");
    info!("   - GET  /api/control/renderers");
    info!("   - GET  /api/control/servers");
    info!("   - GET  /api/control/events (SSE)");
    info!("   - GET  /api/control/events/renderers (SSE)");
    info!("   - GET  /api/control/events/servers (SSE)");
    info!("   - Docs: /swagger-ui/control");
    info!("");
    info!("Press Ctrl+C to stop...");

    server.write().await.wait().await;

    Ok(())
}
