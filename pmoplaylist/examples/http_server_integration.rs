//! Exemple d'intégration avec un serveur HTTP
//!
//! Cet exemple montre comment exposer une playlist FIFO via des endpoints HTTP simples.
//! Dans un vrai MediaServer UPnP, ces endpoints seraient appelés par le protocole ContentDirectory.
//!
//! Pour exécuter :
//! ```bash
//! cargo run -p pmoplaylist --example http_server_integration
//! ```

use pmoplaylist::{DEFAULT_IMAGE, FifoPlaylist, Track};
use std::sync::Arc;

#[tokio::main]
async fn main() {
    println!("=== Intégration HTTP Server ===\n");

    // Créer une playlist partagée
    let playlist = Arc::new(FifoPlaylist::new(
        "my-radio".to_string(),
        "My Internet Radio".to_string(),
        20,
        DEFAULT_IMAGE,
    ));

    println!("📻 Playlist créée: {}", playlist.title().await);
    println!("🆔 ID: {}\n", playlist.id().await);

    // Ajouter quelques tracks initiaux
    println!("📝 Ajout de tracks initiaux...");
    let initial_tracks = vec![
        ("The Beatles", "Come Together", "Abbey Road", 259),
        ("Nirvana", "Smells Like Teen Spirit", "Nevermind", 301),
        ("Queen", "Bohemian Rhapsody", "A Night at the Opera", 354),
    ];

    for (idx, (artist, title, album, duration)) in initial_tracks.iter().enumerate() {
        playlist
            .append_track(
                Track::new(
                    format!("track-{}", idx),
                    *title,
                    format!("http://media.server/music/{}.flac", idx),
                )
                .with_artist(*artist)
                .with_album(*album)
                .with_duration(*duration)
                .with_image(format!("http://media.server/covers/{}.jpg", idx)),
            )
            .await;
        println!("  ✓ {} - {}", artist, title);
    }
    println!();

    // Simuler différents endpoints HTTP

    // 1. GET /playlist/container - Retourne le container DIDL-Lite
    println!("🌐 Endpoint: GET /playlist/container");
    simulate_get_container(playlist.clone()).await;
    println!();

    // 2. GET /playlist/items?offset=0&count=10 - Retourne les items
    println!("🌐 Endpoint: GET /playlist/items?offset=0&count=10");
    simulate_get_items(playlist.clone(), 0, 10).await;
    println!();

    // 3. GET /playlist/metadata - Retourne les métadonnées
    println!("🌐 Endpoint: GET /playlist/metadata");
    simulate_get_metadata(playlist.clone()).await;
    println!();

    // 4. POST /playlist/track - Ajoute un nouveau track
    println!("🌐 Endpoint: POST /playlist/track");
    let new_track = Track::new(
        "track-new-1",
        "Stairway to Heaven",
        "http://media.server/music/stairway.flac",
    )
    .with_artist("Led Zeppelin")
    .with_album("Led Zeppelin IV")
    .with_duration(482);

    simulate_add_track(playlist.clone(), new_track).await;
    println!();

    // 5. DELETE /playlist/oldest - Supprime le plus ancien
    println!("🌐 Endpoint: DELETE /playlist/oldest");
    simulate_delete_oldest(playlist.clone()).await;
    println!();

    // 6. GET /playlist/default-image - Retourne l'image par défaut
    println!("🌐 Endpoint: GET /playlist/default-image");
    simulate_get_default_image(playlist.clone()).await;
    println!();

    // 7. Vérifier l'état final
    println!("📊 État final:");
    let final_items = playlist.get_items(0, 10).await;
    println!("  Total tracks: {}", playlist.len().await);
    println!("  Update ID: {}", playlist.update_id().await);
    println!("\n  Tracks actuels:");
    for (idx, track) in final_items.iter().enumerate() {
        let artist = track.artist.as_deref().unwrap_or("Unknown");
        println!("    {}. {} - {}", idx + 1, artist, track.title);
    }

    println!("\n=== Exemple terminé ===");
}

/// Simule GET /playlist/container
async fn simulate_get_container(playlist: Arc<FifoPlaylist>) {
    let container = playlist.as_container().await;

    println!("  Response (JSON representation):");
    println!("  {{");
    println!("    \"id\": \"{}\",", container.id);
    println!("    \"parentId\": \"{}\",", container.parent_id);
    println!("    \"title\": \"{}\",", container.title);
    println!("    \"class\": \"{}\",", container.class);
    println!(
        "    \"childCount\": {}",
        container.child_count.unwrap_or_default()
    );
    println!("  }}");
}

/// Simule GET /playlist/items?offset=X&count=Y
async fn simulate_get_items(playlist: Arc<FifoPlaylist>, offset: usize, count: usize) {
    let items = playlist
        .as_objects(offset, count, Some("http://media.server/api/default-image"))
        .await;

    println!("  Response: {} items", items.len());
    println!("  [");
    for (idx, item) in items.iter().enumerate() {
        println!("    {{");
        println!("      \"id\": \"{}\",", item.id);
        println!("      \"title\": \"{}\",", item.title);
        println!(
            "      \"artist\": \"{}\",",
            item.artist.as_deref().unwrap_or("")
        );
        println!(
            "      \"album\": \"{}\",",
            item.album.as_deref().unwrap_or("")
        );
        println!("      \"class\": \"{}\",", item.class);
        if !item.resources.is_empty() {
            println!("      \"uri\": \"{}\",", item.resources[0].url);
        }
        print!("    }}");
        if idx < items.len() - 1 {
            println!(",");
        } else {
            println!();
        }
    }
    println!("  ]");
}

/// Simule GET /playlist/metadata
async fn simulate_get_metadata(playlist: Arc<FifoPlaylist>) {
    let update_id = playlist.update_id().await;
    let last_change = playlist.last_change().await;
    let count = playlist.len().await;
    let id = playlist.id().await;
    let title = playlist.title().await;

    println!("  Response:");
    println!("  {{");
    println!("    \"id\": \"{}\",", id);
    println!("    \"title\": \"{}\",", title);
    println!("    \"trackCount\": {},", count);
    println!("    \"updateId\": {},", update_id);
    println!("    \"lastChange\": \"{:?}\"", last_change);
    println!("  }}");
}

/// Simule POST /playlist/track
async fn simulate_add_track(playlist: Arc<FifoPlaylist>, track: Track) {
    let old_update_id = playlist.update_id().await;

    playlist.append_track(track.clone()).await;

    let new_update_id = playlist.update_id().await;

    println!(
        "  Track added: {} - {}",
        track.artist.as_deref().unwrap_or("Unknown"),
        track.title
    );
    println!("  Update ID: {} → {}", old_update_id, new_update_id);
    println!("  Response: 201 Created");
}

/// Simule DELETE /playlist/oldest
async fn simulate_delete_oldest(playlist: Arc<FifoPlaylist>) {
    if let Some(removed) = playlist.remove_oldest().await {
        println!("  Track removed: {} ({})", removed.title, removed.id);
        println!("  New update ID: {}", playlist.update_id().await);
        println!("  Response: 200 OK");
    } else {
        println!("  No tracks to remove");
        println!("  Response: 404 Not Found");
    }
}

/// Simule GET /playlist/default-image
async fn simulate_get_default_image(playlist: Arc<FifoPlaylist>) {
    let image_bytes = playlist.default_image().await;

    println!("  Response:");
    println!("    Content-Type: image/webp");
    println!("    Content-Length: {} bytes", image_bytes.len());
    println!("    Status: 200 OK");
    println!("  (Image WebP {} bytes ready to serve)", image_bytes.len());
}
