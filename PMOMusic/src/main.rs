use pmoupnp::{mediarenderer::avtransport::AVTTRANSPORT, UpnpObject};
use pmoserver::{
    logs::{log_dump, log_sse, LogState, SseLayer},
    ServerBuilder, Webapp
};
use tracing_subscriber::Registry;
use tracing_subscriber::prelude::*;
use tracing::info;

#[tokio::main]
async fn main() {
    // Charger la config

    let mut server = ServerBuilder::new_configured().build();

    // Ajouter des routes
    server
        .add_route("/hello", || async {
            serde_json::json!({"message": "Hello World"})
        })
        .await;

    server
        .add_route("/info", || async {
            serde_json::json!({"version": "1.0.0"})
        })
        .await;

    server.add_spa::<Webapp>("/app").await;

    // Gère la sortie des logs et sur le serveur SSE pour l'interface web et sur la console
    let log_state = LogState::new(1000);
    let subscriber = Registry::default()
        .with(
            tracing_subscriber::fmt::layer()
                .with_target(true)
                .with_level(true)
                .with_ansi(true), // Couleurs dans le terminal
        )
        .with(SseLayer::new(log_state.clone()));
    tracing::subscriber::set_global_default(subscriber).unwrap();

    server
        .add_handler_with_state("/log-sse", log_sse, log_state.clone())
        .await;
    server
        .add_handler_with_state("/log-dump", log_dump, log_state.clone())
        .await;

    server.add_redirect("/", "/app").await;

    info!("{}",AVTTRANSPORT.to_markdown());
    info!("{}",AVTTRANSPORT.scpd_xml());

    server.start().await;
    server.wait().await;
}
