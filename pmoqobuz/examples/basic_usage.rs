//! Exemple d'utilisation basique de pmoqobuz
//!
//! Cet exemple montre comment :
//! - Se connecter à Qobuz avec les credentials de la configuration
//! - Rechercher des albums
//! - Récupérer les détails d'un album
//! - Exporter un album en format DIDL-Lite

use pmoqobuz::{QobuzClient, ToDIDL};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialiser le logging
    tracing_subscriber::fmt::init();

    println!("=== PMOQobuz - Exemple d'utilisation basique ===\n");

    // Créer un client depuis la configuration
    println!("Connexion à Qobuz...");
    let client = QobuzClient::from_config().await?;

    if let Some(auth_info) = client.auth_info() {
        println!("✓ Connecté avec succès !");
        println!("  User ID: {}", auth_info.user_id);
        if let Some(label) = &auth_info.subscription_label {
            println!("  Abonnement: {}", label);
        }
    }

    println!("\n--- Recherche d'albums ---");
    let query = "Miles Davis";
    println!("Recherche: '{}'...", query);

    let albums = client.search_albums(query).await?;
    println!("✓ {} album(s) trouvé(s)\n", albums.len());

    // Afficher les 5 premiers albums
    for (i, album) in albums.iter().take(5).enumerate() {
        println!("  {}. {} - {}", i + 1, album.artist.name, album.title);
        if let Some(date) = &album.release_date {
            println!("     Date: {}", date);
        }
        if let Some(count) = album.tracks_count {
            println!("     Pistes: {}", count);
        }
    }

    // Récupérer les détails du premier album
    if let Some(first_album) = albums.first() {
        println!("\n--- Détails de l'album ---");
        println!("Album: {} - {}", first_album.artist.name, first_album.title);

        // Récupérer les tracks
        let tracks = client.get_album_tracks(&first_album.id).await?;
        println!("Tracks ({}):", tracks.len());

        for track in tracks.iter().take(3) {
            println!(
                "  {}. {} - {} ({}:{})",
                track.track_number,
                track.display_artist().map(|a| a.name.as_str()).unwrap_or("Unknown"),
                track.title,
                track.duration / 60,
                track.duration % 60
            );
        }

        if tracks.len() > 3 {
            println!("  ... et {} autres pistes", tracks.len() - 3);
        }

        // Export DIDL
        println!("\n--- Export DIDL-Lite ---");
        let didl_container = first_album.to_didl_container("0")?;
        println!("Container ID: {}", didl_container.id);
        println!("Title: {}", didl_container.title);
        println!("Class: {}", didl_container.class);

        if let Some(first_track) = tracks.first() {
            let didl_item = first_track.to_didl_item(&didl_container.id)?;
            println!("\nPremière track en DIDL:");
            println!("  Item ID: {}", didl_item.id);
            println!("  Title: {}", didl_item.title);
            if let Some(artist) = &didl_item.artist {
                println!("  Artist: {}", artist);
            }
        }
    }

    // Afficher les statistiques du cache
    println!("\n--- Statistiques du cache ---");
    let stats = client.cache().stats().await;
    println!("Albums en cache: {}", stats.albums_count);
    println!("Tracks en cache: {}", stats.tracks_count);
    println!("Artistes en cache: {}", stats.artists_count);
    println!("Total: {} entrées", stats.total_count());

    // Favoris
    println!("\n--- Albums favoris ---");
    match client.get_favorite_albums().await {
        Ok(favorites) => {
            println!("✓ {} album(s) favori(s)", favorites.len());
            for (i, album) in favorites.iter().take(5).enumerate() {
                println!("  {}. {} - {}", i + 1, album.artist.name, album.title);
            }
            if favorites.len() > 5 {
                println!("  ... et {} autres", favorites.len() - 5);
            }
        }
        Err(e) => {
            println!("⚠ Impossible de récupérer les favoris: {}", e);
        }
    }

    println!("\n✓ Exemple terminé avec succès !");

    Ok(())
}
