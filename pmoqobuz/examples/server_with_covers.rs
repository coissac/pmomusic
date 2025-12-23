//! Exemple d'utilisation de pmoqobuz avec pmoserver et pmocovers
//!
//! Cet exemple montre comment :
//! - Créer un serveur HTTP avec pmoserver
//! - Initialiser le cache d'images avec pmocovers
//! - Initialiser le client Qobuz avec intégration pmocovers
//! - Les images d'albums sont automatiquement mises en cache
//!
//! Pour tester :
//! ```bash
//! cargo run --example server_with_covers --features "pmoserver"
//! ```
//!
//! Endpoints disponibles :
//! - GET /qobuz/search?q=query&type=albums - Recherche d'albums (images auto-cachées)
//! - GET /qobuz/albums/{id} - Détails d'un album (image auto-cachée)
//! - GET /qobuz/favorites/albums - Albums favoris (images auto-cachées)
//! - GET /covers/images/{pk} - Image originale mise en cache
//! - GET /covers/images/{pk}/{size} - Variante redimensionnée
//! - GET /api/covers - API REST du cache d'images
//! - GET /swagger-ui - Documentation interactive

#[cfg(feature = "pmoserver")]
use pmocovers::CoverCacheExt;

#[cfg(feature = "pmoserver")]
use pmoqobuz::QobuzServerExt;

#[cfg(feature = "pmoserver")]
use pmoserver::ServerBuilder;

#[cfg(feature = "pmoserver")]
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialiser le logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    println!("=== PMOQobuz + PMOCovers - Serveur HTTP avec cache d'images ===\n");

    // Créer le serveur depuis la configuration
    let mut server = ServerBuilder::new_configured().build();

    println!("1. Initialisation du cache d'images (pmocovers)...");
    // Initialiser le cache d'images avec la configuration
    let cache = server.init_cover_cache_configured().await?;
    println!("   ✓ Cache d'images initialisé: {}", cache.cache_dir());

    println!("\n2. Initialisation du client Qobuz avec intégration pmocovers...");
    // Initialiser le client Qobuz avec intégration pmocovers
    // Les images d'albums seront automatiquement ajoutées au cache
    let client = server
        .init_qobuz_client_configured_with_covers(cache.clone())
        .await?;

    if let Some(auth_info) = client.auth_info() {
        println!("   ✓ Client Qobuz connecté !");
        println!("     User ID: {}", auth_info.user_id);
        if let Some(label) = &auth_info.subscription_label {
            println!("     Abonnement: {}", label);
        }
    }

    println!("\n3. Démarrage du serveur HTTP...");
    server.start().await;

    println!("\n✓ Serveur démarré avec succès !\n");
    println!("Endpoints disponibles :");
    println!("  • Qobuz API:");
    println!("    - GET /qobuz/search?q=query&type=albums");
    println!("    - GET /qobuz/albums/{{id}}");
    println!("    - GET /qobuz/albums/{{id}}/tracks");
    println!("    - GET /qobuz/favorites/albums");
    println!("    - GET /qobuz/favorites/artists");
    println!("    - GET /qobuz/cache/stats");
    println!("  • Images (auto-cachées depuis Qobuz):");
    println!("    - GET /covers/images/{{pk}}");
    println!("    - GET /covers/images/{{pk}}/{{size}}");
    println!("  • API REST du cache:");
    println!("    - GET /api/covers");
    println!("    - POST /api/covers");
    println!("    - DELETE /api/covers/{{pk}}");
    println!("  • Documentation:");
    println!("    - GET /swagger-ui");
    println!("\nExemple de requête :");
    println!("  curl 'http://localhost:3000/qobuz/search?q=Miles%20Davis&type=albums' | jq '.[0].image_cached'");
    println!("  # Retourne: \"/covers/images/{{pk}}\"");
    println!("\nAppuyez sur Ctrl+C pour arrêter le serveur...\n");

    // Attendre indéfiniment
    server.wait().await;

    Ok(())
}

#[cfg(not(feature = "pmoserver"))]
fn main() {
    eprintln!("Cet exemple nécessite la feature 'pmoserver'");
    eprintln!("Exécutez: cargo run --example server_with_covers --features \"pmoserver\"");
    std::process::exit(1);
}
